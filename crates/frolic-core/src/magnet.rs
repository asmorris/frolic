//! Uniformly magnetized permanent magnets.

use crate::analytic::dipole_field;
use crate::shape::{Shape, SurfaceSample};
use crate::source::{FieldSource, Pose};
use glam::DVec3;

/// Beyond this many bounding radii the exact formula is replaced by the dipole term.
/// The relative error of that substitution is O((R/r)²) ≈ 1% here.
const FAR_FIELD_RADII: f64 = 10.0;

/// Typical remanence of an N42 neodymium magnet, in tesla.
pub const NDFEB_N42_BR: f64 = 1.3;

#[derive(Clone, Debug, PartialEq)]
pub struct Magnet {
    pub pose: Pose,
    pub shape: Shape,
    /// Polarization `J = μ0·M` (tesla) in the **local** frame. For a permanent magnet
    /// this is the remanence `Br` along the magnetization direction.
    pub polarization: DVec3,
}

impl Magnet {
    pub fn new(shape: Shape, pose: Pose, polarization: DVec3) -> Self {
        Self { pose, shape, polarization }
    }

    /// Magnetic moment `μ0·m = J·V` in world frame (tesla · length³).
    pub fn moment(&self) -> DVec3 {
        self.pose.to_world_vector(self.polarization * self.shape.volume())
    }

    /// Field in the local frame at a local point.
    pub fn local_field(&self, p_local: DVec3) -> DVec3 {
        if p_local.length_squared() > (FAR_FIELD_RADII * self.shape.bounding_radius()).powi(2) {
            dipole_field(p_local, self.polarization * self.shape.volume())
        } else {
            self.shape.b_field(p_local, self.polarization)
        }
    }
}

impl FieldSource for Magnet {
    fn field(&self, p: DVec3) -> DVec3 {
        let local = self.pose.to_local_point(p);
        self.pose.to_world_vector(self.local_field(local))
    }

    fn bounding_sphere(&self) -> (DVec3, f64) {
        (self.pose.translation, self.shape.bounding_radius())
    }

    fn signed_distance(&self, p: DVec3) -> f64 {
        self.shape.signed_distance(self.pose.to_local_point(p))
    }

    fn seed_samples(&self, count: usize) -> Vec<SurfaceSample> {
        self.shape
            .surface_samples(count)
            .into_iter()
            .map(|s| SurfaceSample {
                position: self.pose.to_world_point(s.position),
                normal: self.pose.to_world_vector(s.normal),
                area: s.area,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;

    #[test]
    fn rotating_the_body_rotates_the_field() {
        // A block magnetized along local +X, rotated so that local X points along world Z,
        // must equal the same block (dims permuted) magnetized along world Z.
        let rot = DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2); // local X -> world Z
        let a = Magnet::new(
            Shape::Cuboid { size: DVec3::new(2.0, 1.0, 0.5) },
            Pose::new(DVec3::new(0.3, -0.2, 0.1), rot),
            DVec3::new(1.2, 0.0, 0.0),
        );
        assert!((rot * DVec3::X - DVec3::Z).length() < 1e-12);
        let b = Magnet::new(
            Shape::Cuboid { size: DVec3::new(0.5, 1.0, 2.0) },
            Pose::new(DVec3::new(0.3, -0.2, 0.1), DQuat::IDENTITY),
            DVec3::new(0.0, 0.0, 1.2),
        );
        for p in [DVec3::new(1.5, 0.7, -2.0), DVec3::new(-0.4, 0.2, 1.6), DVec3::new(0.3, -0.2, 0.2)] {
            let (fa, fb) = (a.field(p), b.field(p));
            assert!((fa - fb).length() < 1e-9 * fb.length().max(1.0), "{fa:?} vs {fb:?}");
        }
    }

    #[test]
    fn far_field_switch_is_continuous() {
        let m = Magnet::new(Shape::Cylinder { radius: 0.5, height: 1.0 }, Pose::default(), DVec3::new(0.0, 0.3, 1.0));
        let r = FAR_FIELD_RADII * m.shape.bounding_radius();
        let dir = DVec3::new(0.3, 0.5, 0.8).normalize();
        let near = m.field(dir * r * (1.0 - 1e-9));
        let far = m.field(dir * r * (1.0 + 1e-9));
        assert!((near - far).length() / near.length() < 0.02);
    }
}
