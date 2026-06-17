const DEFAULT_CORE_URL = "./vendor/live2dcubismcore.min.js";
const DEFAULT_ADAPTER_URL = "./vendor/aiko-cubism-web-sdk.js";

function loadScript(url) {
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = url;
    script.async = false;
    script.addEventListener("load", resolve, { once: true });
    script.addEventListener(
      "error",
      () => reject(new Error(`Failed to load required Cubism script: ${url}`)),
      { once: true },
    );
    document.head.append(script);
  });
}

export async function loadOfficialCubismSdk(options = {}) {
  const coreUrl = options.coreUrl ?? DEFAULT_CORE_URL;
  const adapterUrl = options.adapterUrl ?? DEFAULT_ADAPTER_URL;

  if (!globalThis.Live2DCubismCore) {
    await loadScript(coreUrl);
  }
  if (!globalThis.Live2DCubismCore) {
    throw new Error(
      "Official Live2D Cubism Core did not expose globalThis.Live2DCubismCore",
    );
  }

  if (!globalThis.AikoCubismWebSdk) {
    await loadScript(adapterUrl);
  }
  const sdk = globalThis.AikoCubismWebSdk;
  if (!sdk || typeof sdk.createRuntime !== "function") {
    throw new Error(
      "Private Cubism Framework adapter must expose AikoCubismWebSdk.createRuntime()",
    );
  }

  return {
    core: globalThis.Live2DCubismCore,
    sdk,
  };
}
