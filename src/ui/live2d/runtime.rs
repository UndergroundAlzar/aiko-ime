use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use super::command::{resolve_command, Live2dCommand, ResolvedCommand};
use super::core::{CoreInfo, CoreLibrary, CoreModelInfo, LoadedCoreModel, RuntimeOptions};
use super::error::Live2dError;
use super::manifest::ModelManifest;

pub struct CubismRuntime {
    core: Arc<CoreLibrary>,
    model: Option<LoadedModel>,
    commands: VecDeque<ResolvedCommand>,
}

struct LoadedModel {
    manifest: ModelManifest,
    core_model: LoadedCoreModel,
}

impl CubismRuntime {
    pub fn new(options: RuntimeOptions) -> Result<Self, Live2dError> {
        Ok(Self {
            core: CoreLibrary::load(&options)?,
            model: None,
            commands: VecDeque::new(),
        })
    }

    pub fn core_info(&self) -> &CoreInfo {
        self.core.info()
    }

    pub fn load_model(
        &mut self,
        manifest_path: impl AsRef<Path>,
    ) -> Result<&ModelManifest, Live2dError> {
        let manifest = ModelManifest::load(manifest_path)?;
        self.load_manifest(manifest)
    }

    pub fn load_manifest(
        &mut self,
        manifest: ModelManifest,
    ) -> Result<&ModelManifest, Live2dError> {
        let core_model = self.core.load_model(&manifest.moc)?;
        self.commands.clear();
        self.model = Some(LoadedModel {
            manifest,
            core_model,
        });
        Ok(&self
            .model
            .as_ref()
            .expect("model was just assigned")
            .manifest)
    }

    pub fn unload_model(&mut self) {
        self.commands.clear();
        self.model = None;
    }

    pub fn manifest(&self) -> Option<&ModelManifest> {
        self.model.as_ref().map(|model| &model.manifest)
    }

    pub fn model_info(&self) -> Option<&CoreModelInfo> {
        self.model.as_ref().map(|model| model.core_model.info())
    }

    pub fn submit(&mut self, command: Live2dCommand) -> Result<(), Live2dError> {
        let model = self.model.as_ref().ok_or(Live2dError::NoModelLoaded)?;
        self.commands
            .push_back(resolve_command(&model.manifest, command)?);
        Ok(())
    }

    pub fn next_command(&mut self) -> Option<ResolvedCommand> {
        self.commands.pop_front()
    }

    pub fn update_core_model(&mut self) -> Result<(), Live2dError> {
        let model = self.model.as_mut().ok_or(Live2dError::NoModelLoaded)?;
        model.core_model.update();
        Ok(())
    }
}
