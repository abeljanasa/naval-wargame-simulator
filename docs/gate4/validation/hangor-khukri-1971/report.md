# Validation: Hangor and Khukri, 9 December 1971

> **Status: DRAFT for SME review: built from public accounts, not yet confirmed.**

The first sinking of a warship by a submarine since 1945. Hangor, submerged west of Diu Head, fired three torpedoes at the two Indian frigates hunting her: the first, at Kirpan, missed (or failed); a second hit Khukri, which sank within minutes with heavy loss of life; a third was fired at Kirpan, which withdrew. Hangor escaped despite a prolonged search.

- **Scenario:** Validation draft — Hangor and Khukri, 9 December 1971 (`scenario.json`)
- **Runs:** 200 (seeds 19711209–19711408)
- **Method:** [docs/models/validation.md](../../../models/validation.md)

## Model against the record

| Measure | Record | Model | Verdict |
|---|---|---|---|
| Khukri sunk | yes | P(record) 0 % (95 % CI 0–2 %) | inconsistent |
| Kirpan survives | yes | P(record) 100 % (95 % CI 98–100 %) | consistent |
| Hangor survives | yes | P(record) 100 % (95 % CI 98–100 %) | consistent |
| Torpedoes fired by Hangor | 3 | mean 0.0 (5–95 %: 0–0; n 200) | outside the model's 90 % range |
| Indian ships lost | 1 | mean 0.0 (5–95 %: 0–0; n 200) | outside the model's 90 % range |
| Hits by Pakistan | 1 | mean 0.0 (5–95 %: 0–0; n 200) | outside the model's 90 % range |
| Time to Hangor's first hit (s) | 600 | did not occur (n 0) | never happened in the model |

## Differences to explain

Each difference is for the SMEs to explain as a **model limitation**, a **data gap** or **historical uncertainty**.

- **Khukri sunk** (inconsistent): _to explain._
- **Torpedoes fired by Hangor** (outside the model's 90 % range): _to explain._
  - Note: Three by most accounts; the first is reported to have run deep or failed to detonate.
- **Indian ships lost** (outside the model's 90 % range): _to explain._
- **Hits by Pakistan** (outside the model's 90 % range): _to explain._
- **Time to Hangor's first hit (s)** (never happened in the model): _to explain._
  - Note: Not in the record to the minute: about ten minutes after the attack began is an estimate for the SMEs to confirm or replace.

## What the scenario assumes

- Start about 14:00 UTC (19:30 local), after dark; Hangor already in contact and submerged at 50 m.
- Khukri steers west at 12 kn; Kirpan keeps station 4 km on her starboard bow (the public accounts give the frigates as searching at slow speed; the formation is an assumption).
- Positions are approximate (about 35 nm SSW of Diu); the accounts disagree by several miles.
- The torpedoes are the database's generic heavyweight torpedo (WPN-TORP-HW), not the French E14/E15 the boat carried; their reliability and the 'first torpedo did not explode' account are not modelled.
- The Indian frigates' sonar (Type 170/174) is the generic hull-mounted medium-frequency sonar.
- The engagement window is one hour; the Indian search of the following days is not modelled.

## Sources

- Wikipedia, 'INS Khukri (1956)', https://en.wikipedia.org/wiki/INS_Khukri_(1956), accessed 2026-11-13
- Wikipedia, 'PNS Hangor (S131)', https://en.wikipedia.org/wiki/PNS_Hangor_(S131), accessed 2026-11-13
- Wikipedia, 'Sinking of INS Khukri', https://en.wikipedia.org/wiki/Sinking_of_INS_Khukri, accessed 2026-11-13
