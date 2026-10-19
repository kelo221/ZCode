# zcode-voice

Local text-to-speech sidecar for zcode-gpui's agent voice ([VOICE.md](../zcode-gpui/VOICE.md)).
It wraps [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) (Kokoro-82M on ONNX Runtime,
phonemes from eSpeak NG) and plays speech on the system default audio device. zcode-gpui
starts it when a line is spoken and stops it 30 seconds after the last line finishes. Desktop only; the Android build
never contains it.

Licensed **GPL-3.0-or-later** because it statically links eSpeak NG. It is a separate program
from zcode-gpui (Apache-2.0), which only talks to it over stdin/stdout. See
[NOTICE.md](NOTICE.md).

## Build

Needs Rust, cmake and libclang (for espeak-ng's bindgen). Either install LLVM, or
`pip install --user libclang` and point `LIBCLANG_PATH` at the wheel's `clang/native`
directory. With cmake 4, set `CMAKE_POLICY_VERSION_MINIMUM=3.5`.

```sh
export LIBCLANG_PATH=<dir containing libclang.dll>
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo build --release
cargo test
```

The build downloads ONNX Runtime and compiles espeak-ng from source. The exe imports
`DirectML.dll`, which cargo places next to it. `scripts/package-gpui.mjs` copies the exe,
`DirectML.dll`, the matching `espeak-ng-data/` and `NOTICE.md` into `dist/zcode-gpui`.

Pins worth knowing: kokoro-tiny comes from a git revision (0.2 is not on crates.io); `ort` is
pinned to `=2.0.0-rc.11` because later release candidates break kokoro-tiny; `ndarray-npy` is
vendored under `vendor/` with an updated `zip` dependency because every zip 5–7 release is
yanked.

## Run by hand

```sh
./target/release/zcode-voice --model-dir ~/.zcode/v2/voice --say "Hello from ZCode"
```

The first run downloads the model (`0.onnx`, about 325 MB, and `0.bin`, about 28 MB) into the
model directory. eSpeak NG data is found through `PIPER_ESPEAKNG_DATA_DIRECTORY`, then
`espeak-ng-data/` next to the exe, then the cargo build output, then
`%ProgramFiles%\eSpeak NG`.

## Protocol

Without `--say`, it reads one JSON command per line on stdin and writes one JSON event per line
on stdout (other stdout lines come from kokoro-tiny and must be skipped):

- Commands: `{"op":"speak","id","text","voice","speed","volume"}`, `{"op":"stop"}`,
  `{"op":"status"}`. Closing stdin stops speech and exits.
- Events: `loading`, `downloading {file, percent}`, `ready {voices}`, `started {id}`,
  `spoken {id}`, `dropped {id}`, `error {id?, code, message}`.

Text is cut to 300 characters. At most 3 lines wait in the queue; the oldest is dropped.
