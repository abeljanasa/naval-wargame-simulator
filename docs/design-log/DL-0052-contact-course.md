# DL-0052: Contact course estimate, and contacts turned to it on the map

- **Date:** 2026-11-14
- **Phase:** 4
- **Sources:**
  - The client's request that contacts be drawn like own units: flat on the map and turned to their heading.
  - Least-squares straight-line fitting (public statistics literature).
  - Original design otherwise.

## Decisions

- **A course the side has measured, not the target's heading.** Turning a contact to its true heading would show the side what it cannot know (design principle 4). Each track estimates its course from its own position estimates. The snapshot carries `course_deg` and `speed_mps` only once that estimate exists.
- **Line fit over 6 minutes, with a significance test.** The first version took the move between two track estimates once it exceeded 3σ. But the track error understates scan-to-scan jitter (about 350 m at 20 km from a 1° bearing error), so a stopped target got a course and a moving one was 17° off. A least-squares line through positions every 15 s, accepted only when the speed is at least 3 standard errors, averages the jitter out. In the engine tests the error is under 15°, and a stopped target has none.
- **Display only.** The estimate does not move the track or feed weapons. Engagement behaviour is unchanged, so the Gate 4 outcomes stand.
- **Rendering:**
  - A classified contact with a course is drawn flat on the map, turned to it, and keeps it in a rotated or tilted view.
  - A newly acquired contact, a stopped one, or an unclassified one (APP-6 frame) faces the screen: its course is not known.
  - Classified contacts carry the affiliation badge beside their label, as own units do.
- **State:** each track's history is saved (format 27, default empty) and hashed, so the golden replays were re-blessed. The outcomes are unchanged.

## Open

- Using the course in the track estimate (a constant-velocity filter), so the estimate moves between updates.
