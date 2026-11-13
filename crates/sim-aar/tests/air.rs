//! Sorties in the AAR (docs/models/air-operations.md): each sortie is a row with its base,
//! mission, loadout, flight time and outcome, and the timeline tells the story.

#![allow(clippy::unwrap_used)] // test harness

use sim_aar::{AarReport, AarTable};
use sim_core::{Command, CommandPayload, GeoPoint, Issuer, SideId, Tick, UnitId};
use sim_scenario::Scenario;

#[test]
fn a_ferry_sortie_is_a_row_from_launch_to_landing() {
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 6, "name": "sortie", "seed": 2,
            "duration_s": 3600,
            "sides": [{"id": 1, "name": "Blue"}],
            "facilities": [{
                "id": 1, "name": "Northfield", "side": 1, "kind": "airbase",
                "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "radius_m": 2000,
                "air": {"runways": 1}
            }],
            "units": [{"id": 1, "name": "Viper", "side": 1, "platform": "GEN-FTR-MR",
                "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "base": 1,
                "loadout": "GEN-FTR-MR/CAP"}]
        })
        .to_string(),
    )
    .unwrap();
    let mut sim = s
        .build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap();
    sim.submit(Command {
        tick: Tick(10),
        issuer: Issuer::WhiteCell {
            umpire: "test".into(),
        },
        side: None,
        payload: CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: sim_core::Mission::Ferry {
                destination: GeoPoint::new(0.4, -170.0),
            },
            loadout: None,
        },
    })
    .unwrap();
    sim.run_until(Tick(1_500));
    let sides = [(SideId(1), "Blue".into())];
    let r = AarReport::build(&sim.command_log(), sim.events(), &sides, sim.tick());
    assert_eq!(r.sorties.len(), 1);
    let row = &r.sorties[0];
    assert_eq!(row.launched, Tick(10));
    assert_eq!(row.base, "Northfield");
    assert_eq!(row.mission, "ferry");
    assert_eq!(row.loadout.as_deref(), Some("GEN-FTR-MR/CAP"));
    assert_eq!(row.outcome, "recovered at Northfield");
    assert!(row.ended.is_some());
    let air: Vec<&str> = r
        .timeline
        .iter()
        .filter(|t| t.kind == sim_aar::TimelineKind::Air)
        .map(|t| t.text.as_str())
        .collect();
    assert!(
        air[0].starts_with("Viper took off from Northfield on a ferry sortie"),
        "{air:?}"
    );
    assert!(air
        .iter()
        .any(|t| t.contains("returning to Northfield: mission complete")));
    assert!(air
        .iter()
        .any(|t| t.starts_with("Viper landed at Northfield")));
    let csv = r.csv(AarTable::Sorties);
    assert!(csv.starts_with("launched,unit,unit_name,side,base,mission,loadout,ended"));
    assert!(csv.contains("Viper,1,Northfield,ferry,GEN-FTR-MR/CAP"));
}
