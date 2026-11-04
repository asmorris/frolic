//! Render meshes for magnets, with per-vertex pole colouring (north red, south blue)
//! that follows the actual magnetization direction.

use crate::magnets::MagnetBody;
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use frolic_core::Shape;
use std::f32::consts::{PI, TAU};

const NORTH: Color = Color::srgb(0.86, 0.16, 0.13);
const SOUTH: Color = Color::srgb(0.14, 0.34, 0.86);
const NEUTRAL: Color = Color::srgb(0.62, 0.63, 0.66);
/// Grid density for colour resolution (subdivisions per length unit).
const DENSITY: f32 = 14.0;

#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Builder {
    /// Add a parametric grid patch; `f(u, v)` returns (position, normal) for u, v ∈ [0, 1].
    /// Triangles are wound so that ∂u × ∂v faces outward (or inward when `flip`).
    fn grid(&mut self, nu: usize, nv: usize, flip: bool, f: impl Fn(f32, f32) -> (Vec3, Vec3)) {
        let base = self.positions.len() as u32;
        for j in 0..=nv {
            for i in 0..=nu {
                let (p, n) = f(i as f32 / nu as f32, j as f32 / nv as f32);
                self.positions.push(p.to_array());
                self.normals.push(n.to_array());
            }
        }
        let row = nu as u32 + 1;
        for j in 0..nv as u32 {
            for i in 0..nu as u32 {
                let a = base + j * row + i;
                let (b, c, d) = (a + 1, a + row + 1, a + row);
                if flip {
                    self.indices.extend_from_slice(&[a, c, b, a, d, c]);
                } else {
                    self.indices.extend_from_slice(&[a, b, c, a, c, d]);
                }
            }
        }
    }
}

fn subdivisions(len: f32) -> usize {
    ((len * DENSITY).ceil() as usize).clamp(2, 160)
}

pub fn magnet_mesh(body: &MagnetBody) -> Mesh {
    let mut b = Builder::default();
    match body.shape {
        Shape::Cuboid { size } => {
            let size = size.as_vec3();
            let half = 0.5 * size;
            for axis in 0..3 {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                for sign in [1.0f32, -1.0] {
                    let mut normal = Vec3::ZERO;
                    normal[axis] = sign;
                    b.grid(subdivisions(size[u]), subdivisions(size[v]), sign < 0.0, |s, t| {
                        let mut p = Vec3::ZERO;
                        p[axis] = sign * half[axis];
                        p[u] = (s - 0.5) * size[u];
                        p[v] = (t - 0.5) * size[v];
                        (p, normal)
                    });
                }
            }
        }
        Shape::Cylinder { radius, height } => {
            let (r, h) = (radius as f32, height as f32);
            let segs = subdivisions(TAU * r).max(48);
            b.grid(segs, subdivisions(h), false, |s, t| {
                let (sn, cs) = (s * TAU).sin_cos();
                let n = Vec3::new(cs, sn, 0.0);
                (r * n + Vec3::Z * (t - 0.5) * h, n)
            });
            for sign in [1.0f32, -1.0] {
                b.grid(subdivisions(r), segs, sign < 0.0, |s, t| {
                    let (sn, cs) = (t * TAU).sin_cos();
                    (Vec3::new(s * r * cs, s * r * sn, sign * 0.5 * h), Vec3::Z * sign)
                });
            }
        }
        Shape::Sphere { radius } => {
            let r = radius as f32;
            let segs = subdivisions(TAU * r).max(48);
            b.grid(segs, (segs / 2).max(24), false, |s, t| {
                let theta = PI * (1.0 - t);
                let (st, ct) = theta.sin_cos();
                let (sp, cp) = (s * TAU).sin_cos();
                let n = Vec3::new(st * cp, st * sp, ct);
                (r * n, n)
            });
        }
    }

    let colors = pole_colors(body, &b.positions);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, b.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(b.indices))
}

/// Colour each vertex by which side of the neutral plane (⊥ magnetization) it is on.
fn pole_colors(body: &MagnetBody, positions: &[[f32; 3]]) -> Vec<[f32; 4]> {
    let dir = body.direction.normalize_or_zero();
    let lin = |c: Color| c.to_linear();
    if dir == DVec3::ZERO || body.remanence == 0.0 {
        return vec![lin(NEUTRAL).to_f32_array(); positions.len()];
    }
    let extent = support(&body.shape, dir).max(1e-6) as f32;
    let dir = dir.as_vec3();
    let (north, south, neutral) = (lin(NORTH), lin(SOUTH), lin(NEUTRAL));
    positions
        .iter()
        .map(|p| {
            let t = Vec3::from(*p).dot(dir) / extent; // -1 (south face) .. 1 (north face)
            let band = 0.06;
            let c = if t > band {
                north
            } else if t < -band {
                south
            } else {
                // Thin neutral band at the magnetic "equator".
                let k = t / band;
                if k >= 0.0 { neutral.mix(&north, k * k) } else { neutral.mix(&south, k * k) }
            };
            c.to_f32_array()
        })
        .collect()
}

/// Support function: max over the shape of p·dir.
fn support(shape: &Shape, dir: DVec3) -> f64 {
    match *shape {
        Shape::Cuboid { size } => (0.5 * size * dir.abs()).element_sum(),
        Shape::Cylinder { radius, height } => radius * dir.truncate().length() + 0.5 * height * dir.z.abs(),
        Shape::Sphere { radius } => radius,
    }
}
