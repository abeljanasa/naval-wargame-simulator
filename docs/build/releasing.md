# Releasing

## What the release workflow does

`.github/workflows/release.yml` runs on a `v*` tag or manually. On Windows, macOS and Ubuntu 22.04 it:

1. Builds the desktop app with `npx tauri build`, which produces:
   - **Windows:** MSI (WiX) and NSIS `.exe` installers.
   - **macOS:** `.app` and `.dmg`.
   - **Linux:** AppImage, `.deb` and `.rpm`.
2. Signs the installers, **only if the signing secrets are configured**; otherwise the run warns `UNSIGNED`. The secret names are below.
3. Runs the **release scan** (`sim-db scan target/release/bundle data`). The release fails if any database file is not on `data/db/release-allowlist.txt`, or if any record or JSON `classification` is not `UNCLASSIFIED` (master prompt §3).
4. Writes **SBOMs** in CycloneDX JSON: one per Rust crate (`cargo cyclonedx`) and one for the desktop front end (`npm sbom`).
5. Uploads the installers and SBOMs as artefacts.

The app needs no data files. The open platform database and the demo scenario are compiled into the binary, and the offline map is bundled. The installed app makes no network calls: there is no updater and no tile server.

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

Linux packages are not signed by default. Distribute their SHA-256 sums alongside, or sign them with the organisation's GPG key in the accredited pipeline.

## Local build

```sh
cd apps/desktop
npm ci
npx tauri build      # installers in target/release/bundle/
cargo run --release -p sim-db -- scan ../../target/release/bundle ../../data
```

On Windows the first `tauri build` downloads the WiX and NSIS toolsets. For air-gapped build machines, see [air-gapped.md](air-gapped.md).
