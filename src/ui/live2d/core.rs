use std::path::PathBuf;

#[cfg(not(windows))]
use std::path::Path;
#[cfg(not(windows))]
use std::sync::Arc;

use super::error::Live2dError;

#[derive(Debug, Clone, Default)]
pub struct RuntimeOptions {
    pub core_library: Option<PathBuf>,
    pub sdk_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreInfo {
    pub library_path: PathBuf,
    pub packed_version: u32,
    pub version: String,
    pub latest_moc_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreModelInfo {
    pub moc_path: PathBuf,
    pub moc_version: u32,
    pub model_memory_bytes: u32,
}

#[cfg(windows)]
mod platform {
    use std::alloc::{alloc_zeroed, dealloc, Layout};
    use std::collections::BTreeSet;
    use std::env;
    use std::ffi::c_void;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::ptr::NonNull;
    use std::sync::Arc;

    use libloading::Library;

    use super::{CoreInfo, CoreModelInfo, Live2dError, RuntimeOptions};

    const MOC_ALIGNMENT: usize = 64;
    const MODEL_ALIGNMENT: usize = 16;
    const CORE_DLL: &str = "Live2DCubismCore.dll";

    type GetVersion = unsafe extern "system" fn() -> u32;
    type GetLatestMocVersion = unsafe extern "system" fn() -> u32;
    type GetMocVersion = unsafe extern "system" fn(*const c_void, u32) -> u32;
    type HasMocConsistency = unsafe extern "system" fn(*const c_void, u32) -> i32;
    type ReviveMocInPlace = unsafe extern "system" fn(*mut c_void, u32) -> *mut c_void;
    type GetSizeofModel = unsafe extern "system" fn(*const c_void) -> u32;
    type InitializeModelInPlace =
        unsafe extern "system" fn(*const c_void, *mut c_void, u32) -> *mut c_void;
    type UpdateModel = unsafe extern "system" fn(*mut c_void);

    struct CoreApi {
        get_moc_version: GetMocVersion,
        has_moc_consistency: HasMocConsistency,
        revive_moc_in_place: ReviveMocInPlace,
        get_sizeof_model: GetSizeofModel,
        initialize_model_in_place: InitializeModelInPlace,
        update_model: UpdateModel,
    }

    pub(crate) struct CoreLibrary {
        _library: Library,
        api: CoreApi,
        info: CoreInfo,
    }

    pub(crate) struct LoadedCoreModel {
        model: NonNull<c_void>,
        _model_memory: AlignedBuffer,
        _moc_memory: AlignedBuffer,
        core: Arc<CoreLibrary>,
        info: CoreModelInfo,
    }

    impl CoreLibrary {
        pub(crate) fn load(options: &RuntimeOptions) -> Result<Arc<Self>, Live2dError> {
            let candidates = core_library_candidates(options);
            let mut attempts = Vec::new();

            for candidate in &candidates {
                let library = match unsafe { Library::new(candidate) } {
                    Ok(library) => library,
                    Err(error) => {
                        attempts.push(format!("{}: {error}", candidate.display()));
                        continue;
                    }
                };
                return unsafe { Self::from_library(library, candidate.clone()).map(Arc::new) };
            }

            Err(Live2dError::CoreUnavailable {
                message: core_unavailable_message(&candidates, &attempts),
            })
        }

        unsafe fn from_library(
            library: Library,
            library_path: PathBuf,
        ) -> Result<Self, Live2dError> {
            let get_version: GetVersion =
                load_symbol(&library, &library_path, b"csmGetVersion\0", "csmGetVersion")?;
            let get_latest_moc_version: GetLatestMocVersion = load_symbol(
                &library,
                &library_path,
                b"csmGetLatestMocVersion\0",
                "csmGetLatestMocVersion",
            )?;
            let api = CoreApi {
                get_moc_version: load_symbol(
                    &library,
                    &library_path,
                    b"csmGetMocVersion\0",
                    "csmGetMocVersion",
                )?,
                has_moc_consistency: load_symbol(
                    &library,
                    &library_path,
                    b"csmHasMocConsistency\0",
                    "csmHasMocConsistency",
                )?,
                revive_moc_in_place: load_symbol(
                    &library,
                    &library_path,
                    b"csmReviveMocInPlace\0",
                    "csmReviveMocInPlace",
                )?,
                get_sizeof_model: load_symbol(
                    &library,
                    &library_path,
                    b"csmGetSizeofModel\0",
                    "csmGetSizeofModel",
                )?,
                initialize_model_in_place: load_symbol(
                    &library,
                    &library_path,
                    b"csmInitializeModelInPlace\0",
                    "csmInitializeModelInPlace",
                )?,
                update_model: load_symbol(
                    &library,
                    &library_path,
                    b"csmUpdateModel\0",
                    "csmUpdateModel",
                )?,
            };
            let packed_version = get_version();
            let latest_moc_version = get_latest_moc_version();
            Ok(Self {
                _library: library,
                api,
                info: CoreInfo {
                    library_path,
                    packed_version,
                    version: format_core_version(packed_version),
                    latest_moc_version,
                },
            })
        }

        pub(crate) fn info(&self) -> &CoreInfo {
            &self.info
        }

        pub(crate) fn load_model(
            self: &Arc<Self>,
            moc_path: &Path,
        ) -> Result<LoadedCoreModel, Live2dError> {
            let bytes = fs::read(moc_path).map_err(|source| Live2dError::Io {
                path: moc_path.to_path_buf(),
                source,
            })?;
            let size = u32::try_from(bytes.len()).map_err(|_| Live2dError::MocRejected {
                moc: moc_path.to_path_buf(),
                message: "file is larger than the Cubism Core 32-bit size limit".to_string(),
            })?;
            if size == 0 {
                return Err(rejected(moc_path, "file is empty"));
            }

            let mut moc_memory = AlignedBuffer::from_bytes(&bytes, MOC_ALIGNMENT)
                .ok_or_else(|| rejected(moc_path, "failed to allocate aligned MOC memory"))?;
            let consistent =
                unsafe { (self.api.has_moc_consistency)(moc_memory.as_ptr(), size) } != 0;
            let moc_version = unsafe { (self.api.get_moc_version)(moc_memory.as_ptr(), size) };
            if !consistent {
                return Err(rejected(
                    moc_path,
                    format!(
                        "csmHasMocConsistency failed (moc version {moc_version}, Core supports through {})",
                        self.info.latest_moc_version
                    ),
                ));
            }
            if moc_version == 0 || moc_version > self.info.latest_moc_version {
                return Err(rejected(
                    moc_path,
                    format!(
                        "unsupported moc version {moc_version}; loaded Core supports through {}",
                        self.info.latest_moc_version
                    ),
                ));
            }

            let moc = unsafe { (self.api.revive_moc_in_place)(moc_memory.as_mut_ptr(), size) };
            let moc = NonNull::new(moc)
                .ok_or_else(|| rejected(moc_path, "csmReviveMocInPlace returned null"))?;
            let model_size = unsafe { (self.api.get_sizeof_model)(moc.as_ptr()) };
            if model_size == 0 {
                return Err(rejected(moc_path, "csmGetSizeofModel returned zero"));
            }
            let mut model_memory = AlignedBuffer::zeroed(model_size as usize, MODEL_ALIGNMENT)
                .ok_or_else(|| rejected(moc_path, "failed to allocate aligned model memory"))?;
            let model = unsafe {
                (self.api.initialize_model_in_place)(
                    moc.as_ptr(),
                    model_memory.as_mut_ptr(),
                    model_size,
                )
            };
            let model = NonNull::new(model)
                .ok_or_else(|| rejected(moc_path, "csmInitializeModelInPlace returned null"))?;
            unsafe { (self.api.update_model)(model.as_ptr()) };

            Ok(LoadedCoreModel {
                model,
                _model_memory: model_memory,
                _moc_memory: moc_memory,
                core: Arc::clone(self),
                info: CoreModelInfo {
                    moc_path: moc_path.to_path_buf(),
                    moc_version,
                    model_memory_bytes: model_size,
                },
            })
        }
    }

    impl LoadedCoreModel {
        pub(crate) fn info(&self) -> &CoreModelInfo {
            &self.info
        }

        pub(crate) fn update(&mut self) {
            unsafe { (self.core.api.update_model)(self.model.as_ptr()) };
        }
    }

    unsafe fn load_symbol<T: Copy>(
        library: &Library,
        library_path: &Path,
        symbol: &'static [u8],
        symbol_name: &'static str,
    ) -> Result<T, Live2dError> {
        library
            .get::<T>(symbol)
            .map(|value| *value)
            .map_err(|error| Live2dError::CoreSymbol {
                library: library_path.to_path_buf(),
                symbol: symbol_name,
                message: error.to_string(),
            })
    }

    fn core_library_candidates(options: &RuntimeOptions) -> Vec<PathBuf> {
        if let Some(explicit) = &options.core_library {
            return vec![explicit.clone()];
        }

        let mut candidates = Vec::new();
        if let Some(path) = env::var_os("CUBISM_CORE_DLL") {
            candidates.push(PathBuf::from(path));
        }

        let mut sdk_roots = Vec::new();
        if let Some(root) = &options.sdk_root {
            sdk_roots.push(root.clone());
        }
        for key in [
            "CUBISM_SDK_NATIVE_DIR",
            "CUBISM_SDK_ROOT",
            "CUBISM_SDK_DIR",
            "LIVE2D_CUBISM_SDK_NATIVE_DIR",
            "LIVE2D_CUBISM_SDK_ROOT",
            "LIVE2D_CUBISM_SDK_DIR",
        ] {
            if let Some(root) = env::var_os(key) {
                sdk_roots.push(PathBuf::from(root));
            }
        }
        if let Some(directory) = env::var_os("CUBISM_CORE_LIB_DIR") {
            candidates.push(PathBuf::from(directory).join(CORE_DLL));
        }
        let architecture = if cfg!(target_pointer_width = "64") {
            "x86_64"
        } else {
            "x86"
        };
        for root in sdk_roots {
            candidates.push(
                root.join("Core")
                    .join("dll")
                    .join("windows")
                    .join(architecture)
                    .join(CORE_DLL),
            );
        }

        if let Ok(executable) = env::current_exe() {
            if let Some(directory) = executable.parent() {
                candidates.push(directory.join(CORE_DLL));
            }
        }
        candidates.push(PathBuf::from(CORE_DLL));

        let mut unique = BTreeSet::new();
        candidates
            .into_iter()
            .filter(|path| unique.insert(path.clone()))
            .collect()
    }

    fn core_unavailable_message(candidates: &[PathBuf], attempts: &[String]) -> String {
        let searched = candidates
            .iter()
            .map(|path| format!("  - {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        let details = if attempts.is_empty() {
            String::new()
        } else {
            format!("\nLoader errors:\n{}", attempts.join("\n"))
        };
        format!(
            "Live2D Cubism Core is unavailable. Download Cubism SDK for Native from Live2D, then either set CUBISM_CORE_DLL to Core/dll/windows/x86_64/{CORE_DLL}, set CUBISM_SDK_NATIVE_DIR to the SDK root, or place {CORE_DLL} beside the executable.\nSearched:\n{searched}{details}"
        )
    }

    fn format_core_version(version: u32) -> String {
        format!(
            "{}.{}.{}",
            version >> 24,
            (version >> 16) & 0xff,
            version & 0xffff
        )
    }

    fn rejected(path: &Path, message: impl Into<String>) -> Live2dError {
        Live2dError::MocRejected {
            moc: path.to_path_buf(),
            message: message.into(),
        }
    }

    struct AlignedBuffer {
        pointer: NonNull<u8>,
        layout: Layout,
    }

    impl AlignedBuffer {
        fn zeroed(size: usize, alignment: usize) -> Option<Self> {
            let layout = Layout::from_size_align(size, alignment).ok()?;
            let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })?;
            Some(Self { pointer, layout })
        }

        fn from_bytes(bytes: &[u8], alignment: usize) -> Option<Self> {
            let mut buffer = Self::zeroed(bytes.len(), alignment)?;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    buffer.as_mut_ptr().cast::<u8>(),
                    bytes.len(),
                );
            }
            Some(buffer)
        }

        fn as_ptr(&self) -> *const c_void {
            self.pointer.as_ptr().cast()
        }

        fn as_mut_ptr(&mut self) -> *mut c_void {
            self.pointer.as_ptr().cast()
        }
    }

    impl Drop for AlignedBuffer {
        fn drop(&mut self) {
            unsafe { dealloc(self.pointer.as_ptr(), self.layout) };
        }
    }

    #[cfg(test)]
    mod tests {
        use std::path::PathBuf;

        use super::CoreLibrary;
        use crate::ui::live2d::core::RuntimeOptions;

        #[test]
        fn missing_explicit_core_has_actionable_error() {
            let missing = PathBuf::from(r"Z:\definitely-missing\Live2DCubismCore.dll");
            let error = CoreLibrary::load(&RuntimeOptions {
                core_library: Some(missing.clone()),
                sdk_root: None,
            })
            .err()
            .expect("missing DLL must fail");
            let message = error.to_string();

            assert!(message.contains("CUBISM_CORE_DLL"));
            assert!(message.contains("CUBISM_SDK_NATIVE_DIR"));
            assert!(message.contains(&missing.display().to_string()));
        }
    }
}

#[cfg(windows)]
pub(crate) use platform::{CoreLibrary, LoadedCoreModel};

#[cfg(not(windows))]
pub(crate) struct CoreLibrary;

#[cfg(not(windows))]
pub(crate) struct LoadedCoreModel;

#[cfg(not(windows))]
impl CoreLibrary {
    pub(crate) fn load(_options: &RuntimeOptions) -> Result<Arc<Self>, Live2dError> {
        Err(Live2dError::UnsupportedPlatform)
    }

    pub(crate) fn info(&self) -> &CoreInfo {
        unreachable!("Cubism Core cannot be constructed on this platform")
    }

    pub(crate) fn load_model(
        self: &Arc<Self>,
        _moc_path: &Path,
    ) -> Result<LoadedCoreModel, Live2dError> {
        Err(Live2dError::UnsupportedPlatform)
    }
}

#[cfg(not(windows))]
impl LoadedCoreModel {
    pub(crate) fn info(&self) -> &CoreModelInfo {
        unreachable!("Cubism model cannot be constructed on this platform")
    }

    pub(crate) fn update(&mut self) {}
}
