# zcode-voice: license and notices

`zcode-voice` is the local text-to-speech sidecar that zcode-gpui starts for
the agent `Speak` tool (see `apps/zcode-gpui/VOICE.md`). It is a separate
program and is licensed **GPL-3.0-or-later**, because it statically links
eSpeak NG. zcode-gpui only talks to it over stdin/stdout (newline-delimited
JSON) and does not link it.

The source code is in `apps/zcode-voice` of the ZCode repository.

| Component                                       | Version                  | License                                    | Upstream                                             |
| ----------------------------------------------- | ------------------------ | ------------------------------------------ | ---------------------------------------------------- |
| eSpeak NG (compiled in, plus `espeak-ng-data/`) | via espeak-rs-sys 0.1.9  | GPL-3.0-or-later                           | https://github.com/espeak-ng/espeak-ng               |
| espeak-rs / espeak-rs-sys                       | 0.1.9                    | MIT                                        | https://crates.io/crates/espeak-rs                   |
| kokoro-tiny                                     | 0.2.0, git rev `0ca44ce` | Apache-2.0                                 | https://github.com/8b-is/kokoro-tiny                 |
| ort (ONNX Runtime bindings)                     | 2.0.0-rc.11              | MIT OR Apache-2.0                          | https://github.com/pykeio/ort                        |
| ONNX Runtime (prebuilt, linked by ort)          | as downloaded by ort     | MIT                                        | https://github.com/microsoft/onnxruntime             |
| DirectML.dll (shipped next to the exe)          | as downloaded by ort     | Microsoft DirectML redistributable license | https://www.nuget.org/packages/Microsoft.AI.DirectML |
| ndarray-npy (vendored with a zip 9 update)      | 0.10.0                   | MIT OR Apache-2.0                          | https://github.com/jturner314/ndarray-npy            |
| rodio                                           | 0.17.3                   | MIT OR Apache-2.0                          | https://github.com/RustAudio/rodio                   |
| reqwest                                         | 0.12                     | MIT OR Apache-2.0                          | https://github.com/seanmonstar/reqwest               |
| tokio                                           | 1.x                      | MIT                                        | https://github.com/tokio-rs/tokio                    |
| serde / serde_json                              | 1.x                      | MIT OR Apache-2.0                          | https://github.com/serde-rs                          |

The Kokoro voice model (`0.onnx`, `0.bin`) is not shipped. The sidecar
downloads it on first use from the kokoro-tiny repository into
`~/.zcode/v2/voice`. Kokoro-82M is published under Apache-2.0.

GPL-3.0 text: https://www.gnu.org/licenses/gpl-3.0.txt
