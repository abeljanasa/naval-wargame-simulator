//! Sensor and track models (master prompt §8), as plain deterministic functions with no ECS
//! dependency (ADR-0005). `sim-core` runs them inside its systems; duel tests call them directly.
//!
//! Model docs: `docs/models/radar-detection-gate1.md` (radar Pd) and `docs/models/tracks.md`
//! (measurement error, fusion, error growth, classification).

#![forbid(unsafe_code)]

mod classify;
mod esm;
mod kind;
mod measurement;
mod radar;
mod sonar;
mod track;

pub use classify::{radar_classification, Classification};
pub use esm::{
    identify, intercept_range_m, triangulate, BearingLine, Carrier, Identification,
    ESM_RANGE_FACTOR, MIN_CROSSING_DEG,
};
pub use kind::SensorKind;
pub use measurement::{gaussian_pair, measurement_error, MeasurementError};
pub use radar::{
    burn_through_range_m, clutter_loss_db, detection_range_m, detection_range_with_loss_m,
    in_main_beam, jamming_to_noise, pd_jammed, pd_per_scan, ClutterBackground, Environment,
    JammerSpec, CLUTTER_CEILING_M, DEFAULT_BEAMWIDTH_DEG, DEFAULT_CLUTTER_DB_PER_SEA_STATE,
    LAND_CLUTTER_SEA_STATE, SIDELOBE_DB,
};
pub use sonar::{
    active_se_db, ambient_noise_db, noise_at_speed_db, passive_se_db, pd_from_se, power_sum_db,
    sonar_params, thorp_absorption_db_per_km, transmission_loss_db, SonarParams,
    DIPPING_MAX_ALTITUDE_M, DIPPING_MAX_SPEED_MPS, SE_SIGMA_DB, SPREADING_TRANSITION_M,
};
pub use track::{measured_point, Cov2, ErrorEllipse, TrackEstimate};
