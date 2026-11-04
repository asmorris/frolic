//! Primitive solid shapes: geometry queries (SDF, ray casts, surface sampling) and
//! their uniformly-magnetized B-field.
//!
//! All shapes are centred on the local origin. Cylinders have their axis along local +Z.

use crate::analytic;
use glam::DVec3;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Rectangular box with full edge lengths `size`.
    Cuboid { size: DVec3 },
    /// Solid cylinder, axis along local Z.
    Cylinder { radius: f64, height: f64 },
    Sphere { radius: f64 },
}

/// A point on a body's surface, used to seed field lines.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceSample {
    pub position: DVec3,
    /// Outward unit normal.
    pub normal: DVec3,
    /// Surface area this sample represents.
    pub area: f64,
}

impl Shape {
    pub fn name(&self) -> &'static str {
        match self {
            Shape::Cuboid { .. } => "Block",
            Shape::Cylinder { .. } => "Cylinder",
            Shape::Sphere { .. } => "Sphere",
        }
    }

    pub fn volume(&self) -> f64 {
        match *self {
            Shape::Cuboid { size } => size.x * size.y * size.z,
            Shape::Cylinder { radius, height } => PI * radius * radius * height,
            Shape::Sphere { radius } => 4.0 / 3.0 * PI * radius.powi(3),
        }
    }

    pub fn surface_area(&self) -> f64 {
        match *self {
            Shape::Cuboid { size } => 2.0 * (size.x * size.y + size.y * size.z + size.z * size.x),
            Shape::Cylinder { radius, height } => TAU * radius * (radius + height),
            Shape::Sphere { radius } => 4.0 * PI * radius * radius,
        }
    }

    /// Radius of the smallest origin-centred sphere enclosing the shape.
    pub fn bounding_radius(&self) -> f64 {
        match *self {
            Shape::Cuboid { size } => 0.5 * size.length(),
            Shape::Cylinder { radius, height } => (radius * radius + 0.25 * height * height).sqrt(),
            Shape::Sphere { radius } => radius,
        }
    }

    /// Half-extents of the local axis-aligned bounding box.
    pub fn half_extents(&self) -> DVec3 {
        match *self {
            Shape::Cuboid { size } => 0.5 * size,
            Shape::Cylinder { radius, height } => DVec3::new(radius, radius, 0.5 * height),
            Shape::Sphere { radius } => DVec3::splat(radius),
        }
    }

    /// Exact signed distance (negative inside).
    pub fn signed_distance(&self, p: DVec3) -> f64 {
        match *self {
            Shape::Cuboid { size } => {
                let q = p.abs() - 0.5 * size;
                q.max(DVec3::ZERO).length() + q.max_element().min(0.0)
            }
            Shape::Cylinder { radius, height } => {
                let d = glam::DVec2::new(p.truncate().length() - radius, p.z.abs() - 0.5 * height);
                d.max(glam::DVec2::ZERO).length() + d.max_element().min(0.0)
            }
            Shape::Sphere { radius } => p.length() - radius,
        }
    }

    pub fn contains(&self, p: DVec3) -> bool {
        self.signed_distance(p) < 0.0
    }

    /// Distance along the ray to the first surface hit, if any (`dir` need not be unit).
    pub fn ray_intersect(&self, origin: DVec3, dir: DVec3) -> Option<f64> {
        match *self {
            Shape::Cuboid { size } => ray_box(origin, dir, 0.5 * size),
            Shape::Sphere { radius } => {
                let a = dir.length_squared();
                let b = origin.dot(dir);
                let c = origin.length_squared() - radius * radius;
                let disc = b * b - a * c;
                if disc < 0.0 {
                    return None;
                }
                let sq = disc.sqrt();
                [(-b - sq) / a, (-b + sq) / a].into_iter().find(|&t| t >= 0.0)
            }
            Shape::Cylinder { radius, height } => {
                let hh = 0.5 * height;
                let mut best: Option<f64> = None;
                let mut consider = |t: f64| {
                    if t >= 0.0 && best.is_none_or(|b| t < b) {
                        best = Some(t);
                    }
                };
                // Curved side.
                let a = dir.x * dir.x + dir.y * dir.y;
                if a > 1e-18 {
                    let b = origin.x * dir.x + origin.y * dir.y;
                    let c = origin.x * origin.x + origin.y * origin.y - radius * radius;
                    let disc = b * b - a * c;
                    if disc >= 0.0 {
                        let sq = disc.sqrt();
                        for t in [(-b - sq) / a, (-b + sq) / a] {
                            if (origin.z + t * dir.z).abs() <= hh {
                                consider(t);
                            }
                        }
                    }
                }
                // Caps.
                if dir.z.abs() > 1e-18 {
                    for zc in [-hh, hh] {
                        let t = (zc - origin.z) / dir.z;
                        let q = origin + t * dir;
                        if q.x * q.x + q.y * q.y <= radius * radius {
                            consider(t);
                        }
                    }
                }
                best
            }
        }
    }

    /// Roughly `count` well-distributed surface samples (local frame). Points within each
    /// face follow a low-discrepancy sequence, so any every-k-th subset stays spread out.
    pub fn surface_samples(&self, count: usize) -> Vec<SurfaceSample> {
        let count = count.max(16);
        let spacing2 = self.surface_area() / count as f64;
        let mut out = Vec::with_capacity(count + 16);
        match *self {
            Shape::Cuboid { size } => {
                let half = 0.5 * size;
                for axis in 0..3 {
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    let area = size[u] * size[v];
                    let n = ((area / spacing2).round() as usize).max(4);
                    for sign in [1.0, -1.0] {
                        let mut normal = DVec3::ZERO;
                        normal[axis] = sign;
                        for i in 0..n {
                            let (s, t) = r2(i);
                            let mut pos = DVec3::ZERO;
                            pos[axis] = sign * half[axis];
                            pos[u] = (s - 0.5) * size[u];
                            pos[v] = (t - 0.5) * size[v];
                            out.push(SurfaceSample { position: pos, normal, area: area / n as f64 });
                        }
                    }
                }
            }
            Shape::Cylinder { radius, height } => {
                let cap_area = PI * radius * radius;
                let n_cap = ((cap_area / spacing2).round() as usize).max(4);
                for sign in [1.0, -1.0] {
                    for i in 0..n_cap {
                        let r = radius * ((i as f64 + 0.5) / n_cap as f64).sqrt();
                        let th = i as f64 * GOLDEN_ANGLE;
                        out.push(SurfaceSample {
                            position: DVec3::new(r * th.cos(), r * th.sin(), sign * 0.5 * height),
                            normal: DVec3::new(0.0, 0.0, sign),
                            area: cap_area / n_cap as f64,
                        });
                    }
                }
                let side_area = TAU * radius * height;
                let n_side = ((side_area / spacing2).round() as usize).max(8);
                for i in 0..n_side {
                    let (s, t) = r2(i);
                    let th = s * TAU;
                    let normal = DVec3::new(th.cos(), th.sin(), 0.0);
                    out.push(SurfaceSample {
                        position: radius * normal + DVec3::Z * (t - 0.5) * height,
                        normal,
                        area: side_area / n_side as f64,
                    });
                }
            }
            Shape::Sphere { radius } => {
                let n = count;
                for i in 0..n {
                    let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
                    let r = (1.0 - z * z).sqrt();
                    let th = i as f64 * GOLDEN_ANGLE;
                    let normal = DVec3::new(r * th.cos(), r * th.sin(), z);
                    out.push(SurfaceSample {
                        position: radius * normal,
                        normal,
                        area: self.surface_area() / n as f64,
                    });
                }
            }
        }
        out
    }

    /// B-field (tesla, local frame) of this shape uniformly magnetized with polarization `j`.
    pub fn b_field(&self, p: DVec3, j: DVec3) -> DVec3 {
        match *self {
            Shape::Cuboid { size } => analytic::cuboid_field(p, 0.5 * size, j),
            Shape::Cylinder { radius, height } => analytic::cylinder_field(p, radius, 0.5 * height, j),
            Shape::Sphere { radius } => analytic::sphere_field(p, radius, j),
        }
    }
}

const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653; // π(3 − √5)

/// Roberts' R2 low-discrepancy sequence on the unit square.
fn r2(i: usize) -> (f64, f64) {
    const G: f64 = 1.324_717_957_244_746; // plastic number
    let a1 = 1.0 / G;
    let a2 = 1.0 / (G * G);
    ((0.5 + a1 * i as f64).fract(), (0.5 + a2 * i as f64).fract())
}

fn ray_box(o: DVec3, d: DVec3, half: DVec3) -> Option<f64> {
    let inv = d.recip();
    let t0 = (-half - o) * inv;
    let t1 = (half - o) * inv;
    let tmin = t0.min(t1).max_element();
    let tmax = t0.max(t1).min_element();
    if tmax < tmin.max(0.0) {
        None
    } else {
        Some(tmin.max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_samples_cover_the_area() {
        for shape in [
            Shape::Cuboid { size: DVec3::new(2.0, 1.0, 0.5) },
            Shape::Cylinder { radius: 0.7, height: 1.3 },
            Shape::Sphere { radius: 1.1 },
        ] {
            let s = shape.surface_samples(3000);
            let area: f64 = s.iter().map(|s| s.area).sum();
            assert!((area / shape.surface_area() - 1.0).abs() < 1e-9, "{shape:?}");
            for x in &s {
                assert!(shape.signed_distance(x.position).abs() < 1e-9, "{shape:?} {x:?}");
            }
        }
    }

    #[test]
    fn ray_hits() {
        let o = DVec3::new(-5.0, 0.1, 0.05);
        let d = DVec3::X;
        let b = Shape::Cuboid { size: DVec3::splat(1.0) };
        assert!((b.ray_intersect(o, d).unwrap() - 4.5).abs() < 1e-12);
        let c = Shape::Cylinder { radius: 1.0, height: 2.0 };
        let t = c.ray_intersect(o, d).unwrap();
        assert!(c.signed_distance(o + t * d).abs() < 1e-9);
        let top = c.ray_intersect(DVec3::new(0.2, 0.0, 5.0), -DVec3::Z).unwrap();
        assert!((top - 4.0).abs() < 1e-12);
        assert!(Shape::Sphere { radius: 1.0 }.ray_intersect(o, DVec3::Y).is_none());
    }
}
