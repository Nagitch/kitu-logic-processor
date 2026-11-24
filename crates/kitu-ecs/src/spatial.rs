//! Integer spatial queries for applications that keep authoritative positions in micrometres.

use std::{cmp::Ordering, collections::BTreeMap};

use kitu_core::{KituError, Result};

const COORDINATE_OVERFLOW: KituError = KituError::InvalidInput("spatial coordinate overflow");

/// Authored position in signed integer millimetres, in XYZ order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionMm(
    /// XYZ coordinates.
    pub [i64; 3],
);

impl PositionMm {
    /// Converts millimetres to micrometres, checking each axis for overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_ecs::spatial::{PositionMm, PositionUm};
    /// assert_eq!(PositionMm([-1, 2, 0]).to_micrometres().unwrap(), PositionUm([-1000, 2000, 0]));
    /// ```
    pub fn to_micrometres(self) -> Result<PositionUm> {
        let mut axes = self.0;
        for axis in &mut axes {
            *axis = axis.checked_mul(1000).ok_or(COORDINATE_OVERFLOW)?;
        }
        Ok(PositionUm(axes))
    }
}

/// Authoritative position in signed integer micrometres, in XYZ order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionUm(
    /// XYZ coordinates.
    pub [i64; 3],
);

impl PositionUm {
    /// Projects this position to metres for presentation. One unit is one metre.
    pub fn to_metres(self) -> [f64; 3] {
        self.0.map(|axis| axis as f64 / 1_000_000.0)
    }

    fn offset(self, delta: DisplacementUm) -> Result<Self> {
        let mut axes = self.0;
        for (axis, displacement) in axes.iter_mut().zip(delta.0) {
            *axis = axis.checked_add(displacement).ok_or(COORDINATE_OVERFLOW)?;
        }
        Ok(Self(axes))
    }
}

/// Displacement in signed integer micrometres, in XYZ order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplacementUm(
    /// XYZ displacement.
    pub [i64; 3],
);

/// Axis-aligned bounds with strictly positive extent on every axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AabbUm {
    min: PositionUm,
    max: PositionUm,
}

impl AabbUm {
    /// Constructs validated micrometre bounds.
    pub fn new(min: PositionUm, max: PositionUm) -> Result<Self> {
        if (0..3).any(|axis| min.0[axis] >= max.0[axis]) {
            return Err(KituError::InvalidInput(
                "spatial bounds must have positive extent",
            ));
        }
        Ok(Self { min, max })
    }

    /// Constructs bounds from authored millimetre positions.
    pub fn from_mm(min: PositionMm, max: PositionMm) -> Result<Self> {
        Self::new(min.to_micrometres()?, max.to_micrometres()?)
    }

    /// Returns the minimum corner.
    pub fn min(self) -> PositionUm {
        self.min
    }

    /// Returns the maximum corner.
    pub fn max(self) -> PositionUm {
        self.max
    }

    fn expanded(self, half_extents_um: [i64; 3]) -> Result<Self> {
        let mut bounds = self;
        for (axis, extent) in half_extents_um.into_iter().enumerate() {
            bounds.min.0[axis] = self.min.0[axis]
                .checked_sub(extent)
                .ok_or(COORDINATE_OVERFLOW)?;
            bounds.max.0[axis] = self.max.0[axis]
                .checked_add(extent)
                .ok_or(COORDINATE_OVERFLOW)?;
        }
        Ok(bounds)
    }

    fn inward_distances(self, position: PositionUm) -> [(i128, i128); 3] {
        std::array::from_fn(|axis| {
            (
                i128::from(position.0[axis]) - i128::from(self.min.0[axis]),
                i128::from(self.max.0[axis]) - i128::from(position.0[axis]),
            )
        })
    }

    fn penetration_depth(self, position: PositionUm) -> Option<i128> {
        let depth = self
            .inward_distances(position)
            .into_iter()
            .map(|(low, high)| low.min(high))
            .min()?;
        (depth > 0).then_some(depth)
    }

    fn moves_toward_nearest_face(
        self,
        position: PositionUm,
        delta: DisplacementUm,
        depth: i128,
    ) -> bool {
        self.inward_distances(position)
            .into_iter()
            .zip(delta.0)
            .any(|((low, high), d)| (low == depth && d < 0) || (high == depth && d > 0))
    }
}

/// Determines whether a static collider blocks movement and how it ranks on a hit tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColliderRole {
    /// Stops movement and takes precedence on ties in segment queries.
    Obstacle,
    /// Participates in segment queries.
    Target,
}

/// An exact fraction of a segment in reduced terms, between zero and one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentFraction {
    numerator: u64,
    denominator: u64,
}

impl SegmentFraction {
    /// Returns the numerator and the positive denominator.
    pub fn ratio(self) -> (u64, u64) {
        (self.numerator, self.denominator)
    }
}

/// Nearest collider intersected by a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHit<'a> {
    /// Stable collider ID.
    pub id: &'a str,
    /// Collider role.
    pub role: ColliderRole,
    /// Exact fraction of the supplied segment where the hit begins.
    pub fraction: SegmentFraction,
}

/// Result of sweeping a point or axis-aligned box along a displacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionResult<'a> {
    /// Final position in micrometres.
    pub position: PositionUm,
    /// Stable ID of the collider that limited the movement, when applicable.
    pub obstacle_id: Option<&'a str>,
}

#[derive(Clone, Copy)]
struct Collider {
    bounds: AabbUm,
    role: ColliderRole,
}

#[derive(Clone, Copy)]
struct Entry<'a> {
    id: &'a str,
    collider: Collider,
    dynamic: bool,
}

/// Static colliders and a replaceable snapshot of integer dynamic obstacles.
///
/// Store this world and application positions as typed ECS resources. Applications
/// can use it on its own or populate the dynamic layer from the runtime physics API.
pub struct SpatialWorld {
    statics: BTreeMap<String, Collider>,
    dynamics: Option<BTreeMap<String, AabbUm>>,
}

impl Default for SpatialWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialWorld {
    /// Creates an empty world with both spatial layers available.
    pub fn new() -> Self {
        Self {
            statics: BTreeMap::new(),
            dynamics: Some(BTreeMap::new()),
        }
    }

    /// Registers a static collider with a unique nonempty ID.
    pub fn insert_static(
        &mut self,
        id: impl Into<String>,
        bounds: AabbUm,
        role: ColliderRole,
    ) -> Result<()> {
        let id = id.into();
        let dynamics = self.ready_dynamics()?;
        if id.is_empty() || self.statics.contains_key(&id) || dynamics.contains_key(&id) {
            return Err(KituError::InvalidInput(
                "spatial collider ID must be nonempty and unique",
            ));
        }
        self.statics.insert(id, Collider { bounds, role });
        Ok(())
    }

    /// Atomically replaces the dynamic obstacle snapshot. IDs must be unique across both layers.
    pub fn replace_dynamic_obstacles(
        &mut self,
        obstacles: impl IntoIterator<Item = (String, AabbUm)>,
    ) -> Result<()> {
        self.ready_dynamics()?;
        let mut next = BTreeMap::new();
        for (id, bounds) in obstacles {
            if id.is_empty() || self.statics.contains_key(&id) || next.insert(id, bounds).is_some()
            {
                return Err(KituError::InvalidInput(
                    "spatial collider ID must be nonempty and unique",
                ));
            }
        }
        self.dynamics = Some(next);
        Ok(())
    }

    /// Invalidates dynamic data and stops queries until the application rebuilds this world.
    pub fn invalidate_dynamic_obstacles(&mut self) {
        self.dynamics = None;
    }

    /// Returns the valid dynamic snapshot in stable ID order.
    pub fn dynamic_obstacles(&self) -> Result<impl Iterator<Item = (&str, AabbUm)> + '_> {
        Ok(self
            .ready_dynamics()?
            .iter()
            .map(|(id, bounds)| (id.as_str(), *bounds)))
    }

    /// Intersects a finite segment, including both endpoints. A zero delta queries one point.
    ///
    /// When supplied, the excluded ID is skipped. Exact ties prefer obstacles,
    /// then the lexicographically first ID.
    pub fn hit_segment(
        &self,
        start: PositionUm,
        delta: DisplacementUm,
        excluded_id: Option<&str>,
    ) -> Result<Option<SegmentHit<'_>>> {
        start.offset(delta)?;
        let hit = self
            .entries()?
            .filter(|entry| Some(entry.id) != excluded_id)
            .filter_map(|entry| {
                intersection(entry.collider.bounds, start, delta, Boundary::Closed)
                    .map(|time| (time, entry))
            })
            .min_by(|(a, left), (b, right)| {
                a.compare(*b)
                    .then(left.collider.role.cmp(&right.collider.role))
                    .then(left.id.cmp(right.id))
            });
        Ok(hit.map(|(time, entry)| SegmentHit {
            id: entry.id,
            role: entry.collider.role,
            fraction: time.fraction(),
        }))
    }

    /// Sweeps an axis-aligned box, stopping at the first entrance into an obstacle.
    ///
    /// Zero half extents represent a point. Touching and tangential movement are
    /// allowed. For overlaps caused by a moving obstacle, the motion must go
    /// toward a nearest face and reduce each existing overlap. A starting point
    /// inside a static obstacle is an error.
    pub fn move_aabb(
        &self,
        start: PositionUm,
        half_extents_um: [i64; 3],
        delta: DisplacementUm,
    ) -> Result<MotionResult<'_>> {
        if half_extents_um.into_iter().any(|value| value < 0) {
            return Err(KituError::InvalidInput(
                "movement half extents must be nonnegative",
            ));
        }
        start.offset(delta)?;

        let mut stop = None;
        for entry in self.obstacles()? {
            let bounds = entry.collider.bounds.expanded(half_extents_um)?;
            let time = match bounds.penetration_depth(start) {
                Some(_) if !entry.dynamic => {
                    return Err(KituError::InvalidInput(
                        "movement starts inside a static obstacle",
                    ));
                }
                Some(depth) => {
                    (!bounds.moves_toward_nearest_face(start, delta, depth)).then_some(Time::ZERO)
                }
                None => intersection(bounds, start, delta, Boundary::Interior),
            };
            if let Some(time) = time {
                if stop.is_none_or(|(previous, id)| {
                    time.compare(previous).then(entry.id.cmp(id)).is_lt()
                }) {
                    stop = Some((time, entry.id));
                }
            }
        }

        let position = stop.map_or_else(
            || start.offset(delta),
            |(time, _)| start.offset(time.scale(delta)),
        )?;
        if position != start {
            // Rounding each component toward start can leave the exact swept line.
            let mut overlap = None;
            for entry in self.obstacles()? {
                let bounds = entry.collider.bounds.expanded(half_extents_um)?;
                if let Some(depth) = bounds.penetration_depth(position) {
                    if bounds
                        .penetration_depth(start)
                        .is_none_or(|initial| depth >= initial)
                    {
                        overlap = Some(overlap.map_or(entry.id, |id: &str| id.min(entry.id)));
                    }
                }
            }
            if let Some(id) = overlap {
                return Ok(MotionResult {
                    position: start,
                    obstacle_id: Some(id),
                });
            }
        }
        Ok(MotionResult {
            position,
            obstacle_id: stop.map(|(_, id)| id),
        })
    }

    fn ready_dynamics(&self) -> Result<&BTreeMap<String, AabbUm>> {
        self.dynamics
            .as_ref()
            .ok_or(KituError::InvalidInput("dynamic spatial data is invalid"))
    }

    fn entries(&self) -> Result<impl Iterator<Item = Entry<'_>>> {
        let dynamic = self.ready_dynamics()?.iter().map(|(id, bounds)| Entry {
            id,
            collider: Collider {
                bounds: *bounds,
                role: ColliderRole::Obstacle,
            },
            dynamic: true,
        });
        let statics = self.statics.iter().map(|(id, collider)| Entry {
            id,
            collider: *collider,
            dynamic: false,
        });
        Ok(statics.chain(dynamic))
    }

    fn obstacles(&self) -> Result<impl Iterator<Item = Entry<'_>>> {
        Ok(self
            .entries()?
            .filter(|entry| entry.collider.role == ColliderRole::Obstacle))
    }
}

#[derive(Clone, Copy)]
enum Boundary {
    Closed,
    Interior,
}

#[derive(Clone, Copy)]
struct Time {
    numerator: i128,
    denominator: i128,
}

impl Time {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    fn compare(self, other: Self) -> Ordering {
        // Bound differences are at most 2^64 - 1, and an absolute i64 delta
        // is at most 2^63. Their product always fits a signed i128.
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }

    fn fraction(self) -> SegmentFraction {
        let mut a = self.numerator;
        let mut b = self.denominator;
        while b != 0 {
            (a, b) = (b, a % b);
        }
        SegmentFraction {
            numerator: (self.numerator / a) as u64,
            denominator: (self.denominator / a) as u64,
        }
    }

    fn scale(self, delta: DisplacementUm) -> DisplacementUm {
        DisplacementUm(
            delta
                .0
                .map(|axis| (i128::from(axis) * self.numerator / self.denominator) as i64),
        )
    }
}

fn intersection(
    bounds: AabbUm,
    start: PositionUm,
    delta: DisplacementUm,
    boundary: Boundary,
) -> Option<Time> {
    let mut enter = Time::ZERO;
    let mut exit = Time::ONE;
    for axis in 0..3 {
        let start = i128::from(start.0[axis]);
        let min = i128::from(bounds.min.0[axis]);
        let max = i128::from(bounds.max.0[axis]);
        let displacement = i128::from(delta.0[axis]);
        if displacement == 0 {
            let outside = match boundary {
                Boundary::Closed => start < min || start > max,
                Boundary::Interior => start <= min || start >= max,
            };
            if outside {
                return None;
            }
            continue;
        }
        let (near, far) = if displacement > 0 {
            (min - start, max - start)
        } else {
            (start - max, start - min)
        };
        let denominator = displacement.abs();
        let near = Time {
            numerator: near,
            denominator,
        };
        let far = Time {
            numerator: far,
            denominator,
        };
        if near.compare(enter).is_gt() {
            enter = near;
        }
        if far.compare(exit).is_lt() {
            exit = far;
        }
    }
    let order = enter.compare(exit);
    let intersects = match boundary {
        Boundary::Closed => order.is_le(),
        Boundary::Interior => order.is_lt(),
    };
    intersects.then_some(enter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min: [i64; 3], max: [i64; 3]) -> AabbUm {
        AabbUm::new(PositionUm(min), PositionUm(max)).unwrap()
    }

    #[test]
    fn moves_on_the_requested_diagonal_until_the_first_contact() {
        let mut world = SpatialWorld::new();
        world
            .insert_static(
                "wall",
                bounds([0, -100, -100], [10, 100, 100]),
                ColliderRole::Obstacle,
            )
            .unwrap();
        let start = PositionUm([-12, 0, 0]);
        let motion = world
            .move_aabb(start, [2, 1, 1], DisplacementUm([20, 7, -7]))
            .unwrap();
        assert_eq!(motion.position, PositionUm([-2, 3, -3]));
        assert_eq!(motion.obstacle_id, Some("wall"));

        let slide = world
            .move_aabb(motion.position, [2, 1, 1], DisplacementUm([0, 5, 5]))
            .unwrap();
        assert_eq!(slide.position, PositionUm([-2, 8, 2]));
        assert_eq!(slide.obstacle_id, None);
        let away = world
            .move_aabb(slide.position, [2, 1, 1], DisplacementUm([-3, 0, 0]))
            .unwrap();
        assert_eq!(away.position, PositionUm([-5, 8, 2]));
        assert!(world
            .move_aabb(PositionUm([1, 0, 0]), [0; 3], DisplacementUm([1, 0, 0]))
            .is_err());
    }

    #[test]
    fn a_tangent_corner_is_a_hit_but_does_not_stop_movement() {
        let mut world = SpatialWorld::new();
        world
            .insert_static("box", bounds([1, 1, -1], [3, 3, 1]), ColliderRole::Obstacle)
            .unwrap();
        let start = PositionUm([0, 2, 0]);
        let delta = DisplacementUm([2, -2, 0]);
        let hit = world.hit_segment(start, delta, None).unwrap().unwrap();
        assert_eq!(hit.fraction.ratio(), (1, 2));
        assert_eq!(
            world.move_aabb(start, [0; 3], delta).unwrap().position,
            PositionUm([2, 0, 0])
        );
    }

    #[test]
    fn hits_include_endpoints_and_break_ties_by_role_then_id() {
        let mut world = SpatialWorld::new();
        let box_bounds = bounds([0, -1, -1], [1, 1, 1]);
        for (id, role) in [
            ("a_target", ColliderRole::Target),
            ("z_wall", ColliderRole::Obstacle),
            ("b_wall", ColliderRole::Obstacle),
        ] {
            world.insert_static(id, box_bounds, role).unwrap();
        }
        let start = PositionUm([-1, 1, 1]);
        assert!(world
            .hit_segment(start, DisplacementUm([0; 3]), None)
            .unwrap()
            .is_none());
        let hit = world
            .hit_segment(start, DisplacementUm([1, 0, 0]), None)
            .unwrap()
            .unwrap();
        assert_eq!((hit.id, hit.fraction.ratio()), ("b_wall", (1, 1)));
        let point = world
            .hit_segment(
                PositionUm([0, 1, 1]),
                DisplacementUm([0; 3]),
                Some("b_wall"),
            )
            .unwrap()
            .unwrap();
        assert_eq!((point.id, point.fraction.ratio()), ("z_wall", (0, 1)));
    }

    #[test]
    fn validates_extremes_and_uses_exact_fractions_at_i64_min() {
        let mut world = SpatialWorld::new();
        let low = i64::MIN;
        let high = i64::MAX;
        world
            .insert_static(
                "limit",
                bounds([low, -1, -1], [low + 1, 2, 1]),
                ColliderRole::Obstacle,
            )
            .unwrap();
        world
            .insert_static(
                "far",
                bounds([high - 1, -1, -1], [high, 2, 1]),
                ColliderRole::Obstacle,
            )
            .unwrap();

        let start = PositionUm([0; 3]);
        let delta = DisplacementUm([low, 1, 0]);
        let hit = world.hit_segment(start, delta, None).unwrap().unwrap();
        assert_eq!(hit.fraction.ratio(), (high as u64, 1 << 63));
        let motion = world.move_aabb(start, [0; 3], delta).unwrap();
        assert_eq!(motion.position, PositionUm([low + 1, 0, 0]));

        let overflow = DisplacementUm([2, 0, 0]);
        let start = PositionUm([high - 1, 0, 0]);
        assert!(world.hit_segment(start, overflow, None).is_err());
        assert!(world.move_aabb(start, [0; 3], overflow).is_err());
        assert!(world
            .move_aabb(PositionUm([0; 3]), [1, 0, 0], delta)
            .is_err());
        assert!(PositionMm([high, 0, 0]).to_micrometres().is_err());
        assert!(AabbUm::new(start, start).is_err());
    }

    #[test]
    fn a_dynamic_overlap_allows_movement_toward_a_nearest_face() {
        let mut world = SpatialWorld::new();
        let obstacle = bounds([0, -10, -10], [10, 10, 10]);
        world
            .replace_dynamic_obstacles([("box".into(), obstacle)])
            .unwrap();
        let start = PositionUm([2, 0, 0]);

        for delta in [[1, 0, 0], [0, 1, 0]] {
            let blocked = world
                .move_aabb(start, [0; 3], DisplacementUm(delta))
                .unwrap();
            assert_eq!(blocked.position, start);
            assert_eq!(blocked.obstacle_id, Some("box"));
        }
        let escape = world
            .move_aabb(start, [0; 3], DisplacementUm([-1, 1, 0]))
            .unwrap();
        assert_eq!(escape.position, PositionUm([1, 1, 0]));
        let clear = world
            .move_aabb(escape.position, [0; 3], DisplacementUm([-3, 0, 0]))
            .unwrap();
        assert_eq!(clear.position, PositionUm([-2, 1, 0]));
    }

    #[test]
    fn replacement_validates_the_whole_set_and_faults_stop_queries() {
        let mut world = SpatialWorld::new();
        let obstacle = bounds([0; 3], [10; 3]);
        world
            .insert_static("static", obstacle, ColliderRole::Obstacle)
            .unwrap();
        world
            .replace_dynamic_obstacles([("old".into(), obstacle)])
            .unwrap();
        assert!(world
            .replace_dynamic_obstacles([("new".into(), obstacle), ("static".into(), obstacle)])
            .is_err());
        assert_eq!(
            world.dynamic_obstacles().unwrap().collect::<Vec<_>>(),
            [("old", obstacle)]
        );
        world.invalidate_dynamic_obstacles();
        assert!(world.replace_dynamic_obstacles([]).is_err());
        assert!(world
            .hit_segment(PositionUm([-1; 3]), DisplacementUm([0; 3]), None)
            .is_err());
        assert!(world
            .move_aabb(PositionUm([-1; 3]), [0; 3], DisplacementUm([0; 3]))
            .is_err());
    }
}
