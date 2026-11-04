//! Orbit camera: left-drag orbits, right/middle-drag (or shift+left-drag) pans,
//! scroll / pinch zooms. Which drag is active is decided in [`crate::interaction`].

use crate::interaction::{DragMode, DragState};
use crate::ui::UiState;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, orbit_camera.after(crate::interaction::pointer_input));
    }
}

#[derive(Component, Debug, Clone)]
pub struct OrbitCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub radius: f32,
}

impl OrbitCamera {
    fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.focus + self.radius * Vec3::new(cp * sy, sp, cp * cy)
    }
}

fn setup(mut commands: Commands) {
    let orbit = OrbitCamera { focus: Vec3::ZERO, yaw: 0.6, pitch: 0.55, radius: 16.0 };
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(orbit.eye()).looking_at(orbit.focus, Vec3::Y),
        orbit,
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadows_enabled: false, ..default() },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 2_500.0, shadows_enabled: false, ..default() },
        Transform::from_xyz(-6.0, -3.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(AmbientLight { brightness: 250.0, ..default() });
}

fn orbit_camera(
    drag: Res<DragState>,
    ui: Res<UiState>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut pinch: EventReader<PinchGesture>,
    mut cameras: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    let Ok((mut orbit, mut transform)) = cameras.single_mut() else { return };
    let delta = motion.delta;
    match drag.mode {
        DragMode::Orbit => {
            orbit.yaw -= delta.x * 0.006;
            orbit.pitch = (orbit.pitch + delta.y * 0.006).clamp(-1.55, 1.55);
        }
        DragMode::Pan => {
            let right = transform.right();
            let up = transform.up();
            let k = orbit.radius * 0.0016;
            orbit.focus += (-right * delta.x + up * delta.y) * k;
        }
        _ => {}
    }

    if !ui.pointer_over {
        let zoom = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y * 0.12,
            MouseScrollUnit::Pixel => scroll.delta.y * 0.004,
        };
        let pinch_zoom: f32 = pinch.read().map(|p| p.0).sum();
        let factor = (-(zoom + 2.0 * pinch_zoom)).exp();
        orbit.radius = (orbit.radius * factor).clamp(1.5, 300.0);
    } else {
        pinch.clear();
    }

    let eye = orbit.eye();
    *transform = Transform::from_translation(eye).looking_at(orbit.focus, Vec3::Y);
}
