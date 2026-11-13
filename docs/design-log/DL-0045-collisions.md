# DL-0045: Ship collisions and collision avoidance

- **Date:** 2026-11-13
- **Phase:** 4, out of slice order (the client found that ships passed through each other)
- **Sources:**
  - COLREGS (the International Regulations for Preventing Collisions at Sea, 1972, public), rules 13–17: overtaking, head-on, crossing, give-way and stand-on action.
  - Public accounts of the USS *Fitzgerald* and USS *John S. McCain* collisions (2017, US Navy reports), used to calibrate the damage scale.
  - Closest-point-of-approach geometry from standard navigation texts.
  - Original design otherwise.

## Decisions

- **Damage from energy, scaled to each hull.** The share of collision energy a hull absorbs, set by the other's mass, over its own kinetic energy at a reference speed gives f = (m_o/(m_s+m_o))²·(v/V)². That is dimensionless and needs only displacement, which the database has. It also makes the light hull take most of the damage, as in every recorded case.
- **V = 14 m/s, calibrated on *Fitzgerald*.** It gives the destroyer about half its damage points (a mission kill, as happened) and the container ship a few per cent (minor damage, as happened). An explosive (TNT-equivalent) scale was rejected: it puts a 30-knot collision at a few kilograms of warhead, far below what collisions do to hulls.
- **The weapon hit's damage roll.** Collisions reuse the same draws (critical, system, fire, flooding), so damage control, criticals and losses work unchanged. A hard collision holes the hull and floods as a torpedo does. A dent below 1 % of damage points takes no roll, so berthing-speed bumps do not start fires.
- **Avoidance by the rules of the road, stateless except for hysteresis.**
  - Each tick, each hull checks its intended course and takes the smallest starboard turn that clears. This needs no stored plan, so it is deterministic and survives saves trivially.
  - The only state is whom it is giving way to, and the heading to resume. This stops it chattering at a rule boundary and lets a hull with no route come back to its ordered heading. It is saved, and hashed only when present.
  - Close quarters makes everyone act, as rule 17 requires.
- **Own side included**, as the client asked. Real ships in company also keep clear of each other.
- **What a ship knows.** A ship does not avoid another side's submerged submarine, because it cannot see it. That is the realistic case in which collisions still happen, and it keeps collisions possible in play.
- **Golden replays re-blessed.** Avoidance changes the tracks of four shipped scenarios; none has a collision. We checked that with avoidance switched off, the old hashes are reproduced exactly, so nothing else changed.

## Open

- Damage by the angle of the blow, and which hull's bow struck.
- Deliberate ramming orders.
- Close-formation work (replenishment alongside, towing) exempt from avoidance.
- Avoidance of contacts a side knows only as tracks, rather than of the true hulls.
