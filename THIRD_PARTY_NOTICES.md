# Third-party notices

NonoSub uses the following direct runtime dependencies. Transitive versions are locked by `pnpm-lock.yaml` and `src-tauri/Cargo.lock`; their license metadata can be reproduced with `pnpm licenses list --prod` and `cargo metadata --manifest-path src-tauri/Cargo.toml`.

## Web runtime

| Project | License | Source |
| --- | --- | --- |
| Tauri JavaScript API | Apache-2.0 OR MIT | <https://github.com/tauri-apps/tauri> |
| Tauri dialog and opener plugins | Apache-2.0 OR MIT | <https://github.com/tauri-apps/plugins-workspace> |
| Three.js | MIT | <https://github.com/mrdoob/three.js> |
| Fontsource packages | MIT | <https://github.com/fontsource/fontsource> |
| Klee One | SIL Open Font License 1.1 | <https://github.com/fontworks-fonts/Klee> |
| NonToon shader research reference | zlib | <https://github.com/lilxyzw/NonToon> |

Klee One is bundled unmodified for multilingual chalkboard text. Copyright 2020 The Klee Project Authors. Its full [SIL Open Font License 1.1](licenses/KLEE_ONE_OFL.txt) notice is included with this repository.

NonoSub's development-only `nontoon` comparison material is an altered Three.js/WebGL implementation inspired by NonToon's ramp-shade, rim-light, hair-specular, and basic-specular concepts. It is not a Unity shader port and is not part of normal user settings. Copyright 2026-present lilxyzw. The full [zlib notice](licenses/NONTOON_ZLIB.txt) is included with this repository. This notice applies to that referenced shader work and does not grant a license to NonoSub or Nico-owned assets.

## Native runtime

| Project | License | Source |
| --- | --- | --- |
| Tauri and plugins | Apache-2.0 OR MIT | <https://github.com/tauri-apps/tauri> |
| base64, futures-util, keyring, reqwest, serde, serde_json, tempfile | MIT OR Apache-2.0 | Their repositories are recorded in Cargo package metadata. |
| hound | Apache-2.0 | <https://github.com/ruuda/hound> |
| ScreenCaptureKit for Rust | MIT OR Apache-2.0 | <https://github.com/doom-fish/screencapturekit-rs> |
| Symphonia | MPL-2.0 | <https://github.com/pdeljanov/Symphonia> |
| Tokio and tokio-tungstenite | MIT | <https://github.com/tokio-rs/tokio> · <https://github.com/snapview/tokio-tungstenite> |

NonoSub does not modify Symphonia. Its corresponding source is available from the pinned crate version and upstream repository above. SPDX license texts are available at <https://spdx.org/licenses/>.

Nico-owned Nono character assets are not third-party dependencies and are governed by [ASSET_RIGHTS.md](ASSET_RIGHTS.md).
