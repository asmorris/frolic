//! Frolic: an interactive 3D magnetic field simulator.

mod camera;
mod devtools;
mod field;
mod interaction;
mod magnets;
mod meshgen;
mod persistence;
mod presets;
mod scene_file;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.045, 0.05, 0.065)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Frolic — magnetic field simulator".into(),
                resolution: (1600.0, 1000.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin { enable_multipass_for_primary_context: true })
        .add_plugins((
            camera::CameraPlugin,
            magnets::MagnetPlugin,
            field::FieldPlugin,
            interaction::InteractionPlugin,
            ui::UiPlugin,
            presets::PresetPlugin,
            persistence::PersistencePlugin,
            devtools::DevToolsPlugin,
        ))
        .run();
}
