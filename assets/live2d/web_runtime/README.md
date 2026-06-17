# Aiko WebView2 Cubism runtime shell

This directory is a public integration shell, not a Cubism SDK distribution.
It contains no Live2D Cubism Core, Framework, Samples, or model binary.

## Private runtime inputs

Supply these files during a licensed private/package build:

```text
assets/live2d/web_runtime/vendor/live2dcubismcore.min.js
assets/live2d/web_runtime/vendor/aiko-cubism-web-sdk.js
assets/live2d/aiko/runtime/aiko.model3.json
assets/live2d/aiko/runtime/*.moc3
assets/live2d/aiko/runtime/<referenced textures, motions, expressions, physics>
```

The public protocol reserves six fixed built-in model IDs:

```text
aiko
aiko_pajama
aiko_rain
aiko_winter
aiko_celebration
aiko_cag_operator
```

Each ID resolves to its own fixed `assets/live2d/<skin>/runtime/*.model3.json`
entry. Arbitrary model URLs and user-imported models are intentionally not
accepted.

`live2dcubismcore.min.js` must come unmodified from the official Cubism SDK for
Web. `aiko-cubism-web-sdk.js` is a private application bundle built against the
official Framework. It must expose:

```js
globalThis.AikoCubismWebSdk = {
  version: "private-build-version",
  async createRuntime({ canvas, core, transparent, onLog }) {
    return {
      async loadModel({ id, model3Url, canvas }) {
        // Use CubismModelSettingJson for model3.json, CubismUserModel/loadModel
        // for the referenced moc3, then bind textures and initialize the
        // official WebGL renderer. Return the model adapter below.
      },
      async releaseModel(model) {},
    };
  },
};
```

The returned model adapter must implement `setParameter`, `playMotion`,
`setExpression`, `resetExpression`, `setLookAt`, `setLipSync`, `hitTest`, and
`stopAllMotions`. `hitTest(x, y)` returns model3 hit-area names.

The public shell deliberately has no canvas-drawn or PNG fallback. Missing
proprietary runtime files produce an explicit host/runtime error and no
rendering claim.

## WebView2 host integration still required

The Rust `WebView2HostAdapter` only adapts commands to `ExecuteScript` and parses
`window.chrome.webview.postMessage` events. A concrete window layer still needs
to:

1. create a transparent WebView2 controller;
2. map `assets/live2d` to a secure virtual host or equivalent local origin;
3. navigate to `web_runtime/index.html`;
4. implement `ScriptExecutor` with WebView2 `ExecuteScript`;
5. forward WebMessageReceived JSON to `on_web_message`.
