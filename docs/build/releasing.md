# Releasing

## What the release workflow does

`.github/workflows/release.yml` runs on a `v*` tag or manually. On Windows and macOS (the application's platforms, ADR-0006) it:

1. Builds the desktop app with `npx tauri build`, which produces:
   - **Windows:** MSI (WiX) and NSIS `.exe` installers.
   - **macOS:** `.app` and `.dmg`.
2. Signs the installers, **only if the signing secrets are configured**; otherwise the run warns `UNSIGNED`. The secret names are below.
3. Runs the **release scan** (`sim-db scan target/release/bundle data`). The release fails if any database file is not on `data/db/release-allowlist.txt`, or if any record or JSON `classification` is not `UNCLASSIFIED` (master prompt §3).
4. Copies the shipped scenarios to `release-extra/scenarios/`, since only the Phase 1 demo is built into the app. Writes `release-extra/SHA256SUMS-<os>.txt` with a SHA-256 sum for every installer and scenario, after signing, so the sums match what ships. The [installation check](../gate3/pilot-kit/installation-check.md) verifies them on the target machine.
5. Writes **SBOMs** in CycloneDX JSON: one per Rust crate (`cargo cyclonedx`) and one for the desktop front end (`npm sbom`).
6. Uploads the installers, scenarios, sums and SBOMs as artefacts.

The app needs no data files. The open platform database and the Phase 1 demo scenario are compiled into the binary, and the offline map is bundled. The installed app makes no network calls: there is no updater and no tile server.

## Signing secrets

Set these in the repository's (or organisation's) Actions secrets.

| Secret | Used for |
|---|---|
| `WINDOWS_CERTIFICATE` | Base64 of the code-signing `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | Its password |
| `APPLE_CERTIFICATE` | Base64 of the Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Its password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: <Org> (<TEAMID>)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarisation (app-specific password) |

macOS builds are back in the CI and release matrices (Gate 3, 2026-11-11). Without the Apple secrets the `.dmg` is unsigned and not notarised; Gatekeeper will then block it until the user allows it in System Settings.

The SHA-256 sums (`release-extra/SHA256SUMS-<os>.txt`) travel with the installers, signed or not.

## Local build

```sh
cd apps/desktop
npm ci
npx tauri build      # installers in target/release/bundle/
cargo run --release -p sim-db -- scan ../../target/release/bundle ../../data
```

On Windows the first `tauri build` downloads the WiX and NSIS toolsets. For air-gapped build machines, see [air-gapped.md](air-gapped.md).

## Packaged-app test

`npm run e2e:app` (`apps/desktop/scripts/e2e-app.mjs`) drives the built binary over WebDriver, through tauri-driver. It tests the real engine, the embedded database and the bundled map, where `npm run e2e` uses a mocked backend. It checks that:
- the app starts on its built-in scenario;
- the database opens from the menu bar;
- the pilot scenario loads from a file;
- the white-cell seat sees both sides;
- the engine runs at Max and the scenario events send messages;
- the white-cell run sheet works: console tabs, events, sending an inject, command posts, a locked seat window, a logged override, and a verified replay.

Set `E2E_SHOTS=<folder>` to save a screenshot at each rehearsal step.

CI runs it on Windows (job *Packaged app end to end*). macOS has no WebDriver for its webview.

```sh
cargo install tauri-driver --locked
cd apps/desktop
npx tauri build --no-bundle          # the binary in target/release/
npm run e2e:app
```

- Needs `msedgedriver` of the same version as the WebView2 runtime, on the `PATH` or in `NATIVE_DRIVER`. The version is under `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` (value `pv`). Download it from `https://msedgedriver.microsoft.com/<version>/edgedriver_win64.zip`.
- **Another build:** set `NWS_APP` to its binary.
