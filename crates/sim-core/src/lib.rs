//! Headless simulation core.
//!
//! The UI, batch runner and multiplayer host never touch the ECS world directly. They submit
//! [`Command`]s and read [`Snapshot`]s through [`Simulation`]. See ADR-0003 for the determinism rules.

#![forbid(unsafe_code)]

mod c2;
mod command;
mod components;
mod detection;
mod error;
mod events;
mod hash;
mod ids;
mod movement;
mod rng;
mod save;
mod sim;
mod snapshot;
mod time;

pub use c2::C2State;
pub use command::{
    Command, CommandPayload, DoctrineScope, Issuer, LoggedCommand, UnitSpawn, PROTOCOL_VERSION,
};
pub use components::{Domain, Kinematics, Position, Route, Unit};
pub use detection::{
    default_rcs_m2, detection_range_m, domain_speed_bound_mps, pd_per_scan, sensor_height_m,
    target_height_m, Contact, ContactRecord, SensorKind, SensorSpec, SensorSuite, Signature,
    CONTACT_TIMEOUT_TICKS, SENSOR_PERIOD_TICKS,
};
pub use error::SimError;
pub use events::{SimEvent, SimEventKind};
pub use hash::StateHash;
pub use ids::{SideId, UnitId};
pub use rng::{RngStreams, StreamId};
pub use save::{SaveGame, UnitRecord, SAVE_FORMAT_VERSION, SAVE_MAGIC};
pub use sim::Simulation;
pub use sim_c2::{Doctrine, Emcon, Posture, ResolvedDoctrine, Wcs};
pub use sim_sensors::{Classification, Cov2, ErrorEllipse};
pub use snapshot::{ContactView, PostureView, RouteView, Snapshot, UnitView, Viewer};
pub use time::{SimClock, Tick, TICK_SECONDS};

pub use sim_geo::GeoPoint;
