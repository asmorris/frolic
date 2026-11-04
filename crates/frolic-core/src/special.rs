//! Special functions needed by the analytic field formulas.

use std::f64::consts::FRAC_PI_2;

/// Bulirsch's generalized complete elliptic integral
///
/// ```text
/// cel(kc, p, c, s) = ∫₀^{π/2} (c cos²φ + s sin²φ) / ((cos²φ + p sin²φ) √(cos²φ + kc² sin²φ)) dφ
/// ```
///
/// This single function covers K, E and Π and is what makes the Derby–Olbert
/// solenoid / cylinder-magnet formula numerically stable everywhere.
///
/// Port of the algorithm given in Derby & Olbert, "Cylindrical magnets and ideal
/// solenoids", Am. J. Phys. 78, 229 (2010).
pub fn cel(kc: f64, p: f64, c: f64, s: f64) -> f64 {
    if kc == 0.0 {
        return f64::NAN;
    }
    const ERRTOL: f64 = 1e-8; // convergence is quadratic, so this yields ~1e-16
    let mut k = kc.abs();
    let mut pp = p;
    let mut cc = c;
    let mut ss = s;
    let mut em = 1.0;

    if p > 0.0 {
        pp = p.sqrt();
        ss = s / pp;
    } else {
        let mut f = kc * kc;
        let mut q = 1.0 - f;
        let g = 1.0 - pp;
        f -= pp;
        q *= ss - c * pp;
        pp = (f / g).sqrt();
        cc = (c - ss) / g;
        ss = -q / (g * g * pp) + cc * pp;
    }

    let mut f = cc;
    cc += ss / pp;
    let mut g = k / pp;
    ss = 2.0 * (ss + f * g);
    pp += g;
    g = em;
    em += k;
    let mut kk = k;

    let mut iter = 0;
    while (g - k).abs() > g * ERRTOL && iter < 64 {
        k = 2.0 * kk.sqrt();
        kk = k * em;
        f = cc;
        cc += ss / pp;
        g = kk / pp;
        ss = 2.0 * (ss + f * g);
        pp += g;
        g = em;
        em += k;
        iter += 1;
    }
    FRAC_PI_2 * (ss + cc * em) / (em * (em + pp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cel_reduces_to_complete_elliptic_integrals() {
        // K(m = 1/2) and E(m = 1/2), with kc = sqrt(1 - m).
        let kc = (0.5f64).sqrt();
        let k = cel(kc, 1.0, 1.0, 1.0);
        let e = cel(kc, 1.0, 1.0, kc * kc);
        assert!((k - 1.854_074_677_301_372).abs() < 1e-12, "K = {k}");
        assert!((e - 1.350_643_881_047_675_5).abs() < 1e-12, "E = {e}");
        // Trivial case: integrand is 1.
        assert!((cel(1.0, 1.0, 1.0, 1.0) - FRAC_PI_2).abs() < 1e-14);
    }
}
