//! Ground-plane geometry shared by gameplay and the read-only Inspector.
use serde::Serialize;

use super::Vec2;
pub(super) const HALF_EXTENT: f32 = 10.0;
pub(super) const PLAYER_RADIUS: f32 = 0.5;
pub(super) const CHEST: Vec2 = Vec2 { x: -3.0, y: 0.0 };
pub(super) const CHEST_RANGE: f32 = 2.0;
pub(super) const PORTAL: Vec2 = Vec2 { x: 0.0, y: 7.5 };
pub(super) const PORTAL_RANGE: f32 = 1.25;

/// The existing chest position and interaction distance, in Arena units.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChestGeometry {
    x: f32,
    y: f32,
    interaction_radius: f32,
}
/// The existing portal position and trigger distance, in Arena units.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalGeometry {
    x: f32,
    y: f32,
    trigger_radius: f32,
}
/// Canonical minimap bounds; Arena y is Unity world Z, not screen y.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaGeometry {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    player_radius: f32,
    chest: ChestGeometry,
    portal: PortalGeometry,
}
/// Returns the geometry already used by movement and interaction rules.
///
/// # Examples
/// ```
/// let geometry = kitu_demo_game::arena::geometry::inspection_geometry();
/// let json = serde_json::to_value(geometry)?;
/// assert_eq!(json["portal"]["y"], 7.5);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn inspection_geometry() -> ArenaGeometry {
    ArenaGeometry {
        min_x: -HALF_EXTENT,
        max_x: HALF_EXTENT,
        min_y: -HALF_EXTENT,
        max_y: HALF_EXTENT,
        player_radius: PLAYER_RADIUS,
        chest: ChestGeometry {
            x: CHEST.x,
            y: CHEST.y,
            interaction_radius: CHEST_RANGE,
        },
        portal: PortalGeometry {
            x: PORTAL.x,
            y: PORTAL.y,
            trigger_radius: PORTAL_RANGE,
        },
    }
}
