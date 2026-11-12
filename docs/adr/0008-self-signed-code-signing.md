# ADR-0008: Self-signed Windows code signing and ad-hoc macOS signing for the TTX release

- **Status:** Accepted (by the client, 2026-11-12). The Gate 3 panel must still accept it as meeting "signed installers" (master prompt §16).
- **Date:** 2026-11-12
- **Amends:** the Gate 3 signing evidence ([releasing.md](../build/releasing.md)), which assumed a certificate from a public certification authority and an Apple Developer ID.

## Context

Gate 3 asks for signed installers for Windows and macOS (ADR-0006). The client chose a self-signed certificate made with PowerShell, not a commercial code-signing certificate or an Apple Developer account.

## Decisions

1. **Windows: self-signed code signing.**
   - The certificate is `CN=Naval Wargame Simulator`: RSA 3072, SHA-256, valid for 3 years.
   - It is made by `scripts/signing/new-selfsigned-codesign.ps1`. The client runs the script and types the PFX password; the developer never sees it.
   - The PFX and its password are the Actions secrets `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD`. The release workflow signs every MSI and NSIS installer with `signtool`, timestamps it, and checks that each file carries a signature.
2. **Trust is deployed, not inherited.** A self-signed certificate is trusted only where its public `.cer` is installed:
   - in **Trusted Root Certification Authorities**, so the signature chains;
   - in **Trusted Publishers**, so Windows treats the publisher as known.

   On a navy-managed network, IT deploys it by Group Policy. On pilot machines it is imported by hand ([installation check](../gate3/pilot-kit/installation-check.md)).
   - Installers copied from removable media carry no "downloaded from the internet" mark, so SmartScreen does not check them.
   - The install prompt shows the publisher *Naval Wargame Simulator*.
3. **macOS: ad-hoc signing.** Apple's Gatekeeper and notarisation accept only Apple-issued Developer ID certificates, so a self-signed certificate cannot help.
   - The `.app` is ad-hoc signed (`bundle.macOS.signingIdentity: "-"` in `tauri.conf.json`). It is sealed against tampering, but its publisher is not identified.
   - Users approve it once (right-click › Open, or System Settings › Privacy & Security).
   - If an Apple Developer ID is obtained later, the `APPLE_*` secrets override the ad-hoc identity with no other change.
4. **Key custody:** the PFX lives in the client's user folder (`%USERPROFILE%\nws-signing`) and in the Actions secret. It is never committed.
   - If the key leaks, make a new certificate, replace the secret, and remove the old `.cer` from the trusted stores.
   - The certificate expires after 3 years. Signatures made before then stay valid, because they are timestamped. New builds need a new certificate made with the same script.

## Consequences

- Release builds are signed with no third-party cost or dependency, and any change to an installer breaks its signature.
- **Windows:** the installers are trusted only on machines that have the `.cer` installed; elsewhere Windows shows "unknown publisher". That is acceptable for a closed exercise network, and not for distribution to the public.
- **macOS:** the app still needs a one-time approval on each Mac.
- **Gate 3:** the panel records whether this meets "signed installers" ([sign-off](../gate3/sign-off.md)).
