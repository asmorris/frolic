//! A snapshot of all field sources; the total field is their superposition.

use crate::source::FieldSource;
use glam::DVec3;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct FieldScene {
    sources: Vec<Arc<dyn FieldSource>>,
}

impl FieldScene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, source: impl FieldSource + 'static) -> &mut Self {
        self.sources.push(Arc::new(source));
        self
    }

    pub fn add_shared(&mut self, source: Arc<dyn FieldSource>) -> &mut Self {
        self.sources.push(source);
        self
    }

    pub fn sources(&self) -> &[Arc<dyn FieldSource>] {
        &self.sources
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Total B (tesla) at `p`.
    pub fn field(&self, p: DVec3) -> DVec3 {
        self.sources.iter().map(|s| s.field(p)).sum()
    }

    /// Distance to the nearest solid body (negative if inside one); `+∞` if none.
    pub fn solid_distance(&self, p: DVec3) -> f64 {
        self.sources
            .iter()
            .filter(|s| s.is_solid())
            .map(|s| s.signed_distance(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Bounding sphere of all sources.
    pub fn bounding_sphere(&self) -> Option<(DVec3, f64)> {
        if self.sources.is_empty() {
            return None;
        }
        let spheres: Vec<_> = self.sources.iter().map(|s| s.bounding_sphere()).collect();
        let centre = spheres.iter().map(|(c, _)| *c).sum::<DVec3>() / spheres.len() as f64;
        let radius = spheres.iter().map(|(c, r)| (*c - centre).length() + r).fold(0.0, f64::max);
        Some((centre, radius))
    }

    /// Typical body size (median bounding radius); sets step sizes and tolerances.
    pub fn characteristic_length(&self) -> f64 {
        let mut radii: Vec<f64> = self.sources.iter().map(|s| s.bounding_sphere().1).collect();
        if radii.is_empty() {
            return 1.0;
        }
        radii.sort_by(f64::total_cmp);
        radii[radii.len() / 2]
    }
}
