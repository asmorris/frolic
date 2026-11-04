//! Developer hooks driven by environment variables (handy for automated checks):
//!
//! * `FROLIC_PRESET=<index>`: start with the n-th built-in example scene (0-based).
//! * `FROLIC_SCREENSHOT=<path.png>`: save a screenshot once the scene has settled, then exit.
//! * `FROLIC_SAVE=1`: save the open scene back to its file once it has settled (with a
//!   scene path on the command line, this round-trips the file through the app).

use crate::persistence::SceneCommand;
use crate::presets::builtin_scenes;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        if let Some(i) = std::env::var("FROLIC_PRESET").ok().and_then(|s| s.parse::<usize>().ok()) {
            app.add_systems(PostStartup, move |mut ev: EventWriter<SceneCommand>| {
                let scenes = builtin_scenes();
                let scene = scenes[i.min(scenes.len() - 1)].clone();
                ev.write(SceneCommand::Load { scene, path: None, force: true });
            });
        }
        if std::env::var("FROLIC_SAVE").is_ok() {
            app.add_systems(Update, |mut frame: Local<u32>, mut ev: EventWriter<SceneCommand>| {
                *frame += 1;
                if *frame == 100 {
                    ev.write(SceneCommand::Save);
                }
            });
        }
        if let Ok(path) = std::env::var("FROLIC_SCREENSHOT") {
            app.add_systems(Update, move |mut commands: Commands, mut frame: Local<u32>, mut exit: EventWriter<AppExit>| {
                *frame += 1;
                if *frame == 120 {
                    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
                }
                if *frame == 160 {
                    exit.write(AppExit::Success);
                }
            });
        }
    }
}
