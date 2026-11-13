# DL-0048: Regional database, first tranche, and ECM records

- **Date:** 2026-11-13
- **Phase:** 4, slice 13
- **Sources:**
  - Master prompt §3, §9 (the database and its provenance), clean-room rules 1–2.
  - [ADR-0007](../adr/0007-phase-4-region-and-data.md) (open-source UNCLASSIFIED data only, with citations).
  - The client's choice of tranche (2026-11-13): the Philippine Navy and Coast Guard, and the PLAN (Southern Theater) and China Coast Guard, first.
  - Public encyclopaedic articles (Wikipedia) for each class, cited on each record with the date accessed.
  - Original design for the ECM schema.

## Decisions

- **Specific records must cite a source.** Every non-generic record carries a citation with a URL, a `doi:` or an ISBN. A new error rule, `provenance.not_generic`, replaces the warning that flagged any non-generic record in the open build.
- **Confidence C for published figures, D for estimates.** Dimensions, speeds, endurance and fits come from the cited article (C). Signatures are never published, so they are copied from the matching generic class, with their own D provenance. A signature may now carry provenance that overrides its platform's.
- **Generic sensors and weapons where nothing better is open.** A named radar or sonar gets the generic record of its kind; the citation names the real system. Named weapons are added only where the record needs them (anti-ship and surface-to-air missiles of the tranche and the historical engagements), with open ranges and the generic class's Ph (D).
- **ECM as records** (migration 0004). The tables are `ecm_systems` (noise or deception jammer, J/N at 1 km) and `platform_ecm`. A unit jams with its platform's strongest ECM unless the scenario gives it a jammer; scenarios that give one are unchanged. A new rule, `ecm.values`, checks the strength (0–100 dB).
- **Historical platforms in their own file** (`historical.json`), for the Gate 4 engagements, so they are not mistaken for current forces.
- **The emitter library grows with the database.** The ESM library lists every database class that carries a radar. The new classes reuse generic radars, so an intercept has more candidate classes. This changed every golden replay, including Phase 1; checked: with the library limited to generic classes, every old hash reproduces. The goldens were re-blessed. A real system's radar gets its own record when an open source gives its figures.
- **No Type 818 or other records without an open source.**
- **Second tranche (2026-11-13, at the client's request): Vietnam, the United States and Japan.** 18 classes and 15 weapons, by the same rules. Choices:
  - US forces are those that operate in the PAR: Flight IIA destroyers, Independence-class LCS, Virginia-class submarines, the Legend-class cutter, P-8A and F/A-18E/F. The Ticonderoga class was left out (its article gives its figures only in sub-templates, and the class is retiring).
  - The Vietnam Coast Guard's open-sourced class is the ex-US Hamilton-class cutter (CSB 8020); no open figures were found for its DN-2000 OPV.
  - VLS splits and tube loads are not published; each record states the assumed split.
  - Jammers only where the source names a jammer or electronic attack (Gepard's Bell Squat, Mogami's NOLQ-3E), with generic strength (D).
  - The golden replays changed again for the same reason as the first tranche (the emitter library), checked the same way, and were re-blessed.
- **Third tranche (2026-11-13): Malaysia, Indonesia, Australia, South Korea and Taiwan.** 25 classes and 13 weapons, by the same rules. Choices:
  - Coast guards where an article gives a class's figures: the MMEA's Tun Fatimah, Bakamla's Tanjung Datu, Taiwan's Chiayi. The Korea Coast Guard article names no classes, and the Australian Border Force was not covered.
  - The Haeseong (SSM-700K) reuses the first tranche's C-Star record.
  - Weapons with no article fetched are modelled by generic records and say so: Taiwan's SUT and Korea's Tiger Shark torpedoes, KM-SAM (left out).
  - Where an article gives several variants, the record says which it uses: the Malaysian Scorpène's figures are the conventional CM-2000's, Tuo Chiang is Flight I, Kang Ding is after its combat-system upgrade, Anzac is the RAN anti-ship missile defence fit.
  - Jammers where the source names ECM: the Lekiu's Scimitar, the Martadinata's Scorpion 2L and the Korean SLQ-200(V)K Sonata suite.
  - Golden replays re-blessed again, after the same check.

## Open

- Further navies and agencies as the client's scenarios need them (for example Singapore, Thailand, Brunei, the Korea Coast Guard, the Australian Border Force).
- The Vietnam Coast Guard's DN-2000 and the Ticonderoga class, if open figures are found.
- Named sensor records as open figures are found.
- Reference-data editing in the app, if the data team needs it.
