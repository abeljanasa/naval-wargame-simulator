//! Classification ladder (master prompt §8): unknown → domain → category → class → identified.

use serde::{Deserialize, Serialize};

/// How much a side knows about a track. Ordered: a track only ever climbs the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Something is there.
    #[default]
    Unknown,
    /// Surface, subsurface, air or land.
    Domain,
    /// e.g. surface combatant, merchant, fighter.
    Category,
    /// The platform class, e.g. GEN-FFG.
    Class,
    /// The individual unit.
    Identified,
}

impl Classification {
    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// The level a single radar detection supports. A radar return gives position, height and
/// closing speed, which fix the domain (surface, air, or land by elevation) but not the type.
/// Category and above need ESM, visual or IR/EO identification (later Phase 2 slices).
#[must_use]
pub fn radar_classification() -> Classification {
    Classification::Domain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_is_ordered() {
        assert!(Classification::Unknown < Classification::Domain);
        assert!(Classification::Domain < Classification::Category);
        assert!(Classification::Class < Classification::Identified);
        assert_eq!(
            Classification::Domain.max(radar_classification()),
            Classification::Domain
        );
    }
}
