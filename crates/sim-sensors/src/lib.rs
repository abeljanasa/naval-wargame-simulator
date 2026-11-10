//! Sensor and track models (master prompt §8), as plain deterministic functions with no ECS
//! dependency (ADR-0005). `sim-core` runs them inside its systems; duel tests call them directly.
//!
//! Model docs: `docs/models/radar-detection-gate1.md` (radar Pd) and `docs/models/tracks.md`
//! (measurement error, fusion, error growth, classification).

#![forbid(unsafe_code)]

mod classify;
mod kind;
mod measurement;
mod radar;
mod track;

pub use classify::{radar_classification, Classification};
pub use kind::SensorKind;
pub use measurement::{gaussian_pair, measurement_error, MeasurementError};
pub use radar::{detection_range_m, pd_per_scan};
pub use track::{measured_point, Cov2, ErrorEllipse, TrackEstimate};
