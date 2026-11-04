//! Field-line (streamline) tracing.
//!
//! Lines are seeded on body surfaces where flux *leaves* the body, chosen so that each
//! line carries roughly the same amount of flux. Line density in space is therefore
//! proportional to field strength, just like in textbook field-line diagrams. Each line
//! is integrated along **B** with RK4 and ends when it enters a solid body, leaves the
//! region of interest, closes on itself, or hits a field null.

use crate::scene::FieldScene;
use glam::DVec3;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TraceParams {
    /// Target total number of lines across the whole scene.
    pub line_count: usize,
    /// Candidate seed points sampled per source surface.
    pub samples_per_source: usize,
    /// Hard cap on integration steps per line.
    pub max_steps: usize,
    /// Step size as a fraction of the distance to the nearest solid surface.
    pub step_fraction: f64,
    /// Minimum / maximum step, as multiples of the scene's characteristic length.
    pub min_step: f64,
    pub max_step: f64,
    /// Lines stop beyond `bounds_scale × (scene bounding radius)` from the scene centre.
    pub bounds_scale: f64,
}

impl Default for TraceParams {
    fn default() -> Self {
        Self {
            line_count: 160,
            samples_per_source: 4000,
            max_steps: 2000,
            step_fraction: 0.2,
            min_step: 0.002,
            max_step: 0.25,
            bounds_scale: 3.0,
        }
    }
}

impl TraceParams {
    /// A cheaper variant for use while the user is dragging things around.
    pub fn interactive(&self) -> Self {
        Self {
            line_count: (self.line_count * 2 / 3).max(8),
            samples_per_source: (self.samples_per_source / 2).max(200),
            step_fraction: self.step_fraction * 1.5,
            min_step: self.min_step * 2.0,
            max_step: self.max_step * 1.5,
            ..self.clone()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FieldLine {
    pub points: Vec<DVec3>,
    /// |B| in tesla at each point.
    pub magnitudes: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct Seed {
    /// Where integration starts: slightly off the surface, so lines don't cling to edges.
    pub position: DVec3,
    /// The surface point the line visually starts from.
    pub surface: DVec3,
    pub source_index: usize,
}

/// Choose flux-weighted seed points on all source surfaces.
pub fn seed_points(scene: &FieldScene, params: &TraceParams) -> Vec<Seed> {
    let length = scene.characteristic_length();
    let offset = 1e-2 * length;

    // (position, outward flux) for every candidate, grouped per source.
    let candidates: Vec<Vec<(DVec3, DVec3, f64)>> = scene
        .sources()
        .par_iter()
        .map(|src| {
            src.seed_samples(params.samples_per_source)
                .into_par_iter()
                .filter_map(|s| {
                    let p = s.position + s.normal * offset;
                    if scene.solid_distance(p) < 0.0 {
                        return None; // touching another body
                    }
                    let flux = scene.field(p).dot(s.normal) * s.area;
                    (flux > 0.0).then_some((p, s.position, flux))
                })
                .collect()
        })
        .collect();

    let total: f64 = candidates.iter().flatten().map(|(_, _, f)| f).sum();
    if total <= 0.0 || params.line_count == 0 {
        return Vec::new();
    }
    let flux_per_line = total / params.line_count as f64;

    // Systematic (error-diffusion) sampling along the low-discrepancy candidate order.
    let mut seeds = Vec::with_capacity(params.line_count + scene.sources().len());
    for (source_index, list) in candidates.iter().enumerate() {
        let mut acc = 0.5;
        for &(position, surface, flux) in list {
            acc += flux / flux_per_line;
            while acc >= 1.0 {
                seeds.push(Seed { position, surface, source_index });
                acc -= 1.0;
            }
        }
    }
    seeds.dedup_by(|a, b| a.position == b.position);
    seeds
}

/// Seed and trace all field lines (parallel).
pub fn trace_field_lines(scene: &FieldScene, params: &TraceParams) -> Vec<FieldLine> {
    let seeds = seed_points(scene, params);
    let tracer = Tracer::new(scene, params);
    seeds
        .par_iter()
        .map(|s| {
            let mut line = tracer.trace(s.position);
            if let Some(&m) = line.magnitudes.first() {
                line.points.insert(0, s.surface);
                line.magnitudes.insert(0, m);
            }
            line
        })
        .filter(|l| l.points.len() >= 3)
        .collect()
}

/// Integrates a single field line through a scene.
pub struct Tracer<'a> {
    scene: &'a FieldScene,
    max_steps: usize,
    step_fraction: f64,
    min_step: f64,
    max_step: f64,
    centre: DVec3,
    bound_radius: f64,
}

impl<'a> Tracer<'a> {
    pub fn new(scene: &'a FieldScene, params: &TraceParams) -> Self {
        let length = scene.characteristic_length();
        let (centre, radius) = scene.bounding_sphere().unwrap_or((DVec3::ZERO, 1.0));
        Self {
            scene,
            max_steps: params.max_steps,
            step_fraction: params.step_fraction,
            min_step: params.min_step * length,
            max_step: params.max_step * length,
            centre,
            bound_radius: params.bounds_scale * radius.max(length) + 2.0 * length,
        }
    }

    pub fn trace(&self, start: DVec3) -> FieldLine {
        let mut line = FieldLine::default();
        let mut p = start;
        let mut b = self.scene.field(p);
        let mut prev_dir: Option<DVec3> = None;
        let mut travelled = 0.0;

        for _ in 0..self.max_steps {
            let mag = b.length();
            line.points.push(p);
            line.magnitudes.push(mag);
            if mag.is_nan() || mag <= 1e-15 {
                break; // field null (or NaN on a singular edge)
            }
            let k1 = b / mag;
            if prev_dir.is_some_and(|d| d.dot(k1) < -0.5) {
                break; // direction reversal: oscillating around a null point
            }
            prev_dir = Some(k1);

            let d = self.scene.solid_distance(p).max(0.0);
            let h = (self.min_step + self.step_fraction * d).min(self.max_step);
            let Some(k2) = self.dir(p + 0.5 * h * k1) else { break };
            let Some(k3) = self.dir(p + 0.5 * h * k2) else { break };
            let Some(k4) = self.dir(p + h * k3) else { break };
            let next = p + h / 6.0 * (k1 + 2.0 * k2 + 2.0 * k3 + k4);

            let d_next = self.scene.solid_distance(next);
            if d_next < 0.0 {
                // Entered a body: end exactly on its surface (linear SDF interpolation).
                let t = (d / (d - d_next)).clamp(0.0, 1.0);
                let hit = p + t * (next - p);
                line.points.push(hit);
                line.magnitudes.push(self.scene.field(hit).length());
                return line;
            }
            travelled += h;
            p = next;
            b = self.scene.field(p);

            if (p - self.centre).length_squared() > self.bound_radius * self.bound_radius {
                break;
            }
            // Closed loop (field lines around wires / through coils have no solid end).
            if travelled > 20.0 * h && (p - start).length_squared() < h * h {
                line.points.push(p);
                line.magnitudes.push(b.length());
                line.points.push(start);
                line.magnitudes.push(line.magnitudes[0]);
                return line;
            }
        }
        if line.points.last() != Some(&p) {
            line.points.push(p);
            line.magnitudes.push(b.length());
        }
        line
    }

    fn dir(&self, p: DVec3) -> Option<DVec3> {
        let b = self.scene.field(p);
        let m = b.length();
        (m > 1e-15).then(|| b / m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Magnet, Pose, Shape};

    fn bar() -> Magnet {
        Magnet::new(
            Shape::Cuboid { size: DVec3::new(4.0, 1.0, 1.0) },
            Pose::default(),
            DVec3::new(1.3, 0.0, 0.0),
        )
    }

    #[test]
    fn bar_magnet_lines_run_from_north_to_south() {
        let mut scene = FieldScene::new();
        scene.add(bar());
        let params = TraceParams { line_count: 60, ..Default::default() };
        let lines = trace_field_lines(&scene, &params);
        assert!(lines.len() >= 50, "only {} lines", lines.len());
        let mut closed = 0;
        for l in &lines {
            let first = l.points[0];
            let last = *l.points.last().unwrap();
            // Seeds leave from the +X (north) half…
            assert!(first.x > -0.1, "seed on south half: {first:?}");
            // …and lines that end on the magnet end on the -X (south) half.
            if scene.solid_distance(last) < 1e-2 {
                closed += 1;
                assert!(last.x < 0.1, "line ended on north half: {last:?}");
            }
            assert!(l.points.len() == l.magnitudes.len());
        }
        assert!(closed as f64 > 0.8 * lines.len() as f64, "{closed}/{}", lines.len());
    }

    #[test]
    fn field_lines_follow_the_field() {
        let mut scene = FieldScene::new();
        scene.add(bar());
        let lines = trace_field_lines(&scene, &TraceParams { line_count: 20, ..Default::default() });
        for l in &lines {
            for w in l.points.windows(2) {
                let mid_p = 0.5 * (w[0] + w[1]);
                // Within a hair of the edges the field turns faster than any step; check
                // accuracy in the bulk of the volume.
                if scene.solid_distance(mid_p) < 0.05 {
                    continue;
                }
                let seg = w[1] - w[0];
                let mid = scene.field(mid_p);
                assert!(seg.normalize().dot(mid.normalize()) > 0.999, "misaligned at {mid_p:?}");
            }
        }
    }

    #[test]
    fn opposing_magnets_produce_no_line_crossing_the_gap_centre() {
        // Two bars facing N-to-N: the symmetry plane x = 0 has B·x̂ = 0, so no line
        // should cross it.
        let mut scene = FieldScene::new();
        let size = DVec3::new(2.0, 1.0, 1.0);
        scene.add(Magnet::new(Shape::Cuboid { size }, Pose::new(DVec3::new(-2.0, 0.0, 0.0), Default::default()), DVec3::new(1.0, 0.0, 0.0)));
        scene.add(Magnet::new(Shape::Cuboid { size }, Pose::new(DVec3::new(2.0, 0.0, 0.0), Default::default()), DVec3::new(-1.0, 0.0, 0.0)));
        let lines = trace_field_lines(&scene, &TraceParams { line_count: 80, ..Default::default() });
        for l in &lines {
            let side = l.points[0].x.signum();
            let crossings = l.points.iter().filter(|p| p.x.signum() != side && p.x.abs() > 0.05).count();
            assert_eq!(crossings, 0);
        }
    }
}

#[cfg(test)]
mod perf {
    use super::*;
    use crate::{Magnet, Pose, Shape};

    /// `cargo test -p frolic-core --release perf -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_three_magnets() {
        let mut scene = FieldScene::new();
        scene.add(Magnet::new(Shape::Cuboid { size: DVec3::new(4.0, 1.0, 1.0) }, Pose::new(DVec3::new(-3.0, 0.0, 0.0), Default::default()), DVec3::new(1.3, 0.0, 0.0)));
        scene.add(Magnet::new(Shape::Cylinder { radius: 0.8, height: 1.5 }, Pose::new(DVec3::new(2.5, 0.5, 1.0), Default::default()), DVec3::new(0.0, 0.6, 1.1)));
        scene.add(Magnet::new(Shape::Sphere { radius: 0.7 }, Pose::new(DVec3::new(0.5, -1.0, -2.0), Default::default()), DVec3::new(0.0, 1.2, 0.0)));
        for params in [TraceParams::default(), TraceParams::default().interactive()] {
            let t = std::time::Instant::now();
            let lines = trace_field_lines(&scene, &params);
            let pts: usize = lines.iter().map(|l| l.points.len()).sum();
            println!("{} lines, {} points in {:?}", lines.len(), pts, t.elapsed());
        }
    }
}
