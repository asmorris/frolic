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

/// Cast a world-space ray against a magnet; returns the hit distance.
pub fn ray_hit(body: &MagnetBody, t: &Transform, origin: Vec3, dir: Vec3) -> Option<f32> {
    let inv: DQuat = t.rotation.as_dquat().inverse();
    let o = inv * (origin - t.translation).as_dvec3();
    let d = inv * dir.as_dvec3();
    body.shape.ray_intersect(o, d).map(|t| t as f32)
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
