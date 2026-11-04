//! The `.frolic.json` scene format and compact share codes.
//!
//! A scene file is plain, human-editable JSON:
//!
//! ```json
//! {
//!   "format": "frolic-scene",
//!   "version": 1,
//!   "name": "Two bars, attracting",
//!   "magnets": [
//!     {
//!       "name": "Bar 1",
//!       "shape": { "type": "block", "size": [4.0, 1.0, 1.0] },
//!       "position": [-3.0, 0.0, 0.0],
//!       "rotation": { "yaw_deg": 0.0, "pitch_deg": 0.0, "roll_deg": 0.0 },
//!       "remanence_tesla": 1.3,
//!       "magnetization": [1.0, 0.0, 0.0]
//!     }
//!   ]
//! }
//! ```
//!
//! Lengths are in cm, rotations are intrinsic yaw (about Y), pitch (X), roll (Z) in
//! degrees, and `magnetization` is the north-pole direction in the magnet's own frame.
//! Unknown fields are ignored, so newer files degrade gracefully; `version` is bumped
//! only for changes that older versions could not read correctly.
//!
//! A share code is the same JSON, deflate-compressed and base64url-encoded behind a
//! `FROLIC1:` prefix, so a scene fits in a chat message.

use crate::magnets::{euler_yxz, MagnetBody};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use bevy::math::DVec3;
use bevy::prelude::*;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use frolic_core::Shape;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};

pub const FORMAT: &str = "frolic-scene";
pub const VERSION: u32 = 1;
pub const SHARE_PREFIX: &str = "FROLIC1:";
pub const FILE_SUFFIX: &str = ".frolic.json";
const MAX_MAGNETS: usize = 2000;
const MAX_DECOMPRESSED: u64 = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SceneFile {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDef>,
    #[serde(default)]
    pub magnets: Vec<MagnetDef>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CameraDef {
    pub focus: [f32; 3],
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MagnetDef {
    pub name: String,
    pub shape: ShapeDef,
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation: RotationDef,
    pub remanence_tesla: f64,
    pub magnetization: [f64; 3],
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ShapeDef {
    Block { size: [f64; 3] },
    Cylinder { radius: f64, height: f64 },
    Sphere { radius: f64 },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct RotationDef {
    #[serde(default)]
    pub yaw_deg: f32,
    #[serde(default)]
    pub pitch_deg: f32,
    #[serde(default)]
    pub roll_deg: f32,
}

#[derive(Debug)]
pub enum SceneError {
    Json(serde_json::Error),
    NotAScene,
    UnsupportedVersion(u32),
    Invalid(String),
    BadShareCode,
    Io(std::io::Error),
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "not valid scene JSON ({e})"),
            Self::NotAScene => write!(f, "this isn't a Frolic scene"),
            Self::UnsupportedVersion(v) => {
                write!(f, "scene format version {v} is newer than this Frolic understands (v{VERSION}); please update")
            }
            Self::Invalid(m) => write!(f, "invalid scene: {m}"),
            Self::BadShareCode => write!(f, "that share code is incomplete or corrupted"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SceneError {}

impl SceneFile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            name: name.into(),
            description: String::new(),
            camera: None,
            magnets: Vec::new(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("scene serialization cannot fail") + "\n"
    }

    pub fn from_json(text: &str) -> Result<Self, SceneError> {
        // Check identity first, so a random JSON file gives a helpful error.
        let value: serde_json::Value = serde_json::from_str(text).map_err(SceneError::Json)?;
        if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
            return Err(SceneError::NotAScene);
        }
        if let Some(v) = value.get("version").and_then(|v| v.as_u64()) {
            if v > VERSION as u64 {
                return Err(SceneError::UnsupportedVersion(v as u32));
            }
        }
        let scene: SceneFile = serde_json::from_value(value).map_err(SceneError::Json)?;
        scene.validate()?;
        Ok(scene)
    }

    pub fn to_share_code(&self) -> String {
        let json = serde_json::to_vec(self).expect("scene serialization cannot fail");
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&json).expect("in-memory write");
        let bytes = enc.finish().expect("in-memory write");
        format!("{SHARE_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn from_share_code(code: &str) -> Result<Self, SceneError> {
        // Codes pasted from chat may be wrapped across lines.
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let body = compact.strip_prefix(SHARE_PREFIX).ok_or(SceneError::BadShareCode)?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| SceneError::BadShareCode)?;
        let mut json = String::new();
        DeflateDecoder::new(bytes.as_slice())
            .take(MAX_DECOMPRESSED)
            .read_to_string(&mut json)
            .map_err(|_| SceneError::BadShareCode)?;
        Self::from_json(&json)
    }

    /// Accepts either a share code or raw scene JSON.
    pub fn parse_any(text: &str) -> Result<Self, SceneError> {
        let t = text.trim();
        if t.starts_with('{') {
            Self::from_json(t)
        } else if t.contains(SHARE_PREFIX) {
            Self::from_share_code(&t[t.find(SHARE_PREFIX).unwrap_or(0)..])
        } else {
            Err(SceneError::BadShareCode)
        }
    }

    pub fn read(path: &std::path::Path) -> Result<Self, SceneError> {
        let text = std::fs::read_to_string(path).map_err(SceneError::Io)?;
        Self::from_json(&text)
    }

    fn validate(&self) -> Result<(), SceneError> {
        let bad = |m: String| Err(SceneError::Invalid(m));
        if self.magnets.len() > MAX_MAGNETS {
            return bad(format!("too many magnets ({} > {MAX_MAGNETS})", self.magnets.len()));
        }
        let finite = |xs: &[f64]| xs.iter().all(|x| x.is_finite());
        let size_ok = |x: f64| x.is_finite() && x > 0.0 && x <= 1000.0;
        for (i, m) in self.magnets.iter().enumerate() {
            let who = if m.name.is_empty() { format!("magnet #{}", i + 1) } else { format!("\"{}\"", m.name) };
            let dims_ok = match m.shape {
                ShapeDef::Block { size } => size.iter().all(|&x| size_ok(x)),
                ShapeDef::Cylinder { radius, height } => size_ok(radius) && size_ok(height),
                ShapeDef::Sphere { radius } => size_ok(radius),
            };
            if !dims_ok {
                return bad(format!("{who} has a size that isn't between 0 and 1000 cm"));
            }
            let pos: Vec<f64> = m.position.iter().map(|&x| x as f64).collect();
            let rot = [m.rotation.yaw_deg as f64, m.rotation.pitch_deg as f64, m.rotation.roll_deg as f64];
            if !finite(&pos) || !finite(&rot) || pos.iter().any(|x| x.abs() > 1e5) {
                return bad(format!("{who} has an invalid position or rotation"));
            }
            if !finite(&m.magnetization) || !m.remanence_tesla.is_finite() || !(0.0..=10.0).contains(&m.remanence_tesla) {
                return bad(format!("{who} has an invalid magnetization"));
            }
        }
        if let Some(c) = self.camera {
            let all = [c.focus[0], c.focus[1], c.focus[2], c.yaw_deg, c.pitch_deg, c.distance];
            if !all.iter().all(|x| x.is_finite()) || c.distance <= 0.0 {
                return bad("invalid camera".into());
            }
        }
        Ok(())
    }
}

/// Round for tidy, diff-friendly files (drags produce values like 2.0000002).
fn round(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    let r = (x * k).round() / k;
    if r == 0.0 { 0.0 } else { r } // no "-0.0"
}

fn round32(x: f32, places: i32) -> f32 {
    round(x as f64, places) as f32
}

impl MagnetDef {
    pub fn from_ecs(body: &MagnetBody, t: &Transform) -> Self {
        let shape = match body.shape {
            Shape::Cuboid { size } => ShapeDef::Block { size: size.to_array().map(|x| round(x, 4)) },
            Shape::Cylinder { radius, height } => ShapeDef::Cylinder { radius: round(radius, 4), height: round(height, 4) },
            Shape::Sphere { radius } => ShapeDef::Sphere { radius: round(radius, 4) },
        };
        let (yaw, pitch, roll) = euler_yxz(t.rotation);
        Self {
            name: body.name.clone(),
            shape,
            position: t.translation.to_array().map(|x| round32(x, 4)),
            rotation: RotationDef {
                yaw_deg: round32(yaw.to_degrees(), 3),
                pitch_deg: round32(pitch.to_degrees(), 3),
                roll_deg: round32(roll.to_degrees(), 3),
            },
            remanence_tesla: round(body.remanence, 4),
            magnetization: body.direction.to_array().map(|x| round(x, 5)),
        }
    }

    pub fn to_ecs(&self) -> (MagnetBody, Transform) {
        let shape = match self.shape {
            ShapeDef::Block { size } => Shape::Cuboid { size: DVec3::from_array(size) },
            ShapeDef::Cylinder { radius, height } => Shape::Cylinder { radius, height },
            ShapeDef::Sphere { radius } => Shape::Sphere { radius },
        };
        let body = MagnetBody {
            name: self.name.clone(),
            shape,
            remanence: self.remanence_tesla,
            direction: DVec3::from_array(self.magnetization),
        };
        let r = self.rotation;
        let rotation = Quat::from_euler(
            EulerRot::YXZ,
            r.yaw_deg.to_radians(),
            r.pitch_deg.to_radians(),
            r.roll_deg.to_radians(),
        );
        (body, Transform::from_translation(Vec3::from_array(self.position)).with_rotation(rotation))
    }
}

/// File-name-safe version of a scene name ("Two bars!" -> "two-bars").
pub fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "scene".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SceneFile {
        let mut s = SceneFile::new("Test scene");
        s.description = "two magnets".into();
        s.camera = Some(CameraDef { focus: [0.0, 1.0, 0.0], yaw_deg: 30.0, pitch_deg: 20.0, distance: 15.0 });
        s.magnets.push(MagnetDef {
            name: "Bar 1".into(),
            shape: ShapeDef::Block { size: [4.0, 1.0, 1.0] },
            position: [-3.0, 0.0, 0.5],
            rotation: RotationDef { yaw_deg: 45.0, pitch_deg: -90.0, roll_deg: 0.0 },
            remanence_tesla: 1.3,
            magnetization: [1.0, 0.0, 0.0],
        });
        s.magnets.push(MagnetDef {
            name: "Disc 1".into(),
            shape: ShapeDef::Cylinder { radius: 1.2, height: 0.4 },
            position: [2.0, 0.0, 0.0],
            rotation: RotationDef::default(),
            remanence_tesla: 0.4,
            magnetization: [0.0, 0.6, 0.8],
        });
        s
    }

    #[test]
    fn json_round_trip() {
        let s = sample();
        assert_eq!(SceneFile::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn share_code_round_trip_and_is_compact() {
        let s = sample();
        let code = s.to_share_code();
        assert!(code.starts_with(SHARE_PREFIX));
        assert!(code.len() < s.to_json().len(), "{} vs {}", code.len(), s.to_json().len());
        // Survives being wrapped / padded when pasted from chat.
        let wrapped: String = code.chars().enumerate().flat_map(|(i, c)| if i % 40 == 39 { vec![c, '\n'] } else { vec![c] }).collect();
        assert_eq!(SceneFile::parse_any(&format!("  here you go: {wrapped}  ")).unwrap(), s);
    }

    #[test]
    fn ecs_round_trip_preserves_pose() {
        for m in sample().magnets {
            let (body, t) = m.to_ecs();
            let back = MagnetDef::from_ecs(&body, &t);
            assert_eq!(back.position, m.position);
            let (b, _) = back.to_ecs();
            assert_eq!(b, body);
            let q2 = back.to_ecs().1.rotation;
            assert!(crate::magnets::same_rotation(q2, t.rotation), "{m:?}");
        }
    }

    #[test]
    fn minimal_hand_written_file_loads() {
        let text = r#"{ "format": "frolic-scene", "version": 1, "magnets": [
            { "name": "m", "shape": { "type": "sphere", "radius": 1 }, "position": [0,0,0],
              "remanence_tesla": 1.2, "magnetization": [0,0,1], "some_future_field": true } ] }"#;
        let s = SceneFile::from_json(text).unwrap();
        assert_eq!(s.magnets.len(), 1);
        assert_eq!(s.magnets[0].rotation, RotationDef::default());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(SceneFile::from_json(r#"{"hello": 1}"#), Err(SceneError::NotAScene)));
        assert!(matches!(
            SceneFile::from_json(r#"{"format": "frolic-scene", "version": 99}"#),
            Err(SceneError::UnsupportedVersion(99))
        ));
        let mut s = sample();
        s.magnets[0].shape = ShapeDef::Sphere { radius: -1.0 };
        assert!(matches!(SceneFile::from_json(&s.to_json()), Err(SceneError::Invalid(_))));
        assert!(SceneFile::parse_any("FROLIC1:not-real-data").is_err());
        assert!(SceneFile::parse_any("hello").is_err());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Two bars, attracting (N→S)"), "two-bars-attracting-n-s");
        assert_eq!(slug("!!!"), "scene");
    }
}
