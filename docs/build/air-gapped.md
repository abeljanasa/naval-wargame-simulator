# Air-gapped build

Master prompt §12 requires a build path with every dependency vendored. CI's `offline-build` job proves the Rust part on every change: it vendors all crates, then builds and tests the engine with `--offline`.

## On a connected machine: prepare the transfer bundle

```sh
git clone <repo> nws && cd nws
cargo vendor --locked vendor > vendor-config.toml   # every crate source, plus the source replacement to use
(cd apps/desktop && npm ci)                         # fills node_modules exactly from package-lock.json
```

Also copy:

- **Toolchains:** the Rust toolchain installer for the pinned version in `rust-toolchain.toml` (an offline rustup or standalone installer) and the Node 24 installer.
- **Windows only:** the WiX 3 and NSIS toolsets that `tauri build` would otherwise download. Place them in the Tauri tool cache (`%LOCALAPPDATA%\tauri\WixTools314` and `%LOCALAPPDATA%\tauri\NSIS`).

Transfer the whole directory, including `vendor/` and `apps/desktop/node_modules/`, through the approved media process.

## On the air-gapped machine

```sh
mkdir -p .cargo && cp vendor-config.toml .cargo/config.toml
cargo test --offline --locked --workspace
cargo run --offline --release -p sim-db -- validate
cd apps/desktop && npx tauri build -- --offline
```

The open database, demo scenario, offline coastline and help content need no network at runtime.

## Verified

- On 2026-11-10 the engine crates (`sim-core`, `sim-geo`, `sim-db`, `sim-scenario`) built and passed all tests from a vendored source tree with `--offline`.
- The full desktop bundle has not been built on a disconnected machine yet (backlog).
