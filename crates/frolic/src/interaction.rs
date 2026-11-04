//! Mouse / keyboard interaction with magnets.
//!
//! * Left-drag a magnet: slide it horizontally.  Shift: move it vertically.
//!   Alt/Option: rotate it (horizontal mouse = spin about vertical axis,
//!   vertical mouse = tilt toward/away from the camera).
//! * Left-drag empty space: orbit.  Right/middle-drag or Shift+left-drag: pan.
//! * Click empty space: deselect.

use crate::camera::OrbitCamera;
use crate::magnets::{ray_hit, MagnetBody, NameCounter, Selection};
use crate::ui::UiState;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<EditSettings>()
            .add_systems(Update, (pointer_input, keyboard_shortcuts));
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct EditSettings {
    /// Snap dragged positions to this grid (0 = off).
    pub snap: f32,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self { snap: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum DragMode {
    #[default]
    None,
    /// The press started over the UI; ignore it in the 3D view.
    Ui,
    Orbit,
    Pan,
    Move {
        entity: Entity,
        plane_point: Vec3,
        plane_normal: Vec3,
        grab_offset: Vec3,
        vertical: bool,
    },
    Rotate {
        entity: Entity,
    },
}

#[derive(Resource, Default, Debug)]
pub struct DragState {
    pub mode: DragMode,
    moved: f32,
}

impl DragState {
    /// True while a magnet is being moved or rotated.
    pub fn is_manipulating(&self) -> bool {
        matches!(self.mode, DragMode::Move { .. } | DragMode::Rotate { .. })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn pointer_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    ui: Res<UiState>,
    edit: Res<EditSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    mut drag: ResMut<DragState>,
    mut selection: ResMut<Selection>,
    mut magnets: Query<(Entity, &MagnetBody, &mut Transform)>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_tf)) = cameras.single() else { return };
    let ray = window.cursor_position().and_then(|c| camera.viewport_to_world(cam_tf, c).ok());
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);

    // --- Press ---------------------------------------------------------------------
    if buttons.just_pressed(MouseButton::Left) && drag.mode == DragMode::None {
        drag.moved = 0.0;
        drag.mode = if ui.pointer_over {
            DragMode::Ui
        } else {
            let hit = ray.and_then(|ray| {
                magnets
                    .iter()
                    .filter_map(|(e, body, t)| ray_hit(body, t, ray.origin, *ray.direction).map(|d| (e, d, t.translation)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(e, d, pos)| (e, ray.origin + *ray.direction * d, pos))
            });
            match hit {
                Some((entity, point, pos)) => {
                    selection.0 = Some(entity);
                    if alt {
                        DragMode::Rotate { entity }
                    } else if shift {
                        // Vertical plane through the grab point, facing the camera.
                        let fwd = cam_tf.forward().as_vec3();
                        let normal = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::Z);
                        DragMode::Move { entity, plane_point: point, plane_normal: normal, grab_offset: point - pos, vertical: true }
                    } else {
                        DragMode::Move { entity, plane_point: point, plane_normal: Vec3::Y, grab_offset: point - pos, vertical: false }
                    }
                }
                None if shift => DragMode::Pan,
                None => DragMode::Orbit,
            }
        };
    }
    if (buttons.just_pressed(MouseButton::Right) || buttons.just_pressed(MouseButton::Middle))
        && drag.mode == DragMode::None
    {
        drag.mode = if ui.pointer_over { DragMode::Ui } else { DragMode::Pan };
    }

    drag.moved += motion.delta.length();

    // --- Drag ----------------------------------------------------------------------
    match drag.mode {
        DragMode::Move { entity, plane_point, plane_normal, grab_offset, vertical } => {
            if let (Some(ray), Ok((_, _, mut t))) = (ray, magnets.get_mut(entity)) {
                let denom = ray.direction.dot(plane_normal);
                if denom.abs() > 1e-4 {
                    let dist = (plane_point - ray.origin).dot(plane_normal) / denom;
                    if dist > 0.0 {
                        let mut target = ray.origin + *ray.direction * dist - grab_offset;
                        if edit.snap > 0.0 {
                            target = (target / edit.snap).round() * edit.snap;
                        }
                        let mut new = t.translation;
                        if vertical {
                            new.y = target.y;
                        } else {
                            new.x = target.x;
                            new.z = target.z;
                        }
                        if new != t.translation {
                            t.translation = new;
                        }
                    }
                }
            }
        }
        DragMode::Rotate { entity } => {
            let d = motion.delta;
            if d != Vec2::ZERO {
                if let Ok((_, _, mut t)) = magnets.get_mut(entity) {
                    let right = cam_tf.right().as_vec3();
                    let q = Quat::from_axis_angle(Vec3::Y, d.x * 0.01) * Quat::from_axis_angle(right, d.y * 0.01);
                    t.rotation = (q * t.rotation).normalize();
                }
            }
        }
        _ => {}
    }

    // --- Release -------------------------------------------------------------------
    let any_down = buttons.any_pressed([MouseButton::Left, MouseButton::Right, MouseButton::Middle]);
    if !any_down && drag.mode != DragMode::None {
        if drag.mode == DragMode::Orbit && drag.moved < 3.0 {
            selection.0 = None; // simple click on empty space
        }
        drag.mode = DragMode::None;
    }
}

#[allow(clippy::too_many_arguments)]
fn keyboard_shortcuts(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiState>,
    mut selection: ResMut<Selection>,
    mut names: ResMut<NameCounter>,
    mut magnets: Query<(&MagnetBody, &mut Transform)>,
    mut cameras: Query<&mut OrbitCamera>,
) {
    if ui.keyboard {
        return;
    }
    let Some(entity) = selection.0 else { return };
    let Ok((body, mut t)) = magnets.get_mut(entity) else {
        selection.0 = None;
        return;
    };
    let cmd = keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight, KeyCode::ControlLeft, KeyCode::ControlRight]);

    if keys.any_just_pressed([KeyCode::Delete, KeyCode::Backspace]) {
        commands.entity(entity).despawn();
        selection.0 = None;
    } else if keys.just_pressed(KeyCode::Escape) {
        selection.0 = None;
    } else if keys.just_pressed(KeyCode::KeyF) {
        if let Ok(mut cam) = cameras.single_mut() {
            cam.focus = t.translation;
        }
    } else if keys.just_pressed(KeyCode::KeyQ) {
        t.rotate_y(15f32.to_radians());
    } else if keys.just_pressed(KeyCode::KeyE) {
        t.rotate_y(-15f32.to_radians());
    } else if cmd && keys.just_pressed(KeyCode::KeyD) {
        let new = duplicate(&mut commands, &mut names, body, &t);
        selection.0 = Some(new);
    }
}

/// Spawn a copy of a magnet next to the original.
pub fn duplicate(commands: &mut Commands, names: &mut NameCounter, body: &MagnetBody, t: &Transform) -> Entity {
    let mut copy = body.clone();
    let base = copy.name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ').to_string();
    copy.name = names.next(if base.is_empty() { "Magnet" } else { &base });
    let offset = Vec3::Z * (2.0 * body.shape.bounding_radius() as f32 + 0.5);
    commands.spawn((copy, Transform::from_translation(t.translation + offset).with_rotation(t.rotation))).id()
}
