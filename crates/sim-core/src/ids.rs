use serde::{Deserialize, Serialize};

/// Stable unit identifier assigned by the scenario. It never depends on ECS entity
/// allocation, so it survives save/load and replay, and it is the sort key for deterministic iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
// Exported to TypeScript as `number`: ids stay far below 2^53, so the conversion is exact.
pub struct UnitId(#[cfg_attr(feature = "specta", specta(type = u32))] pub u64);

/// Side (faction) identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct SideId(pub u16);

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "U{}", self.0)
    }
}

impl std::fmt::Display for SideId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "S{}", self.0)
    }
}
