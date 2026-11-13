# Validation: Operation Praying Mantis, 18 April 1988 (ship actions)

> **Status: DRAFT for SME review: built from public accounts, not yet confirmed.**

In reprisal for the mining of USS Samuel B. Roberts, US forces struck two Iranian oil platforms and then fought the largest US surface action since 1945. Joshan fired a Harpoon at SAG Charlie that missed and was sunk by Standard missiles and gunfire; Sahand was sunk by Harpoons, Skippers and laser-guided bombs from A-6Es after firing on them; Sabalan was crippled by a laser-guided bomb and allowed to be towed away. The United States lost a Cobra helicopter with its two crew (an accident, not modelled).

- **Scenario:** Validation draft — Operation Praying Mantis, 18 April 1988 (`scenario.json`)
- **Runs:** 200 (seeds 19880418–19880617)
- **Method:** [docs/models/validation.md](../../../models/validation.md)

## Model against the record

| Measure | Record | Model | Verdict |
|---|---|---|---|
| Joshan sunk | yes | P(record) 100 % (95 % CI 98–100 %) | consistent |
| Sahand sunk | yes | P(record) 29 % (95 % CI 23–36 %) | consistent |
| Sabalan disabled | yes | P(record) 60 % (95 % CI 54–67 %) | consistent |
| Sabalan sunk | no | P(record) 57 % (95 % CI 51–64 %) | consistent |
| US ships and aircraft lost to Iranian fire | 0 | mean 1.2 (5–95 %: 0–3; n 200) | consistent |
| Hits by Iran | 0 | mean 1.5 (5–95 %: 0–3; n 200) | consistent |
| US missiles fired at ships | 12 | mean 14.2 (5–95 %: 10–18; n 200) | consistent |
| Iranian missiles fired | 2 | mean 3.9 (5–95 %: 4–4; n 200) | outside the model's 90 % range |

## Differences to explain

Each difference is for the SMEs to explain as a **model limitation**, a **data gap** or **historical uncertainty**.

- **Iranian missiles fired** (outside the model's 90 % range): _to explain._
  - Note: Joshan's Harpoon at Wainwright, and a surface-to-air missile from Sahand at the A-6Es (accounts vary).

## What the scenario assumes

- Time-compressed: the Joshan action (about 12:00 local) and the frigate actions (afternoon) start together.
- Omitted: the platform strikes (Sassan, Sirri), SAG Bravo and SAG Delta, USS Strauss, the Boghammar attacks, the F-4 and F-14 activity, the E-2C, and the Cobra loss.
- SAG Charlie (Wainwright, Simpson, Bagley) is about 20 km from Joshan, which closes it at 25 kn as she did.
- Sahand and Sabalan patrol west of the Strait; the A-6Es arrive airborne with no airbase or tanker modelled. Two carry the anti-ship loadout (Harpoon and two Skippers), one the strike loadout (two laser-guided bombs). They hold stations about 18 km east of their targets (stand-off), engaging what comes into weapon range.
- Sabalan's 'crippled but towed' outcome is the model's mission kill (disabled, not sunk).
- Sensors are the database's generic classes; the US ships' radar and ESM picture (and the AWACS and E-2C cueing) is approximated by their own sensors.
- Weapon data for Harpoon, Skipper, SM-1MR, Sea Killer and the guided bomb are open-source estimates (data/db/open/weapons.json).

## Sources

- Wikipedia, 'Operation Praying Mantis', https://en.wikipedia.org/wiki/Operation_Praying_Mantis, accessed 2026-11-13
- Wikipedia, 'IRIS Sahand (F74)', https://en.wikipedia.org/wiki/IRIS_Sahand_(F74), accessed 2026-11-13
- Wikipedia, 'IRIS Joshan', https://en.wikipedia.org/wiki/IRIS_Joshan, accessed 2026-11-13
- Wikipedia, 'IRIS Sabalan', https://en.wikipedia.org/wiki/IRIS_Sabalan, accessed 2026-11-13
