//! Magnet entities: the ECS-side description of a magnet and its conversion to the
//! physics core.

use crate::meshgen;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::render::view::NoFrustumCulling;
use frolic_core::{Magnet, Pose, Shape};

pub struct MagnetPlugin;

impl Plugin for MagnetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<NameCounter>()
            .add_systems(Startup, setup_material)
            .add_systems(PostUpdate, attach_meshes);
    }
}

/// A uniformly magnetized permanent magnet. Its pose is the entity's [`Transform`]
/// (translation + rotation; scale is ignored, size lives in `shape`).
#[derive(Component, Clone, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct MagnetBody {
    pub name: String,
    pub shape: Shape,
    /// Remanence Br in tesla.
    pub remanence: f64,
    /// Magnetization direction in the magnet's local frame.
    pub direction: DVec3,
}

impl MagnetBody {
    pub fn new(name: impl Into<String>, shape: Shape, direction: DVec3) -> Self {
        Self { name: name.into(), shape, remanence: frolic_core::NDFEB_N42_BR, direction }
    }

    pub fn polarization(&self) -> DVec3 {
        self.direction.normalize_or_zero() * self.remanence
    }

    pub fn to_core(&self, transform: &Transform) -> Magnet {
        Magnet::new(self.shape, pose_of(transform), self.polarization())
    }
}

pub fn pose_of(t: &Transform) -> Pose {
    Pose::new(t.translation.as_dvec3(), t.rotation.as_dquat())
}

/// Intrinsic yaw (Y), pitch (X), roll (Z) angles in radians, i.e. the inverse of
/// `Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll)`. Unlike `Quat::to_euler`, this is
/// computed in f64 and stays exact at ±90° pitch (gimbal lock), where it sets roll to 0.
pub fn euler_yxz(q: Quat) -> (f32, f32, f32) {
    let m = bevy::math::DMat3::from_quat(q.as_dquat().normalize());
    // m.col(c)[r] is row r, column c.
    let at = |r: usize, c: usize| m.col(c)[r];
    let (m10, m11, m12) = (at(1, 0), at(1, 1), at(1, 2));
    let pitch = (-m12).atan2((m10 * m10 + m11 * m11).sqrt());
    let (yaw, roll) = if m12.abs() > 1.0 - 1e-12 {
        ((-at(2, 0)).atan2(at(0, 0)), 0.0)
    } else {
        (at(0, 2).atan2(at(2, 2)), m10.atan2(m11))
    };
    (yaw as f32, pitch as f32, roll as f32)
}

/// Whether two unit quaternions describe the same rotation (q and -q are equivalent).
#[cfg(test)]
pub fn same_rotation(a: Quat, b: Quat) -> bool {
    (a - b).length() < 2e-5 || (a + b).length() < 2e-5
}

/// Cast a world-space ray against a magnet; returns the hit distance.
pub fn ray_hit(body: &MagnetBody, t: &Transform, origin: Vec3, dir: Vec3) -> Option<f32> {
    let inv: DQuat = t.rotation.as_dquat().inverse();
    let o = inv * (origin - t.translation).as_dvec3();
    let d = inv * dir.as_dvec3();
    body.shape.ray_intersect(o, d).map(|t| t as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_round_trips_including_gimbal_lock() {
        for (y, p, r) in [(45.0f32, -90.0f32, 0.0f32), (10.0, 90.0, 0.0), (30.0, 20.0, -50.0), (-170.0, -45.0, 120.0), (0.0, 0.0, 0.0)] {
            let q = Quat::from_euler(EulerRot::YXZ, y.to_radians(), p.to_radians(), r.to_radians());
            let (a, b, c) = euler_yxz(q);
            let q2 = Quat::from_euler(EulerRot::YXZ, a, b, c);
            assert!(same_rotation(q, q2), "({y},{p},{r}) -> ({a},{b},{c})");
        }
    }
}

#[derive(Resource, Default)]
pub struct Selection(pub Option<Entity>);

/// Hands out "Bar 1", "Bar 2", … names.
#[derive(Resource, Default)]
pub struct NameCounter(std::collections::HashMap<String, u32>);

impl NameCounter {
    pub fn next(&mut self, base: &str) -> String {
        let n = self.0.entry(base.to_string()).or_insert(0);
        *n += 1;
        format!("{base} {n}")
    }

    /// Account for an existing name like "Bar 3" so the next "Bar" becomes "Bar 4".
    pub fn observe(&mut self, name: &str) {
        if let Some((base, num)) = name.rsplit_once(' ') {
            if let Ok(n) = num.parse::<u32>() {
                let e = self.0.entry(base.to_string()).or_insert(0);
                *e = (*e).max(n);
            }
        }
    }
}

#[derive(Resource)]
struct MagnetMaterial(Handle<StandardMaterial>);

fn setup_material(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    // White base colour: the pole colours come from per-vertex colours.
    let handle = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.42,
        metallic: 0.15,
        reflectance: 0.4,
        ..default()
    });
    commands.insert_resource(MagnetMaterial(handle));
}

/// (Re)build the render mesh whenever a magnet is created or its shape/magnetization changes.
fn attach_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<MagnetMaterial>,
    changed: Query<(Entity, &MagnetBody, Option<&Mesh3d>), Changed<MagnetBody>>,
) {
    for (entity, body, existing) in &changed {
        let mesh = meshgen::magnet_mesh(body);
        match existing.and_then(|h| meshes.get_mut(&h.0)) {
            Some(m) => *m = mesh,
            None => {
                commands.entity(entity).insert((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material.0.clone()),
                    // Meshes are edited in place; skip stale-AABB culling (few entities).
                    NoFrustumCulling,
                ));
            }
        }
    }
}
