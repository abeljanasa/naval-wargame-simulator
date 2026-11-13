# DL-0051: Weapon ranges by target domain

- **Date:** 2026-11-13
- **Phase:** 4, Gate 4 finding 2 ([Gate 4 pack](../gate4/README.md))
- **Sources:**
  - The Gate 4 Praying Mantis draft, in which a frigate's 127 mm gun shot down A-6Es 18 km away.
  - Open figures for small guns against aircraft: Oerlikon 35 mm KCB 3,000 m, Bofors 40 mm L/70 4 km (Wikipedia, accessed 2026-11-13).
  - Original design for the generic values.

## Decisions

- **A range per target domain, where it differs.** A new table, `weapon_target_ranges` (migration 0005), holds a weapon's range against a domain. The weapon's maximum range applies to every other domain. The engine uses it when checking an `Engage` order and in point defence, where incoming weapons are air targets.
- **Generic guns:**
  - the 127 mm-class reaches aircraft at 12 km (surface 24 km);
  - the 76 mm-class at 8 km (surface 16 km);
  - the 30 mm-class's 3 km already fits.

  These are about half the surface range, reflecting fire control and time of flight against a manoeuvring target. They are generic estimates, confidence D, for SME review.
- **New validation rule `weapon.target_ranges`:** a range must be for a domain the weapon targets, above its minimum range and no more than its maximum.

## Effect on Gate 4

Sahand's gun no longer reaches the A-6Es at their stand-off stations. See the regenerated [report](../gate4/validation/praying-mantis-1988/report.md).

## Open

- Ranges against air targets for named guns and dual-purpose missiles, as open figures are found.
- Altitude limits (a gun's ceiling) as well as range.
