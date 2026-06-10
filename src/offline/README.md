# Aiko IME Offline ASR / 离线语音识别

## Status / 当前状态

The provider layer and sherpa-onnx C API backend are implemented and tested on
Windows with sherpa-onnx `v1.13.2` and the bilingual streaming Zipformer model.

Provider 层与 sherpa-onnx C API 后端已经完成，并已在 Windows 上使用
sherpa-onnx `v1.13.2` 和中英双语流式 Zipformer 模型完成真实识别测试。

The current application capture path still returns Opus frames for Doubao.
The main integration must add a PCM output path and select `DoubaoProvider` or
`SherpaOnnxProvider` in the voice controller. The offline provider rejects Opus
with an explicit error instead of pretending to decode it.

当前应用的录音链路仍只为豆包输出 Opus。主程序集成时需要给录音模块增加 PCM
输出，并在语音控制器中选择 `DoubaoProvider` 或 `SherpaOnnxProvider`。离线
provider 收到 Opus 时会明确报错，不会伪装成可识别。

## Build / 构建

```powershell
$env:PROTOC = (Resolve-Path tools/protoc/bin/protoc.exe)
cargo build --release --features sherpa-onnx
```

No new Rust dependency is required. The feature enables Windows dynamic loading
through the existing `windows` crate.

不需要新增 Rust 依赖。该 feature 使用项目现有的 `windows` crate 在运行时动态
加载 DLL。

## Runtime package / 运行库

Use the official Windows x64 shared no-TTS package matching `v1.13.2` or newer:

使用官方 Windows x64 shared no-TTS 包，版本需为 `v1.13.2` 或更高：

`sherpa-onnx-v1.13.2-win-x64-shared-MT-Release-no-tts.tar.bz2`

Keep these files together:

以下文件必须放在同一目录：

```text
runtime/sherpa-onnx/lib/
  sherpa-onnx-c-api.dll
  onnxruntime.dll
  onnxruntime_providers_shared.dll
```

The loader checks, in order:

加载器按以下顺序查找：

1. `SherpaOnnxConfig.library_path`
2. `AIKO_IME_SHERPA_DLL`
3. The selected model directory
4. `runtime/sherpa-onnx/lib` beside `aiko-ime.exe`
5. `%LOCALAPPDATA%/AikoIME/runtime/sherpa-onnx/lib`

Call `SherpaOnnxConfig::probe_backend()` to verify the DLL and all required C API
symbols before starting a session.

设置界面可调用 `SherpaOnnxConfig::probe_backend()`，在开始会话前验证 DLL 及所需
C API 符号。

## Models / 模型

`ModelManager::standard()` checks only application-owned model locations:

`ModelManager::standard()` 只检查应用自己的模型目录：

```text
AIKO_IME_MODEL_DIR
<exe>/models
<exe>/models/sherpa-onnx
%LOCALAPPDATA%/AikoIME/models
<working-directory>/models
```

For a conventional online transducer package, discovery recognizes:

常规在线 Transducer 模型包会自动识别：

```text
*encoder*.onnx
*decoder*.onnx
*joiner*.onnx
tokens.txt
```

For deterministic packaging, add `aiko-sherpa-model.json`:

为了让分发包完全确定，建议增加 `aiko-sherpa-model.json`：

```json
{
  "schema_version": 1,
  "name": "Aiko bilingual Zipformer",
  "family": "online-transducer",
  "sample_rate": 16000,
  "feature_dim": 80,
  "files": {
    "encoder": "encoder-epoch-99-avg-1.int8.onnx",
    "decoder": "decoder-epoch-99-avg-1.int8.onnx",
    "joiner": "joiner-epoch-99-avg-1.int8.onnx",
    "tokens": "tokens.txt"
  }
}
```

Manifest paths must be relative and cannot escape the model package directory.
Missing files and malformed manifests produce bilingual actionable errors.

清单路径必须为相对路径，且不能越过模型包目录。文件缺失或 JSON 错误都会返回可
操作的中英双语错误信息。

## Audio contract / 音频约定

- Doubao: 16 kHz mono Opus frames.
- sherpa-onnx: interleaved PCM16 or normalized PCM float frames.
- Multi-channel PCM is downmixed to mono.
- sherpa-onnx performs sample-rate conversion internally.
- `finish()` drains queued audio and emits final results.
- `cancel()` stops immediately and emits `SessionEndReason::Cancelled`.

- 豆包：16 kHz 单声道 Opus 帧。
- sherpa-onnx：交错 PCM16 或归一化浮点 PCM。
- 多声道 PCM 会自动混合为单声道。
- 采样率转换由 sherpa-onnx 内部完成。
- `finish()` 会处理完排队音频并返回最终结果。
- `cancel()` 会立即停止并返回 `SessionEndReason::Cancelled`。

## Main-app integration / 主程序集成

The voice controller should depend on `Arc<dyn AsrProvider>`. For offline mode,
capture 20 ms PCM frames before Opus encoding and send them as:

语音控制器应依赖 `Arc<dyn AsrProvider>`。离线模式需要在 Opus 编码之前取得
20 ms PCM 帧并发送：

```rust
session
    .send_audio(AudioFrame::pcm_i16(samples, 16_000, 1))
    .await?;
```

Map provider events directly to the existing UI states:

可直接将 provider 事件映射到现有界面状态：

```text
SessionStarted -> connecting/ready
SpeechStarted -> listening
PartialResult -> live text
FinalResult -> committed text
Error -> visible bilingual error
SessionFinished -> idle
```
