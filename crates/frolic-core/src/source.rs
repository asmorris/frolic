//! The extension point of the simulator: anything that produces a magnetic field.
//!
//! Permanent magnets ([`crate::Magnet`]) implement this today. Planned implementors:
//! * composite magnets built from primitives / voxels (sum of exact sub-fields),
//! * mesh magnets (triangulated surface charge),
//! * current-carrying conductors: straight segments, loops, solenoids (Biot–Savart;
//!   a solenoid reuses the exact cylinder formula), with optional ferromagnetic cores.
//!
//! Time-dependence: a [`crate::FieldScene`] is an immutable *snapshot*. Time-varying
//! sources (AC coils, moving magnets, induced currents) are evaluated at a time `t`
//! when the snapshot is built, so the tracing and visualization code never needs to
//! know about time. Induction (dΦ/dt, eddy currents, coupled circuits) will live in a
//! circuit/time-stepping layer that produces successive snapshots.

use crate::shape::SurfaceSample;
use glam::{DQuat, DVec3};

/// Rigid placement of a body in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub translation: DVec3,
    pub rotation: DQuat,
}

impl Default for Pose {
    fn default() -> Self {
        Self { translation: DVec3::ZERO, rotation: DQuat::IDENTITY }
    }
}

impl Pose {
    pub fn new(translation: DVec3, rotation: DQuat) -> Self {
        Self { translation, rotation }
    }
    pub fn to_local_point(&self, p: DVec3) -> DVec3 {
        self.rotation.inverse() * (p - self.translation)
    }
    pub fn to_local_vector(&self, v: DVec3) -> DVec3 {
        self.rotation.inverse() * v
    }
    pub fn to_world_point(&self, p: DVec3) -> DVec3 {
        self.rotation * p + self.translation
    }
    pub fn to_world_vector(&self, v: DVec3) -> DVec3 {
        self.rotation * v
    }
}

/// Anything that contributes to the magnetic field.
pub trait FieldSource: Send + Sync + std::fmt::Debug {
    /// Magnetic flux density **B** in tesla at world-space point `p`.
    fn field(&self, p: DVec3) -> DVec3;

    /// World-space bounding sphere `(centre, radius)`.
    fn bounding_sphere(&self) -> (DVec3, f64);

    /// Signed distance to the body's solid region (negative inside). Field lines stop
    /// when they enter a solid region. Sources without a solid body (e.g. thin wires)
    /// should return a positive distance (default: distance to the bounding sphere).
    fn signed_distance(&self, p: DVec3) -> f64 {
        let (c, r) = self.bounding_sphere();
        ((p - c).length() - r).max(f64::MIN_POSITIVE)
    }

    /// Whether this source has a solid region that terminates field lines.
    fn is_solid(&self) -> bool {
        true
    }

    /// World-space candidate points from which field lines may be seeded. The tracer
    /// picks among them in proportion to outward flux `B·n̂·area`.
    fn seed_samples(&self, count: usize) -> Vec<SurfaceSample>;
}
