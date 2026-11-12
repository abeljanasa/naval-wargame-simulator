//! Headless simulation core.
//!
//! The UI, batch runner and multiplayer host never touch the ECS world directly. They submit
//! [`Command`]s and read [`Snapshot`]s through [`Simulation`]. See ADR-0003 for the determinism rules.

#![forbid(unsafe_code)]

mod adjudication;
mod ai;
mod c2;
mod combat;
mod command;
mod comms;
mod components;
mod detection;
mod error;
mod esm;
mod events;
mod hash;
mod ids;
mod mines;
mod movement;
mod optical;
mod planning;
mod point_defence;
mod rng;
mod save;
mod scenario_events;
mod seats;
mod sim;
mod snapshot;
mod sonar;
mod time;

pub use adjudication::{
    Adjudication, AdjudicationLog, AdjudicationRecord, ADJUDICATION_VIEW_LIMIT,
};
pub use ai::MissionState;
pub use c2::C2State;
pub use combat::{
    default_damage_profile, Health, Magazine, WeaponCatalog, WeaponInFlight, WeaponsInFlight,
    DAMAGE_CONTROL_PERIOD_TICKS, FUEL_RANGE_FACTOR,
};
pub use command::{
    Command, CommandPayload, DoctrineScope, EmitterEntry, Issuer, LoggedCommand, UnitSpawn,
    PROTOCOL_VERSION,
};
pub use comms::{CommsView, PostView};
pub use components::{Domain, Kinematics, Position, Route, Unit};
pub use detection::{
    clutter_background, default_rcs_m2, detection_range_m, domain_speed_bound_mps, pd_per_scan,
    sensor_height_m, target_height_m, Contact, ContactRecord, EnvState, Jammer, SensorKind,
    SensorSpec, SensorSuite, Signature, CONTACT_TIMEOUT_TICKS, SENSOR_PERIOD_TICKS,
};
pub use error::SimError;
pub use esm::{
    BearingRecord, BearingTable, BearingView, EmitterLibrary, BEARING_MAX_AGE_TICKS, JAMMER_EMITTER,
};
pub use events::{SimEvent, SimEventKind};
pub use hash::StateHash;
pub use ids::{SideId, UnitId};
pub use mines::{Minefield, Minefields, MINE_CHECK_PERIOD_TICKS};
pub use optical::default_length_m;
pub use planning::{
    validate_planning, Diversion, Facility, FacilityKind, GapPolicy, Loadout, PlanningLayer,
    QueuedOrder, ServiceState, Services, Zone, ZoneKind, ALONGSIDE_SPEED_MPS, DEFAULT_HANDOVER_S,
    REARM_TICKS, REPAIR_FRACTION_PER_MIN, SERVICE_PERIOD_TICKS,
};
pub use point_defence::{PointDefence, Shot};
pub use rng::{RngStreams, StreamId};
pub use save::{SaveGame, UnitRecord, SAVE_FORMAT_VERSION, SAVE_MAGIC};
pub use scenario_events::{
    EventEngine, EventStatus, Facilitation, InjectStatus, Message, MessageLog, EXERCISE_CONTROL,
    MESSAGE_VIEW_LIMIT,
};
pub use seats::{validate_seats, Seat, SeatRole, Seats};
pub use sim::Simulation;
pub use sim_ai::{Mission, AI_PERIOD_TICKS};
pub use sim_c2::comms::LinkStatus;
pub use sim_c2::{Doctrine, Emcon, Posture, ResolvedDoctrine, Wcs};
pub use sim_events::{
    validate_events, Area, Audience, Cmp, Condition, DoctrineTarget, EventAction, EventRuntime,
    Inject, ScenarioEvent, Trigger,
};
pub use sim_sensors::{
    BearingLine, Carrier, Classification, Cov2, Environment, ErrorEllipse, JammerSpec,
};
pub use sim_weapons::{DamageProfile, Guidance, Status, System, WeaponKind, WeaponSpec};
pub use snapshot::{
    ContactView, PostureView, RouteView, Snapshot, StoreView, UnitView, Viewer, WeaponView,
};
pub use sonar::{
    default_radiated_noise_db, sonar_usable, ACOUSTIC_LABEL, DEFAULT_SUBMARINE_NOISE_DB,
    DEFAULT_SURFACE_NOISE_DB, TARGET_STRENGTH_SUBMARINE_DB, TARGET_STRENGTH_SURFACE_DB,
};
pub use time::{SimClock, Tick, TICK_SECONDS};

pub use sim_geo::GeoPoint;
