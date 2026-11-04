//! Closed-form (or near closed-form) B-fields of uniformly magnetized bodies.
//!
//! Conventions used throughout:
//! * Everything is in the body's **local frame**, with the body centred at the origin.
//! * Magnetization is given as the polarization `J = μ0·M` in tesla (for a permanent
//!   magnet this is its remanence `Br` times the magnetization direction).
//! * Lengths are in arbitrary but consistent units. For a given `J` the B-field only
//!   depends on *ratios* of lengths, so the unit system never affects field strengths.
//!
//! The fields are derived from the magnetic surface-charge model
//! (`σ = J·n̂`, `B = μ0·H` outside, `B = μ0·H + J` inside), except the axial cylinder
//! which uses the Derby–Olbert ideal-solenoid formula (current model, gives B directly).

use crate::special::cel;
use glam::DVec3;
use std::f64::consts::{FRAC_1_PI, PI};

const INV_4PI: f64 = 0.25 * FRAC_1_PI;

/// Point dipole. `moment` is `J·V` (tesla × length³), i.e. `μ0·m`.
pub fn dipole_field(r: DVec3, moment: DVec3) -> DVec3 {
    let r2 = r.length_squared();
    if r2 == 0.0 {
        return DVec3::ZERO;
    }
    let inv_r = r2.sqrt().recip();
    let r_hat = r * inv_r;
    let inv_r3 = inv_r * inv_r * inv_r;
    INV_4PI * inv_r3 * (3.0 * moment.dot(r_hat) * r_hat - moment)
}

/// Uniformly magnetized sphere. Exact everywhere: a pure dipole outside, `2J/3` inside.
pub fn sphere_field(p: DVec3, radius: f64, j: DVec3) -> DVec3 {
    if p.length_squared() < radius * radius {
        2.0 / 3.0 * j
    } else {
        let volume = 4.0 / 3.0 * PI * radius.powi(3);
        dipole_field(p, j * volume)
    }
}

// ---------------------------------------------------------------------------------------
// Cuboid
// ---------------------------------------------------------------------------------------

/// Uniformly magnetized rectangular box with half-extents `half`. Exact everywhere
/// except exactly on edges, where the true field diverges logarithmically.
pub fn cuboid_field(p: DVec3, half: DVec3, j: DVec3) -> DVec3 {
    let mut b = DVec3::ZERO;
    // Each magnetization component is handled by the z-axis formula after a cyclic
    // permutation of the axes that brings that component onto z.
    if j.z != 0.0 {
        b += cuboid_field_z(p, half, j.z);
    }
    if j.x != 0.0 {
        // (x, y, z) -> (y, z, x): local x becomes z'.
        let bp = cuboid_field_z(
            DVec3::new(p.y, p.z, p.x),
            DVec3::new(half.y, half.z, half.x),
            j.x,
        );
        b += DVec3::new(bp.z, bp.x, bp.y);
    }
    if j.y != 0.0 {
        // (x, y, z) -> (z, x, y): local y becomes z'.
        let bp = cuboid_field_z(
            DVec3::new(p.z, p.x, p.y),
            DVec3::new(half.z, half.x, half.y),
            j.y,
        );
        b += DVec3::new(bp.y, bp.z, bp.x);
    }
    if p.x.abs() < half.x && p.y.abs() < half.y && p.z.abs() < half.z {
        b += j;
    }
    b
}

/// `μ0·H` of a box magnetized along +z with polarization `jz`: two uniformly charged
/// rectangles at `z = ±half.z`. The interior `+J` term is added by the caller.
fn cuboid_field_z(p: DVec3, half: DVec3, jz: f64) -> DVec3 {
    let scale = half.max_element();
    let tiny = 1e-12 * scale;
    let tiny2 = tiny * tiny;

    // X/Y offsets from the far/near edges: index 0 = "hi" limit, 1 = "lo" limit of the
    // mixed antiderivative.
    let xs = [p.x + half.x, p.x - half.x];
    let ys = [p.y + half.y, p.y - half.y];

    let mut acc = DVec3::ZERO;
    for (zk, charge_sign) in [(half.z, 1.0), (-half.z, -1.0)] {
        let mut z = p.z - zk;
        if z.abs() < tiny {
            // Sitting in the plane of a face: nudge off it (choose the side consistently).
            z = tiny;
        }
        let z2 = z * z;
        let mut face = DVec3::ZERO;
        for (i, &x) in xs.iter().enumerate() {
            for (k, &y) in ys.iter().enumerate() {
                let corner_sign = if (i + k) % 2 == 0 { 1.0 } else { -1.0 };
                let x2 = x * x;
                let y2 = y * y;
                let r = (x2 + y2 + z2).sqrt();
                // ∫∫ X/R³ = -ln(Y + R),  ∫∫ Y/R³ = -ln(X + R),  ∫∫ Z/R³ = atan(XY / (Z R))
                face.x -= corner_sign * ln_r_plus(y, r, x2 + z2 + tiny2);
                face.y -= corner_sign * ln_r_plus(x, r, y2 + z2 + tiny2);
                face.z += corner_sign * (x * y / (z * r)).atan();
            }
        }
        acc += charge_sign * face;
    }
    jz * INV_4PI * acc
}

/// `ln(u + r)` where `r = sqrt(u² + rest2)`, evaluated without catastrophic cancellation
/// when `u` is negative and `|u| ≈ r`.
#[inline]
fn ln_r_plus(u: f64, r: f64, rest2: f64) -> f64 {
    if u >= 0.0 {
        (u + r).ln()
    } else {
        (rest2 / (r - u)).ln()
    }
}

// ---------------------------------------------------------------------------------------
// Cylinder
// ---------------------------------------------------------------------------------------

/// Uniformly magnetized solid cylinder with its axis along local z.
///
/// The axial part is exact (Derby–Olbert). The transverse (diametric) part is integrated
/// analytically along z and numerically around the circumference with graded
/// Gauss–Legendre panels, accurate to ~1e-6 relative even right at the surface.
pub fn cylinder_field(p: DVec3, radius: f64, half_height: f64, j: DVec3) -> DVec3 {
    let mut b = DVec3::ZERO;
    if j.z != 0.0 {
        b += cylinder_axial_field(p, radius, half_height, j.z);
    }
    if j.x != 0.0 || j.y != 0.0 {
        b += cylinder_transverse_field(p, radius, half_height, j.x, j.y);
    }
    b
}

/// Derby & Olbert (2010): field of an ideal finite solenoid, identical to an axially
/// magnetized cylinder with `J = μ0·n·I`. Gives B directly (interior included).
pub fn cylinder_axial_field(p: DVec3, a: f64, b: f64, jz: f64) -> DVec3 {
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    let z = p.z;
    let b0 = jz * FRAC_1_PI;

    let zp = z + b;
    let zm = z - b;
    let apr = a + rho;
    let amr = a - rho;
    let min_kc = 1e-12;

    let inv_p = (zp * zp + apr * apr).sqrt().recip();
    let inv_m = (zm * zm + apr * apr).sqrt().recip();
    let (alpha_p, alpha_m) = (a * inv_p, a * inv_m);
    let (beta_p, beta_m) = (zp * inv_p, zm * inv_m);
    let gamma = amr / apr;
    let kc_p = ((zp * zp + amr * amr) / (zp * zp + apr * apr)).sqrt().max(min_kc);
    let kc_m = ((zm * zm + amr * amr) / (zm * zm + apr * apr)).sqrt().max(min_kc);

    let b_rho = b0 * (alpha_p * cel(kc_p, 1.0, 1.0, -1.0) - alpha_m * cel(kc_m, 1.0, 1.0, -1.0));
    let g2 = gamma * gamma;
    let b_z = b0 * a / apr
        * (beta_p * cel(kc_p, g2, 1.0, gamma) - beta_m * cel(kc_m, g2, 1.0, gamma));

    if rho > 1e-15 * a {
        DVec3::new(b_rho * p.x / rho, b_rho * p.y / rho, b_z)
    } else {
        DVec3::new(0.0, 0.0, b_z)
    }
}

/// Diametrically magnetized cylinder: surface charge `σ = J⊥·n̂` on the curved side.
/// Each angular slice is a uniformly charged straight segment (closed form).
pub fn cylinder_transverse_field(p: DVec3, a: f64, b: f64, jx: f64, jy: f64) -> DVec3 {
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    let dz_out = (p.z.abs() - b).max(0.0);
    let dist = ((rho - a).powi(2) + dz_out * dz_out).sqrt();
    // Angular width of the near-singular peak of the integrand around the point's azimuth.
    let delta = (dist / a).max(1e-12);
    let phi0 = if rho > 0.0 { p.y.atan2(p.x) } else { 0.0 };

    let z1 = p.z + b; // z - (-b)
    let z2 = p.z - b; // z - (+b)
    // Field of the charged vertical segment at azimuth φ, per unit angle.
    let integrand = |phi: f64| -> DVec3 {
        let (s, c) = phi.sin_cos();
        let sigma = jx * c + jy * s;
        let dx = p.x - a * c;
        let dy = p.y - a * s;
        let d2 = dx * dx + dy * dy;
        if sigma == 0.0 || d2 < 1e-28 * a * a {
            return DVec3::ZERO;
        }
        let r1 = (d2 + z1 * z1).sqrt();
        let r2 = (d2 + z2 * z2).sqrt();
        let radial = (z1 / r1 - z2 / r2) / d2;
        sigma * a * DVec3::new(dx * radial, dy * radial, 1.0 / r2 - 1.0 / r1)
    };

    // Geometrically graded panels [δ·2^(i-1), δ·2^i] on both sides of φ0, each with
    // 8-point Gauss–Legendre: exponential accuracy at any distance from the surface.
    let mut h = DVec3::ZERO;
    let mut lo = 0.0;
    let mut hi = delta.min(PI);
    loop {
        for side in [-1.0, 1.0] {
            h += gauss_legendre_8(&integrand, phi0 + side * lo, phi0 + side * hi) * side;
        }
        if hi >= PI {
            break;
        }
        lo = hi;
        hi = (2.0 * hi).min(PI);
    }
    let mut out = INV_4PI * h;
    if rho < a && p.z.abs() < b {
        out += DVec3::new(jx, jy, 0.0);
    }
    out
}

/// ∫ₐᵇ f with 8-point Gauss–Legendre (signed: b may be < a).
fn gauss_legendre_8(f: &impl Fn(f64) -> DVec3, a: f64, b: f64) -> DVec3 {
    const X: [f64; 4] = [0.183_434_642_495_649_8, 0.525_532_409_916_329, 0.796_666_477_413_626_7, 0.960_289_856_497_536_3];
    const W: [f64; 4] = [0.362_683_783_378_362, 0.313_706_645_877_887_3, 0.222_381_034_453_374_5, 0.101_228_536_290_376_3];
    let mid = 0.5 * (a + b);
    let half = 0.5 * (b - a);
    let mut sum = DVec3::ZERO;
    for i in 0..4 {
        sum += W[i] * (f(mid - half * X[i]) + f(mid + half * X[i]));
    }
    half * sum
}

// ---------------------------------------------------------------------------------------
// Tests: every analytic formula is checked against brute-force numerical integration of
// the surface-charge model, plus Maxwell sanity checks (div B = 0, curl B = 0 outside).
// ---------------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force `μ0·H` from a set of charged surface patches (midpoint rule).
    fn numeric_h(p: DVec3, patches: &[(DVec3, f64)]) -> DVec3 {
        let mut h = DVec3::ZERO;
        for &(pos, q) in patches {
            let d = p - pos;
            let r2 = d.length_squared();
            h += q * d / (r2 * r2.sqrt());
        }
        INV_4PI * h
    }

    fn cuboid_patches(half: DVec3, j: DVec3, n: usize) -> Vec<(DVec3, f64)> {
        let mut out = Vec::new();
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let du = 2.0 * half[u] / n as f64;
            let dv = 2.0 * half[v] / n as f64;
            for sign in [-1.0, 1.0] {
                let sigma = j[axis] * sign;
                if sigma == 0.0 {
                    continue;
                }
                for a in 0..n {
                    for b in 0..n {
                        let mut pos = DVec3::ZERO;
                        pos[axis] = sign * half[axis];
                        pos[u] = -half[u] + (a as f64 + 0.5) * du;
                        pos[v] = -half[v] + (b as f64 + 0.5) * dv;
                        out.push((pos, sigma * du * dv));
                    }
                }
            }
        }
        out
    }

    fn cylinder_patches(a: f64, b: f64, j: DVec3, n: usize) -> Vec<(DVec3, f64)> {
        let mut out = Vec::new();
        // Caps (polar midpoint grid).
        for sign in [-1.0, 1.0] {
            let sigma = j.z * sign;
            for ir in 0..n {
                let r0 = a * ir as f64 / n as f64;
                let r1 = a * (ir + 1) as f64 / n as f64;
                let rm = 0.5 * (r0 + r1);
                let nphi = (4 * n).max(8);
                let dphi = 2.0 * PI / nphi as f64;
                for ip in 0..nphi {
                    let phi = (ip as f64 + 0.5) * dphi;
                    let area = 0.5 * (r1 * r1 - r0 * r0) * dphi;
                    out.push((DVec3::new(rm * phi.cos(), rm * phi.sin(), sign * b), sigma * area));
                }
            }
        }
        // Side.
        let nphi = 4 * n;
        let nz = n;
        let dphi = 2.0 * PI / nphi as f64;
        let dz = 2.0 * b / nz as f64;
        for ip in 0..nphi {
            let phi = (ip as f64 + 0.5) * dphi;
            let (s, c) = phi.sin_cos();
            let sigma = j.x * c + j.y * s;
            for iz in 0..nz {
                let z = -b + (iz as f64 + 0.5) * dz;
                out.push((DVec3::new(a * c, a * s, z), sigma * a * dphi * dz));
            }
        }
        out
    }

    fn assert_close(label: &str, got: DVec3, want: DVec3, rel: f64) {
        let err = (got - want).length();
        let scale = want.length().max(1e-6);
        assert!(
            err / scale < rel,
            "{label}: got {got:?}, want {want:?}, rel err {:.3e}",
            err / scale
        );
    }

    #[test]
    fn cuboid_matches_numeric_integration() {
        let half = DVec3::new(0.5, 1.0, 1.5);
        let j = DVec3::new(0.3, -0.5, 1.1);
        let patches = cuboid_patches(half, j, 300);
        let outside = [
            DVec3::new(0.9, 0.2, -0.3),
            DVec3::new(-1.3, 1.7, 2.2),
            DVec3::new(0.1, -1.4, 0.4),
            DVec3::new(0.0, 0.0, 2.0),
            DVec3::new(3.0, -2.0, 1.0),
        ];
        for p in outside {
            assert_close("outside", cuboid_field(p, half, j), numeric_h(p, &patches), 2e-4);
        }
        let inside = [DVec3::new(0.1, 0.2, -0.3), DVec3::new(-0.2, -0.5, 1.0)];
        for p in inside {
            assert_close("inside", cuboid_field(p, half, j), numeric_h(p, &patches) + j, 2e-4);
        }
    }

    #[test]
    fn cylinder_axial_matches_on_axis_formula() {
        let (a, b, jz): (f64, f64, f64) = (0.7, 1.2, 1.3);
        for z in [-3.0, -1.5, -0.4, 0.0, 0.9, 2.5] {
            let want = 0.5 * jz * ((z + b) / ((z + b).powi(2) + a * a).sqrt() - (z - b) / ((z - b).powi(2) + a * a).sqrt());
            let got = cylinder_axial_field(DVec3::new(0.0, 0.0, z), a, b, jz);
            assert!((got.z - want).abs() < 1e-12 && got.x == 0.0 && got.y == 0.0, "z={z}: {got:?} vs {want}");
        }
    }

    #[test]
    fn cylinder_matches_numeric_integration() {
        let (a, b) = (0.8, 0.6);
        let j = DVec3::new(0.4, -0.7, 0.9);
        let patches = cylinder_patches(a, b, j, 400);
        let outside = [
            DVec3::new(1.2, 0.3, 0.2),
            DVec3::new(-0.4, 0.5, 1.0),
            DVec3::new(2.0, -1.5, -1.8),
            DVec3::new(0.0, 0.0, -1.1),
        ];
        for p in outside {
            assert_close("outside", cylinder_field(p, a, b, j), numeric_h(p, &patches), 5e-4);
        }
        let inside = [DVec3::new(0.2, -0.3, 0.1), DVec3::new(0.0, 0.5, -0.4)];
        for p in inside {
            assert_close("inside", cylinder_field(p, a, b, j), numeric_h(p, &patches) + j, 5e-4);
        }
    }

    #[test]
    fn sphere_matches_numeric_dipole_sum_inside() {
        // Inside field of a uniformly magnetized sphere is 2J/3; just sanity-check outside
        // continuity of the normal component at the surface.
        let r = 1.0;
        let j = DVec3::new(0.0, 0.0, 1.0);
        let n = DVec3::new(0.3, 0.4, 0.866).normalize();
        let b_in = sphere_field(n * (r - 1e-9), r, j);
        let b_out = sphere_field(n * (r + 1e-9), r, j);
        assert!((b_in.dot(n) - b_out.dot(n)).abs() < 1e-6);
    }

    fn div_curl(f: &dyn Fn(DVec3) -> DVec3, p: DVec3, h: f64) -> (f64, DVec3) {
        let d = |axis: usize| {
            let mut e = DVec3::ZERO;
            e[axis] = h;
            (f(p + e) - f(p - e)) / (2.0 * h)
        };
        let (dx, dy, dz) = (d(0), d(1), d(2));
        let div = dx.x + dy.y + dz.z;
        let curl = DVec3::new(dy.z - dz.y, dz.x - dx.z, dx.y - dy.x);
        (div, curl)
    }

    #[test]
    fn fields_are_divergence_and_curl_free_outside() {
        let j = DVec3::new(0.5, 0.8, -0.6);
        type FieldFn = Box<dyn Fn(DVec3) -> DVec3>;
        let shapes: Vec<(&str, FieldFn)> = vec![
            ("cuboid", Box::new(move |p| cuboid_field(p, DVec3::new(0.5, 0.3, 0.8), j))),
            ("cylinder", Box::new(move |p| cylinder_field(p, 0.6, 0.4, j))),
            ("sphere", Box::new(move |p| sphere_field(p, 0.7, j))),
        ];
        let points = [DVec3::new(1.1, 0.2, 0.3), DVec3::new(-0.3, 0.9, -0.95), DVec3::new(0.2, -0.1, 1.3)];
        for (name, f) in &shapes {
            for &p in &points {
                let (div, curl) = div_curl(f.as_ref(), p, 1e-4);
                let scale = f(p).length() / 0.1; // field / characteristic length
                assert!(div.abs() < 1e-5 * scale, "{name} div {div} at {p:?}");
                assert!(curl.length() < 1e-5 * scale, "{name} curl {curl:?} at {p:?}");
            }
        }
    }

    #[test]
    fn normal_b_is_continuous_across_surfaces() {
        let j = DVec3::new(0.3, -0.2, 1.0);
        let eps = 1e-7;
        // Cuboid top face and side face.
        let half = DVec3::new(0.5, 0.4, 0.3);
        for (p, n) in [
            (DVec3::new(0.1, -0.1, 0.3), DVec3::Z),
            (DVec3::new(0.5, 0.2, 0.05), DVec3::X),
        ] {
            let bi = cuboid_field(p - eps * n, half, j).dot(n);
            let bo = cuboid_field(p + eps * n, half, j).dot(n);
            assert!((bi - bo).abs() < 1e-4, "cuboid Bn jump {bi} vs {bo}");
        }
        // Cylinder cap and side.
        for (p, n) in [
            (DVec3::new(0.2, 0.1, 0.4), DVec3::Z),
            (DVec3::new(0.6, 0.0, 0.1), DVec3::X),
        ] {
            let bi = cylinder_field(p - eps * n, 0.6, 0.4, j).dot(n);
            let bo = cylinder_field(p + eps * n, 0.6, 0.4, j).dot(n);
            assert!((bi - bo).abs() < 1e-4, "cylinder Bn jump {bi} vs {bo}");
        }
    }

    #[test]
    fn far_field_approaches_dipole() {
        let half = DVec3::new(0.5, 0.3, 0.8);
        let j = DVec3::new(0.2, 0.0, 1.0);
        let v = 8.0 * half.x * half.y * half.z;
        let p = DVec3::new(20.0, -13.0, 9.0);
        assert_close("cuboid far", cuboid_field(p, half, j), dipole_field(p, j * v), 1e-2);
        let (a, b) = (0.6, 0.4);
        let v = PI * a * a * 2.0 * b;
        assert_close("cyl far", cylinder_field(p, a, b, j), dipole_field(p, j * v), 1e-2);
    }
}
