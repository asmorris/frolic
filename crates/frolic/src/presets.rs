//! Built-in magnet kinds and the example scenes shipped in the repo's `scenes/` folder.

use crate::magnets::{MagnetBody, NameCounter};
use crate::persistence::SceneCommand;
use crate::scene_file::SceneFile;
use bevy::math::DVec3;
use bevy::prelude::*;
use frolic_core::Shape;
use std::f32::consts::FRAC_PI_2;
use std::sync::OnceLock;

pub struct PresetPlugin;

impl Plugin for PresetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_default_scene);
    }
}

/// The example scenes, embedded from `scenes/*.frolic.json` at compile time.
const BUILTIN_JSON: [&str; 8] = [
    include_str!("../../../scenes/01-single-bar.frolic.json"),
    include_str!("../../../scenes/02-attracting-bars.frolic.json"),
    include_str!("../../../scenes/03-repelling-bars.frolic.json"),
    include_str!("../../../scenes/04-side-by-side-bars.frolic.json"),
    include_str!("../../../scenes/05-horseshoe.frolic.json"),
    include_str!("../../../scenes/06-halbach-array.frolic.json"),
    include_str!("../../../scenes/07-disc-stacks.frolic.json"),
    include_str!("../../../scenes/08-mixed-shapes.frolic.json"),
];

/// Index of the scene shown at startup.
pub const DEFAULT_SCENE: usize = 1;

pub fn builtin_scenes() -> &'static [SceneFile] {
    static SCENES: OnceLock<Vec<SceneFile>> = OnceLock::new();
    SCENES.get_or_init(|| {
        BUILTIN_JSON
            .iter()
            .map(|json| SceneFile::from_json(json).expect("built-in scene files are valid"))
            .collect()
    })
}

fn load_default_scene(mut commands: EventWriter<SceneCommand>) {
    // A scene file given on the command line takes precedence.
    if std::env::args().skip(1).any(|a| !a.starts_with('-')) {
        return;
    }
    let scene = builtin_scenes()[DEFAULT_SCENE].clone();
    commands.write(SceneCommand::Load { scene, path: None, force: true });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagnetKind {
    Bar,
    Cube,
    Cylinder,
    Disc,
    Sphere,
}

impl MagnetKind {
    pub const ALL: [MagnetKind; 5] = [Self::Bar, Self::Cube, Self::Cylinder, Self::Disc, Self::Sphere];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Cube => "Cube",
            Self::Cylinder => "Cylinder",
            Self::Disc => "Disc",
            Self::Sphere => "Sphere",
        }
    }

    pub fn body(self, names: &mut NameCounter) -> MagnetBody {
        let (shape, dir) = match self {
            Self::Bar => (Shape::Cuboid { size: DVec3::new(4.0, 1.0, 1.0) }, DVec3::X),
            Self::Cube => (Shape::Cuboid { size: DVec3::splat(1.5) }, DVec3::Y),
            Self::Cylinder => (Shape::Cylinder { radius: 0.6, height: 2.5 }, DVec3::Z),
            Self::Disc => (Shape::Cylinder { radius: 1.2, height: 0.4 }, DVec3::Z),
            Self::Sphere => (Shape::Sphere { radius: 0.8 }, DVec3::Z),
        };
        MagnetBody::new(names.next(self.label()), shape, dir)
    }

    /// A pleasant default orientation (cylinders and discs stand upright, N up).
    pub fn default_rotation(self) -> Quat {
        match self {
            Self::Cylinder | Self::Disc | Self::Sphere => Quat::from_rotation_x(-FRAC_PI_2),
            _ => Quat::IDENTITY,
        }
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn builtin_scenes_parse() {
        assert_eq!(super::builtin_scenes().len(), 8);
    }
}
