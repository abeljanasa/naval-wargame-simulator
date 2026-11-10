use serde::{Deserialize, Serialize};

/// Sensor kinds, matching the platform database `sensors.kind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    Radar,
    SonarHull,
    SonarTowed,
    SonarDipping,
    Sonobuoy,
    Esm,
    Ir,
    Eo,
    Visual,
    Mad,
}

impl SensorKind {
    /// Parses a platform-database sensor kind.
    #[must_use]
    pub fn from_db(kind: &str) -> Option<Self> {
        Some(match kind {
            "radar" => Self::Radar,
            "sonar_hull" => Self::SonarHull,
            "sonar_towed" => Self::SonarTowed,
            "sonar_dipping" => Self::SonarDipping,
            "sonobuoy" => Self::Sonobuoy,
            "esm" => Self::Esm,
            "ir" => Self::Ir,
            "eo" => Self::Eo,
            "visual" => Self::Visual,
            "mad" => Self::Mad,
            _ => return None,
        })
    }

    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_kinds_round_trip_from_database_names() {
        for k in [
            "radar",
            "sonar_hull",
            "sonar_towed",
            "sonar_dipping",
            "sonobuoy",
            "esm",
            "ir",
            "eo",
            "visual",
            "mad",
        ] {
            assert!(SensorKind::from_db(k).is_some(), "{k}");
        }
        assert_eq!(SensorKind::from_db("laser"), None);
    }
}
