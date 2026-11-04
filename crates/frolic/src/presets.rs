//! Built-in magnet kinds and example scenes.

use crate::magnets::{MagnetBody, NameCounter, Selection};
use bevy::math::DVec3;
use bevy::prelude::*;
use frolic_core::Shape;
use std::f32::consts::FRAC_PI_2;

pub struct PresetPlugin;

impl Plugin for PresetPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoadPreset>()
            .add_systems(Startup, |mut ev: EventWriter<LoadPreset>| {
                ev.write(LoadPreset(Preset::AttractingBars));
            })
            .add_systems(Update, load_preset);
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    SingleBar,
    AttractingBars,
    RepellingBars,
    ParallelBars,
    Horseshoe,
    Halbach,
    DiscStack,
    Mixed,
    Empty,
}

impl Preset {
    pub const ALL: [Preset; 9] = [
        Self::SingleBar,
        Self::AttractingBars,
        Self::RepellingBars,
        Self::ParallelBars,
        Self::Horseshoe,
        Self::Halbach,
        Self::DiscStack,
        Self::Mixed,
        Self::Empty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SingleBar => "Single bar magnet",
            Self::AttractingBars => "Two bars, attracting (N→S)",
            Self::RepellingBars => "Two bars, repelling (N↔N)",
            Self::ParallelBars => "Side-by-side bars (antiparallel)",
            Self::Horseshoe => "Horseshoe (three blocks)",
            Self::Halbach => "Halbach array (5 cubes)",
            Self::DiscStack => "Disc magnets with a gap",
            Self::Mixed => "Mixed shapes",
            Self::Empty => "Empty scene",
        }
    }
}

#[derive(Event, Clone, Copy)]
pub struct LoadPreset(pub Preset);

fn load_preset(
    mut commands: Commands,
    mut events: EventReader<LoadPreset>,
    existing: Query<Entity, With<MagnetBody>>,
    mut names: ResMut<NameCounter>,
    mut selection: ResMut<Selection>,
) {
    let Some(LoadPreset(preset)) = events.read().last().copied() else { return };
    for e in &existing {
        commands.entity(e).despawn();
    }
    *names = NameCounter::default();
    selection.0 = None;

    let mut spawn = |name: &str, shape: Shape, dir: DVec3, pos: Vec3, rot: Quat| {
        let body = MagnetBody::new(names.next(name), shape, dir);
        commands.spawn((body, Transform::from_translation(pos).with_rotation(rot)));
    };
    let bar = Shape::Cuboid { size: DVec3::new(4.0, 1.0, 1.0) };
    let id = Quat::IDENTITY;
    match preset {
        Preset::SingleBar => spawn("Bar", bar, DVec3::X, Vec3::ZERO, id),
        Preset::AttractingBars => {
            spawn("Bar", bar, DVec3::X, Vec3::new(-3.0, 0.0, 0.0), id);
            spawn("Bar", bar, DVec3::X, Vec3::new(3.0, 0.0, 0.0), id);
        }
        Preset::RepellingBars => {
            spawn("Bar", bar, DVec3::X, Vec3::new(-3.0, 0.0, 0.0), id);
            spawn("Bar", bar, -DVec3::X, Vec3::new(3.0, 0.0, 0.0), id);
        }
        Preset::ParallelBars => {
            spawn("Bar", bar, DVec3::X, Vec3::new(0.0, 0.0, -1.5), id);
            spawn("Bar", bar, -DVec3::X, Vec3::new(0.0, 0.0, 1.5), id);
        }
        Preset::Horseshoe => {
            let leg = Shape::Cuboid { size: DVec3::new(1.0, 3.0, 1.0) };
            let base = Shape::Cuboid { size: DVec3::new(4.0, 1.0, 1.0) };
            // Magnetization follows the U: down the left leg, across, up the right leg.
            spawn("Leg", leg, -DVec3::Y, Vec3::new(-1.5, 1.0, 0.0), id);
            spawn("Base", base, DVec3::X, Vec3::new(0.0, -1.0, 0.0), id);
            spawn("Leg", leg, DVec3::Y, Vec3::new(1.5, 1.0, 0.0), id);
        }
        Preset::Halbach => {
            let cube = Shape::Cuboid { size: DVec3::splat(1.2) };
            // Magnetization rotates by 90° per cube: strong field below, weak above.
            let dirs = [DVec3::X, DVec3::NEG_Y, DVec3::NEG_X, DVec3::Y, DVec3::X];
            for (i, d) in dirs.into_iter().enumerate() {
                spawn("Cube", cube, d, Vec3::new((i as f32 - 2.0) * 1.2, 0.0, 0.0), id);
            }
        }
        Preset::DiscStack => {
            let disc = Shape::Cylinder { radius: 1.2, height: 0.4 };
            let up = Quat::from_rotation_x(-FRAC_PI_2);
            for y in [-1.4, -1.0, 1.0, 1.4] {
                spawn("Disc", disc, DVec3::Z, Vec3::new(0.0, y, 0.0), up);
            }
        }
        Preset::Mixed => {
            spawn("Bar", bar, DVec3::X, Vec3::new(-2.5, 0.0, -1.5), id);
            spawn(
                "Cylinder",
                Shape::Cylinder { radius: 0.6, height: 2.5 },
                DVec3::Z,
                Vec3::new(2.5, 0.0, -1.0),
                Quat::from_rotation_x(-FRAC_PI_2),
            );
            spawn("Sphere", Shape::Sphere { radius: 0.8 }, DVec3::X, Vec3::new(0.5, 0.0, 2.5), id);
        }
        Preset::Empty => {}
    }
}
