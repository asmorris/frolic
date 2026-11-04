//! Developer hooks driven by environment variables (handy for automated checks):
//!
//! * `FROLIC_PRESET=<index>`: start with the n-th preset from the Scene menu.
//! * `FROLIC_SCREENSHOT=<path.png>`: save a screenshot once the scene has settled, then exit.

use crate::presets::{LoadPreset, Preset};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        if let Some(i) = std::env::var("FROLIC_PRESET").ok().and_then(|s| s.parse::<usize>().ok()) {
            let preset = Preset::ALL[i.min(Preset::ALL.len() - 1)];
            app.add_systems(PostStartup, move |mut ev: EventWriter<LoadPreset>| {
                ev.write(LoadPreset(preset));
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
