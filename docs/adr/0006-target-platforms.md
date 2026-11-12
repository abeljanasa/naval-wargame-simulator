# ADR-0006: Target platforms are Windows and macOS

- **Status:** Accepted
- **Date:** 2026-11-12
- **Amends:** master prompt §0 (*Target OS*) and §16 (Gate 3: signed installers), ADR-0001, ADR-0003

## Context

The master prompt lists Windows 10/11, macOS 13+ and Ubuntu 22.04+ as targets, each with native installers. On 2026-11-12 the client said it wants the application for Windows and macOS only.

## Decision

1. **The application ships for Windows and macOS only.** The release workflow builds MSI and NSIS installers on Windows, and an `.app` and `.dmg` on macOS. No AppImage, `.deb` or `.rpm`. The Tauri bundle targets stay `all`, which builds the formats of the OS it runs on. With no Linux build, that means the four above.
2. **CI tests the application on its platforms:** lint and test, and the determinism hashes, on Windows and macOS. The packaged-app test runs on Windows, because macOS has no WebDriver for its webview.
3. **Platform-neutral tooling may still run on Linux runners:**
   - formatting;
   - `cargo deny`;
   - database validation;
   - the performance budget and benchmark regression;
   - the vendored offline build of the engine crates;
   - the front-end checks.

   They test the code and data, not a Linux build of the application, and Linux runners are the cheapest and fastest.
4. **Determinism (ADR-0003)** is guaranteed and checked across Windows and macOS. The engine still avoids anything platform-specific, so the rule needs no other change.
5. **Gate 3** asks for signed installers that run air-gapped on Windows and macOS.

## Consequences

- Release and CI runs are shorter; no Linux system packages or virtual display are needed.
- The documents no longer describe Linux installation, Linux packages or Linux test drivers. Records of past runs (the Gate 2 pack, earlier CHANGELOG entries) stay as they were.
- If Linux is wanted again, a new ADR reverses this one. Restore the `ubuntu-22.04` release target (the AppImage's glibc floor), the WebKitGTK system packages, and the Linux rows of the CI matrices.
