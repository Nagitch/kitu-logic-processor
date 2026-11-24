//! Integer spatial operations for the synthetic replay.

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Role {
    Wall,
    Target,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BoxMm {
    pub(super) id: String,
    pub(super) role: Role,
    pub(super) min_mm: [i64; 3],
    pub(super) max_mm: [i64; 3],
}

struct Bounds {
    id: String,
    role: Role,
    min: [i64; 3],
    max: [i64; 3],
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Hit<'a> {
    pub(super) id: &'a str,
    pub(super) distance_um: i64,
}

pub(super) struct SpatialState {
    pub(super) position_um: [i64; 3],
    boxes: Vec<Bounds>,
}

impl SpatialState {
    pub(super) fn new(start_mm: [i64; 3], boxes: Vec<BoxMm>) -> Result<Self> {
        let mut state = Self {
            position_um: to_um(start_mm)?,
            boxes: Vec::with_capacity(boxes.len()),
        };
        for shape in boxes {
            ensure!(
                !shape.id.is_empty() && state.boxes.iter().all(|b| b.id != shape.id),
                "expected a unique nonempty box ID: {:?}",
                shape.id,
            );
            let bounds = Bounds {
                id: shape.id,
                role: shape.role,
                min: to_um(shape.min_mm)?,
                max: to_um(shape.max_mm)?,
            };
            ensure!(
                (0..3).all(|axis| bounds.min[axis] < bounds.max[axis]),
                "invalid bounds for {}",
                bounds.id
            );
            ensure!(
                bounds.role != Role::Wall
                    || !bounds.crosses_lane(state.position_um)
                    || !(bounds.min[0] < state.position_um[0]
                        && state.position_um[0] < bounds.max[0]),
                "initial point is inside wall {}",
                bounds.id
            );
            state.boxes.push(bounds);
        }
        Ok(state)
    }

    pub(super) fn move_x(&mut self, dx_um: i64) -> Result<()> {
        let start = self.position_um[0];
        let mut end = i128::from(start) + i128::from(dx_um);
        for wall in self
            .boxes
            .iter()
            .filter(|b| b.role == Role::Wall && b.crosses_lane(self.position_um))
        {
            if dx_um > 0 && start <= wall.min[0] {
                end = end.min(i128::from(wall.min[0]));
            } else if dx_um < 0 && start >= wall.max[0] {
                end = end.max(i128::from(wall.max[0]));
            }
        }
        self.position_um[0] = i64::try_from(end).context("movement exceeds i64 micrometres")?;
        Ok(())
    }

    pub(super) fn hit_x(&self, range_um: i64) -> Option<Hit<'_>> {
        let start = self.position_um[0];
        let (distance, _, id) = self
            .boxes
            .iter()
            .filter(|b| b.crosses_lane(self.position_um) && b.max[0] >= start)
            .filter_map(|b| {
                let distance = i128::from(b.min[0].max(start)) - i128::from(start);
                (distance <= i128::from(range_um)).then_some((distance, b.role, b.id.as_str()))
            })
            .min()?;
        Some(Hit {
            id,
            distance_um: distance as i64,
        })
    }
}

impl Bounds {
    fn crosses_lane(&self, point: [i64; 3]) -> bool {
        (1..3).all(|axis| (self.min[axis]..=self.max[axis]).contains(&point[axis]))
    }
}

fn to_um(point_mm: [i64; 3]) -> Result<[i64; 3]> {
    let mut point_um = [0; 3];
    for (dest, source) in point_um.iter_mut().zip(point_mm) {
        *dest = source
            .checked_mul(1000)
            .context("mm value exceeds i64 micrometres")?;
    }
    Ok(point_um)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(id: &str, role: Role, min_x: i64, max_x: i64) -> BoxMm {
        BoxMm {
            id: id.into(),
            role,
            min_mm: [min_x, -1, -1],
            max_mm: [max_x, 1, 1],
        }
    }

    #[test]
    fn sweep_stops_at_both_faces_and_can_move_away() {
        let mut spatial = SpatialState::new(
            [0; 3],
            vec![
                shape("right", Role::Wall, 1, 2),
                shape("left", Role::Wall, -2, -1),
            ],
        )
        .unwrap();
        spatial.move_x(i64::MAX).unwrap();
        assert_eq!(spatial.position_um[0], 1000);
        spatial.move_x(i64::MIN).unwrap();
        assert_eq!(spatial.position_um[0], -1000);
        spatial.move_x(1).unwrap();
        assert_eq!(spatial.position_um[0], -999);
    }

    #[test]
    fn hit_includes_endpoint_and_breaks_ties_with_role_then_id() {
        let spatial = SpatialState::new(
            [-1, 1, 1],
            vec![
                shape("a_target", Role::Target, 0, 1),
                shape("z_wall", Role::Wall, 0, 1),
                shape("b_wall", Role::Wall, 0, 1),
            ],
        )
        .unwrap();
        assert!(spatial.hit_x(999).is_none());
        assert_eq!(
            spatial.hit_x(1000),
            Some(Hit {
                id: "b_wall",
                distance_um: 1000
            })
        );
    }

    #[test]
    fn rejects_conversion_and_motion_overflow() {
        assert!(SpatialState::new([i64::MAX, 0, 0], vec![]).is_err());
        let mut spatial = SpatialState::new([-1, 0, 0], vec![]).unwrap();
        assert!(spatial.move_x(i64::MIN).is_err());
        assert_eq!(spatial.position_um[0], -1000);
    }
}
