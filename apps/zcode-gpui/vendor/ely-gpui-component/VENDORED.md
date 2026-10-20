# Vendored copy of ely-gpui-component

Source: https://github.com/ZacharyZhang-NY/Ely-GPUI-Components at commit
`2f8b2f687cd1e9b98a7cd29d4e882d09406fc547` (2026-10-03). The upstream repository
is no longer reachable, so cargo can no longer fetch it as a git dependency; this
copy keeps `apps/zcode-gpui` buildable on CI and fresh machines.

Only `src/`, `assets/`, the license files and the README are kept. The manifest
drops the `[[example]]` targets and the `web` profile, whose files are not
vendored. The code is unchanged. Licensed MIT OR Apache-2.0 (see the license
files); the embedded fonts (SIL OFL 1.1) and Lucide icons (ISC) keep their own
license texts under `assets/`.
