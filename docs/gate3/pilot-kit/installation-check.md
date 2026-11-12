# Installation check (air-gapped)

One copy per exercise machine and for the spare. This covers success criterion 1 of the [pilot plan](../ttx-pilot-plan.md) §7. The same sheet serves the Gate 3 air-gapped run of the signed build ([Gate 3 pack](../README.md)) on Windows and macOS.

Machine: ____________  OS and version: ____________  Checked by: ____________  Date: ________

## On a connected machine, the day before

1. Download the release workflow's artefact for this OS: `installers-windows-2022` or `installers-macos-15`. Each holds:
   - the installers, under `target/release/bundle/`;
   - `release-extra/scenarios/`, the shipped scenarios (only the Phase 1 demo is built into the app);
   - `release-extra/SHA256SUMS-<os>.txt`, a SHA-256 sum for every file.
2. Copy the whole unzipped folder to removable media. Write the first 8 characters of the installer's sum here: __________

## On the exercise machine

| ☐ | Step | Pass when |
|---|---|---|
| ☐ | Disconnect the network: unplug the cable, turn off Wi-Fi and Bluetooth, and disable the adapters in the OS settings | The OS reports no network |
| ☐ | Copy the folder from the media and verify the sums (commands below) | Every line says `OK` |
| ☐ | **Windows:** install the signing certificate as trusted, unless IT has deployed it (administrator PowerShell; commands below) | Both imports succeed |
| ☐ | Install. **Windows:** the MSI or the setup `.exe`. **macOS:** open the `.dmg` and drag the app to Applications. | It installs with no download prompt. **Windows:** the prompt names the publisher *Naval Wargame Simulator*. **macOS:** the app is ad-hoc signed, so approve it once (right-click › *Open*). Note anything else below |
| ☐ | Start *Naval Wargame Simulator* | The map shows the coastline; no error about the network |
| ☐ | **File › Open scenario…** → `release-extra/scenarios/phase3-pilot.json` | The title reads *Phase 3 — Kent passage pilot*; the units are on the map |
| ☐ | **Seats** tab → **Open window** for Blue and for Red | Each window shows only its own side |
| ☐ | Run at **Max** (key **7**) to 00:30 game time, then pause (**Space**) | Contacts and messages appear; the clock stops |
| ☐ | **Review › Save record**, then **Replay** | The banner reads *"Replay: matches the recorded run"* |
| ☐ | Copy the record to the spare machine and open it there under **Review** | The same banner on the spare |
| ☐ | Reload the scenario (**File › Open scenario…**) for the exercise | Back at `D+0 00:00:01`, paused |

### Verifying the sums

Run these from the unzipped artefact folder (the one holding `target` and `release-extra`).

**macOS:**

```sh
shasum -a 256 -c release-extra/SHA256SUMS-macos-15.txt
```

**Windows (PowerShell):**

```powershell
Get-Content release-extra\SHA256SUMS-windows-2022.txt | ForEach-Object {
  $hash, $file = $_ -split '\s+\*?', 2
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash -eq $hash) { "OK    $file" } else { "FAIL  $file" }
}
```

**Windows, trusting the signing certificate** (`nws-codesign.cer`, from the release owner; [ADR-0008](../../adr/0008-self-signed-code-signing.md)):

```powershell
Import-Certificate -FilePath nws-codesign.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath nws-codesign.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
Get-AuthenticodeSignature .\target\release\bundle\msi\*.msi | Format-List Status, SignerCertificate
```

The last line should show `Status : Valid` and the subject `CN=Naval Wargame Simulator`.

A `FAIL` means the copy is damaged or altered. Don't install it: copy it again from the media, or from the artefact.

## Result

- Signed build: **yes / no (unsigned, OS warning shown)**
- Any network prompt or error: ____________________________________________
- **Pass / fail**, and why: ____________________________________________
