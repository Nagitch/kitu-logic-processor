//! Segment clipping against convex integer half-spaces.

use std::cmp::Ordering;

use kitu_core::{KituError, Result};

use super::{DisplacementUm, PositionUm};

const OVERFLOW: KituError = KituError::InvalidInput("integer convex query overflow");

/// A half-space of the form normal dot position <= offset.
///
/// An unnormalized integer normal is sufficient. The offset uses the same
/// scale as the dot product with a position in micrometres.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    normal: [i64; 3],
    offset: i128,
}

impl Plane {
    /// Creates a plane with a nonzero outward normal.
    pub fn new(normal: [i64; 3], offset: i128) -> Result<Self> {
        if normal == [0; 3] {
            return Err(KituError::InvalidInput(
                "convex plane normal must be nonzero",
            ));
        }
        Ok(Self { normal, offset })
    }

    /// Returns the outward normal.
    pub fn normal(self) -> [i64; 3] {
        self.normal
    }

    /// Returns the dot-product threshold for this plane.
    pub fn offset(self) -> i128 {
        self.offset
    }
}

/// Determines whether the boundary of a convex shape counts as an intersection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// Includes faces, edges, and segment endpoints.
    Closed,
    /// Requires a nonempty interval in the shape's interior.
    Interior,
}

/// Exact segment parameter in the interval [0, 1].
#[derive(Clone, Copy, Debug)]
pub struct SegmentTime {
    numerator: i128,
    denominator: i128,
}

impl SegmentTime {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Returns the numerator and positive denominator.
    pub fn ratio(self) -> (i128, i128) {
        (self.numerator, self.denominator)
    }

    /// Compares two fractions without converting them to floating point.
    pub fn compare(self, other: Self) -> Result<Ordering> {
        compare(
            self.numerator,
            self.denominator,
            other.numerator,
            other.denominator,
        )
    }

    /// Returns the supplied displacement scaled by this parameter, rounding toward zero.
    pub fn scale(self, delta: DisplacementUm) -> Result<DisplacementUm> {
        let mut scaled = delta.0;
        for axis in &mut scaled {
            let value = i128::from(*axis)
                .checked_mul(self.numerator)
                .ok_or(OVERFLOW)?
                / self.denominator;
            *axis = i64::try_from(value).map_err(|_| OVERFLOW)?;
        }
        Ok(DisplacementUm(scaled))
    }
}

/// Part of a segment contained in a convex shape.
#[derive(Clone, Copy, Debug)]
pub struct ConvexHit {
    /// Entry parameter, clamped to the segment.
    pub entry: SegmentTime,
    /// Exit parameter, clamped to the segment.
    pub exit: SegmentTime,
    /// Outward normal of the entering face, if entry is on a face.
    pub entering_normal: Option<[i64; 3]>,
    /// Whether the segment starts strictly inside every plane.
    pub starts_inside: bool,
}

/// Clips a segment against the intersection of the supplied planes.
///
/// Equal entry parameters use the first plane in input order. Checked
/// arithmetic reports an error if the coordinates or intermediates overflow.
///
/// # Examples
///
/// ```
/// use kitu_ecs::spatial::{convex::{clip, Boundary, Plane}, DisplacementUm, PositionUm};
/// let plane = Plane::new([1, 1, 0], 2).unwrap();
/// let hit = clip(&[plane], PositionUm([2, 2, 0]), DisplacementUm([-2, -2, 0]), Boundary::Closed)
///     .unwrap().unwrap();
/// assert_eq!(hit.entry.scale(DisplacementUm([-2, -2, 0])).unwrap().0, [-1, -1, 0]);
/// ```
pub fn clip(
    planes: &[Plane],
    start: PositionUm,
    delta: DisplacementUm,
    boundary: Boundary,
) -> Result<Option<ConvexHit>> {
    if planes.is_empty() {
        return Err(KituError::InvalidInput(
            "expected at least one convex plane",
        ));
    }
    start.offset(delta)?;
    let mut entry = SegmentTime::ZERO;
    let mut exit = SegmentTime::ONE;
    let mut entering_normal = None;
    let mut starts_inside = true;
    for plane in planes {
        let gap = plane
            .offset
            .checked_sub(dot(plane.normal, start.0)?)
            .ok_or(OVERFLOW)?;
        let speed = dot(plane.normal, delta.0)?;
        starts_inside &= gap > 0;
        if speed == 0 {
            if gap < 0 || (gap == 0 && boundary == Boundary::Interior) {
                return Ok(None);
            }
        } else if speed < 0 {
            let near = SegmentTime {
                numerator: gap.checked_neg().ok_or(OVERFLOW)?,
                denominator: speed.checked_neg().ok_or(OVERFLOW)?,
            };
            let order = near.compare(entry)?;
            if order.is_gt() || (order.is_eq() && entering_normal.is_none()) {
                entry = near;
                entering_normal = Some(plane.normal);
            }
        } else {
            let far = SegmentTime {
                numerator: gap,
                denominator: speed,
            };
            if far.compare(exit)?.is_lt() {
                exit = far;
            }
        }

        let order = entry.compare(exit)?;
        if order.is_gt() || (order.is_eq() && boundary == Boundary::Interior) {
            return Ok(None);
        }
    }
    Ok(Some(ConvexHit {
        entry,
        exit,
        entering_normal,
        starts_inside,
    }))
}

fn dot(normal: [i64; 3], position: [i64; 3]) -> Result<i128> {
    normal
        .into_iter()
        .zip(position)
        .try_fold(0_i128, |sum, (axis, coordinate)| {
            sum.checked_add(i128::from(axis) * i128::from(coordinate))
                .ok_or(OVERFLOW)
        })
}

fn compare(a: i128, b: i128, c: i128, d: i128) -> Result<Ordering> {
    let left = a.checked_mul(d).ok_or(OVERFLOW)?;
    let right = c.checked_mul(b).ok_or(OVERFLOW)?;
    Ok(left.cmp(&right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_a_tilted_plane_in_both_directions() {
        let plane = Plane::new([1, 1, 0], 2).unwrap();
        let entry = clip(
            &[plane],
            PositionUm([2, 2, 0]),
            DisplacementUm([-2, -2, 0]),
            Boundary::Interior,
        )
        .unwrap()
        .unwrap();
        assert_eq!(entry.entry.ratio(), (2, 4));
        assert_eq!(entry.entering_normal, Some([1, 1, 0]));
        assert!(!entry.starts_inside);
        let exit = clip(
            &[plane],
            PositionUm([0; 3]),
            DisplacementUm([2, 2, 0]),
            Boundary::Interior,
        )
        .unwrap()
        .unwrap();
        assert_eq!(exit.exit.ratio(), (2, 4));
        assert!(exit.starts_inside);
        assert_eq!(exit.entering_normal, None);
    }

    #[test]
    fn boundary_and_first_face_ties_are_explicit() {
        let planes = [
            Plane::new([0, 1, 0], 0).unwrap(),
            Plane::new([1, 0, 0], 0).unwrap(),
        ];
        let delta = DisplacementUm([-2, -2, 0]);
        let hit = clip(&planes, PositionUm([1, 1, 0]), delta, Boundary::Interior)
            .unwrap()
            .unwrap();
        assert_eq!(hit.entering_normal, Some([0, 1, 0]));
        let tangent = DisplacementUm([0, -1, 0]);
        assert!(clip(&planes, PositionUm([0; 3]), tangent, Boundary::Closed)
            .unwrap()
            .is_some());
        assert!(
            clip(&planes, PositionUm([0; 3]), tangent, Boundary::Interior)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_larger_intermediates_and_invalid_inputs() {
        let plane = Plane::new([i64::MAX; 3], 0).unwrap();
        assert!(clip(
            &[plane],
            PositionUm([i64::MAX; 3]),
            DisplacementUm([0; 3]),
            Boundary::Closed
        )
        .is_err());
        assert!(Plane::new([0; 3], 0).is_err());
        assert!(clip(
            &[],
            PositionUm([0; 3]),
            DisplacementUm([0; 3]),
            Boundary::Closed
        )
        .is_err());
    }
    #[test]
    fn misses_when_contact_is_beyond_the_endpoint() {
        let plane = Plane::new([1, 0, 0], 5).unwrap();
        assert!(clip(
            &[plane],
            PositionUm([10; 3]),
            DisplacementUm([-1; 3]),
            Boundary::Closed
        )
        .unwrap()
        .is_none());
    }
}
