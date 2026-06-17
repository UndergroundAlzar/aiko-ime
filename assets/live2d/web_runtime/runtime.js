import { loadOfficialCubismSdk } from "./sdk-loader.js";

const PROTOCOL_VERSION = 1;
const BUILTIN_MODELS = Object.freeze({
  aiko: "../aiko/runtime/aiko.model3.json",
  aiko_pajama: "../aiko_pajama/runtime/aiko_pajama.model3.json",
  aiko_rain: "../aiko_rain/runtime/aiko_rain.model3.json",
  aiko_winter: "../aiko_winter/runtime/aiko_winter.model3.json",
  aiko_celebration:
    "../aiko_celebration/runtime/aiko_celebration.model3.json",
  aiko_cag_operator: "../aiko_cag/runtime/aiko_cag.model3.json",
});

const canvas = document.querySelector("#live2d-canvas");
const status = document.querySelector("#runtime-status");
const webview = globalThis.chrome?.webview;

let cubismRuntime = null;
let activeModel = null;
let activeModelId = null;

function postEvent(payload, requestId = null) {
  const envelope = {
    protocol_version: PROTOCOL_VERSION,
    request_id: requestId,
    payload,
  };
  if (webview?.postMessage) {
    webview.postMessage(envelope);
  } else {
    globalThis.dispatchEvent(
      new CustomEvent("aiko-live2d-event", { detail: envelope }),
    );
  }
}

function reportError(code, error, requestId = null, recoverable = false) {
  const message = error instanceof Error ? error.message : String(error);
  status.textContent = message;
  postEvent({ type: "error", code, message, recoverable }, requestId);
}

function requireModel() {
  if (!activeModel) {
    throw new Error("No Cubism model is loaded");
  }
  return activeModel;
}

function requireMethod(target, method) {
  if (typeof target?.[method] !== "function") {
    throw new Error(`Cubism SDK adapter does not implement ${method}()`);
  }
  return target[method].bind(target);
}

async function loadBuiltinModel(command, requestId) {
  const model3Url = BUILTIN_MODELS[command.model];
  if (!model3Url) {
    throw new Error(`Unknown built-in Cubism model: ${command.model}`);
  }

  if (activeModel && typeof cubismRuntime.releaseModel === "function") {
    await cubismRuntime.releaseModel(activeModel);
  }

  // The private provider must parse model3.json with
  // CubismModelSettingJson, load its real moc3 through CubismUserModel, bind
  // textures, and initialize CubismRenderer_WebGL. No fallback renderer lives
  // in this public shell.
  activeModel = await requireMethod(cubismRuntime, "loadModel")({
    id: command.model,
    model3Url,
    canvas,
  });
  activeModelId = command.model;
  status.textContent = "";
  postEvent({ type: "model_loaded", model: command.model }, requestId);
}

async function executeCommand(envelope) {
  if (!envelope || envelope.protocol_version !== PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported bridge protocol version: ${envelope?.protocol_version}`,
    );
  }

  const command = envelope.payload;
  const requestId = envelope.request_id;
  switch (command?.type) {
    case "load_builtin_model":
      await loadBuiltinModel(command, requestId);
      return;
    case "set_parameter":
      requireMethod(requireModel(), "setParameter")(
        command.id,
        command.value,
        command.weight,
      );
      return;
    case "play_motion": {
      const finished = requireMethod(requireModel(), "playMotion")({
        group: command.group,
        index: command.index,
        priority: command.priority,
      });
      Promise.resolve(finished).then(() => {
        postEvent(
          {
            type: "motion_finished",
            group: command.group,
            index: command.index,
          },
          requestId,
        );
      });
      return;
    }
    case "set_expression":
      requireMethod(requireModel(), "setExpression")(command.id);
      return;
    case "reset_expression":
      requireMethod(requireModel(), "resetExpression")();
      return;
    case "set_look_at":
      requireMethod(requireModel(), "setLookAt")(command.x, command.y);
      return;
    case "set_lip_sync":
      requireMethod(requireModel(), "setLipSync")(command.value);
      return;
    case "hit_test":
      emitHitAreas(command.x, command.y, requestId);
      return;
    case "stop_all_motions":
      requireMethod(requireModel(), "stopAllMotions")();
      return;
    default:
      throw new Error(`Unknown Live2D command: ${command?.type}`);
  }
}

function emitHitAreas(x, y, requestId = null) {
  const hits = requireMethod(requireModel(), "hitTest")(x, y);
  for (const name of hits ?? []) {
    postEvent({ type: "hit_area", name, x, y }, requestId);
  }
}

function normalizedPointer(event) {
  const bounds = canvas.getBoundingClientRect();
  return {
    x: ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
    y: 1 - ((event.clientY - bounds.top) / bounds.height) * 2,
  };
}

canvas.addEventListener("pointermove", (event) => {
  if (!activeModel) return;
  const point = normalizedPointer(event);
  requireMethod(activeModel, "setLookAt")(point.x, point.y);
});

canvas.addEventListener("click", (event) => {
  if (!activeModel) return;
  const point = normalizedPointer(event);
  emitHitAreas(point.x, point.y);
});

async function dispatch(message) {
  let envelope = message;
  if (typeof message === "string") {
    envelope = JSON.parse(message);
  }
  try {
    await executeCommand(envelope);
  } catch (error) {
    reportError("command_failed", error, envelope?.request_id, true);
  }
}

globalThis.AikoImeLive2D = Object.freeze({ dispatch });
webview?.addEventListener("message", (event) => dispatch(event.data));

async function initialize() {
  try {
    const { core, sdk } = await loadOfficialCubismSdk();
    cubismRuntime = await sdk.createRuntime({
      canvas,
      core,
      transparent: true,
      onLog(level, message) {
        postEvent({ type: "log", level, message });
      },
    });
    const coreVersion =
      typeof core.Version?.csmGetVersion === "function"
        ? String(core.Version.csmGetVersion())
        : "available";
    postEvent({
      type: "runtime_ready",
      core_version: coreVersion,
      adapter_version: String(sdk.version ?? "unknown"),
    });
    status.textContent = "Cubism runtime ready; waiting for model command.";
  } catch (error) {
    reportError("cubism_sdk_unavailable", error);
  }
}

initialize();
