# Naval Wargame Simulator (working title)

An original, cross-platform naval operational and tactical wargaming simulator. It has a headless, deterministic Rust engine with a Tauri 2 desktop app and a batch runner. **Unclassified open build:** no classified data is ever stored in this repository.

Status: **Phase 3 (TTX release 1.0)**, built and awaiting the Gate 3 review and navy TTX pilot. See the [Gate 3 review pack](docs/gate3/README.md), the [CHANGELOG](CHANGELOG.md) and the [backlog](docs/backlog.md).

**Manuals:** [user](docs/manuals/user-manual.md) · [white cell](docs/manuals/white-cell-manual.md) · [scripting reference](docs/manuals/scripting-reference.md)

## Build and run (≤ 5 commands)

Prerequisites:

- Rust (rustup; the version is pinned in `rust-toolchain.toml`)
- Node 20+
- **Windows:** VS 2022 C++ Build Tools

```sh
cargo test --workspace                                                       # engine tests
cargo run -p sim-db -- validate                                              # build + validate the platform database
cargo run -p nws-batch -- run data/scenarios/phase1-strait-transit.json      # headless run, prints the state hash
cd apps/desktop && npm install                                               # UI dependencies
npx tauri dev                                                                # desktop app
```

In the app:

- **Space** runs or pauses; **1–7** set time compression; **.** steps one tick.
- Select a unit, choose **Plan new route…**, click the map to add waypoints, then press **Enter**.
- Switch **Seat** between a side and the white cell (ground truth). The white cell has the console (overrides, injects, events, zone rules, seats), **Exercise review** (replay and AAR) and the scenario editor.

Headless: `cargo run -p nws-batch -- aar data/scenarios/phase3-planning-demo.json --out aar/` runs a scenario, records it, checks the replay, and writes the AAR tables as CSV.

## Layout

| Path | Contents |
|---|---|
| `crates/sim-core` | ECS world, fixed-step clock, RNG streams, commands, snapshots, saves; movement, detection, combat, missions, events, seats, adjudication, planning layer |
| `crates/sim-geo` | WGS-84 geodesics, radar horizon and coastline (deterministic `libm` maths) |
| `crates/sim-sensors`, `sim-weapons`, `sim-c2`, `sim-ai`, `sim-events` | Pure model rules: sensors, weapons and damage, C2 doctrine, missions, events and sandboxed Lua |
| `crates/sim-aar` | Exercise records, verified replay with keyframes, AAR tables and CSV |
| `crates/sim-scenario` | Scenario JSON format, validation (`check`) and loader; determinism and golden-replay tests |
| `crates/sim-db` | Platform database: schema migrations, JSON source loader, validation suite, `sim-db` CLI |
| `data/db/open` | Open generic platform database source (JSON, UNCLASSIFIED); see [data dictionary](docs/db/data-dictionary.md) |
| `apps/batch` | Headless CLI runner |
| `apps/desktop` | Tauri 2 app (`src-tauri`) + Svelte 5 UI (`src`) |
| `data/scenarios` | Shipped scenarios |
| `tests/golden` | Golden replay hashes (update with `NWS_BLESS=1`, explained in the PR) |
| `docs/` | `manuals/`, `adr/`, `models/` (behaviour docs), `design-log/`, `plan/`, `gate2/`, `gate3/`, `backlog.md` |

Planned crates, not yet created: `sim-analysis` (Phase 4); `sim-net` (Phase 5).

## Building releases

See [docs/build/releasing.md](docs/build/releasing.md) for installers, signing, the release scan and SBOMs, [docs/build/air-gapped.md](docs/build/air-gapped.md) for offline builds, and [docs/build/performance.md](docs/build/performance.md) for the performance budgets.

## Ground rules

- **Clean room:** we match published features, never another product's code, data, names or UI. Every feature has a design-log entry ([docs/design-log](docs/design-log/README.md)).
- **Determinism:** same scenario + seed + command log gives a bit-identical result on every OS ([ADR-0003](docs/adr/0003-determinism-strategy.md)).
- **Data security:** only unclassified, generic data is allowed here. Accredited data loads through a separate extension point on approved systems.
