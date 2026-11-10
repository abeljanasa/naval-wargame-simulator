# DL-0007: Scenario units reference platform-database classes

- **Date:** 2026-11-10
- **Source:** Original design, following master prompt §4.6 ("platforms … live in data, not code").
- **Decisions:**
  - **Format:** scenario format v2 adds an optional `"platform": "<db id>"`. The database supplies the domain, max and cruise speed, acceleration, fitted sensors and RCS. Any value written inline overrides it. v1 files, which put everything inline, still load.
  - **Self-contained commands:** the resolved values are copied **into the spawn command**, not referenced. Replays and saves therefore don't depend on later database edits: a replay reproduces the original run even after the database changes.
  - **Embedded database:** the desktop app and batch runner build the open database from sources compiled into the binary (`sim_db::embedded`), so they need no data files and work air-gapped. `nws-batch --db <file>` loads a built database instead.
