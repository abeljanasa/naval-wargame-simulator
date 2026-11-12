# Releasing

## What the release workflow does

`.github/workflows/release.yml` runs on a `v*` tag or manually.

First, the **world terrain layer** job (on Windows) builds the 4′ terrain pack from its online source, so every release carries the current GEBCO grid:
- **Download:** `scripts/terrain/world-layer.sh` downloads the world grid from GEBCO on NOAA CoastWatch ERDDAP. It is a 2′ subset, about 117 MB.
- **Build:** it builds the pack with `nws-batch terrain build --average 2`, about 17.5 MB, and writes `SOURCE.txt`: the query, the download's SHA-256 and the date.
- **Updating the map:** a manual run takes `gebco_dataset` and `gebco_credit`. When a newer GEBCO grid is on ERDDAP, run the workflow with its dataset id and credit. A tag build uses GEBCO 2020.
- **Reproducible:** the pack holds no date, so the same data always gives the same pack and the same pack id. Records made with one release's layer replay on another's when the data has not changed.
- **Not in the repository:** the pack is not kept in the repository.

Then, on Windows and macOS (the application's platforms, ADR-0006), it:

1. Builds the desktop app with `npx tauri build --config src-tauri/tauri.terrain.json`. The overlay bundles the world layer as the app's built-in terrain folder. The build produces:
   - **Windows:** MSI (WiX) and NSIS `.exe` installers.
   - **macOS:** `.app` and `.dmg`.
2. Signs the installers, **only if the signing secrets are configured**; otherwise the run warns `UNSIGNED`. The secret names are below.
3. Runs the **release scan** (`sim-db scan target/release/bundle data`). The release fails if any database file is not on `data/db/release-allowlist.txt`, or if any record or JSON `classification` is not `UNCLASSIFIED` (master prompt §3).
4. Copies the shipped scenarios to `release-extra/scenarios/`, since only the Phase 1 demo is built into the app, and the world layer with its `.sha256` and `SOURCE.txt` to `release-extra/terrain/`. Writes `release-extra/SHA256SUMS-<os>.txt` with a SHA-256 sum for every installer and scenario, after signing, so the sums match what ships. The [installation check](../gate3/pilot-kit/installation-check.md) verifies them on the target machine.
5. Writes **SBOMs** in CycloneDX JSON: one per Rust crate (`cargo cyclonedx`) and one for the desktop front end (`npm sbom`).
6. Uploads the installers, scenarios, sums and SBOMs as artefacts.

The app needs no data files. The open platform database and the Phase 1 demo scenario are compiled into the binary, and the offline map and the world terrain layer are bundled. The installed app makes no network calls: there is no updater and no tile server. Regional terrain packs (the Philippine area at 15″) are installed separately with *File › Install terrain pack…*.

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

### Self-signed Windows certificate (ADR-0008)

The TTX release signs Windows installers with a self-signed certificate made by the client. Run this once, and again before the certificate expires (3 years), in your own PowerShell window. It asks for the PFX password:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\signing\new-selfsigned-codesign.ps1
```

It does four things:
- creates `CN=Naval Wargame Simulator` (RSA 3072, SHA-256);
- writes `%USERPROFILE%\nws-signing\nws-codesign.pfx` (the private key: back it up, never commit it) and `nws-codesign.cer` (the public certificate);
- removes the key from the certificate store;
- sets `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD` with the GitHub CLI. Use `-SkipSecrets` to set them later.

The release workflow then signs and timestamps every MSI and NSIS installer, and checks that each carries a signature.

**Trust on the exercise machines.** A self-signed certificate is trusted only where its `.cer` is installed:
- **Group Policy:** add it to *Trusted Root Certification Authorities* and *Trusted Publishers*.
- **By hand,** in an administrator PowerShell, on each machine:

```powershell
Import-Certificate -FilePath nws-codesign.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath nws-codesign.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
```

The installer then shows the publisher *Naval Wargame Simulator*. On a machine without the `.cer`, Windows shows "unknown publisher".

### macOS

macOS builds are ad-hoc signed (`bundle.macOS.signingIdentity: "-"`). The app is sealed against tampering, but it is not notarised, so each Mac approves it once: right-click › **Open**, or System Settings › Privacy & Security. If an Apple Developer ID is obtained later, the `APPLE_*` secrets above override the ad-hoc identity.

The SHA-256 sums (`release-extra/SHA256SUMS-<os>.txt`) travel with the installers, signed or not.

## Local build

```sh
cd apps/desktop
npm ci
npx tauri build      # installers in target/release/bundle/
cargo run --release -p sim-db -- scan ../../target/release/bundle ../../data
```

Without the overlay, the installers have no built-in terrain layer. To bundle it locally, as a release does:

```sh
cargo build --release -p nws-batch
scripts/terrain/world-layer.sh apps/desktop/src-tauri/terrain      # needs the internet
cd apps/desktop && npx tauri build --config src-tauri/tauri.terrain.json
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
