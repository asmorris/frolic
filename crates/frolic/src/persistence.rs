//! Saving and loading scenes: native file dialogs, drag-and-drop, a command-line path,
//! keyboard shortcuts, unsaved-change tracking and the window title.

use crate::camera::OrbitCamera;
use crate::magnets::{MagnetBody, NameCounter, Selection};
use crate::scene_file::{slug, CameraDef, MagnetDef, SceneFile, FILE_SUFFIX};
use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, IoTaskPool, Task};
use bevy::window::PrimaryWindow;
use std::path::PathBuf;
use std::time::Duration;

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentScene>()
            .init_resource::<PendingLoad>()
            .init_resource::<Toasts>()
            .init_resource::<DialogTask>()
            .add_event::<SceneCommand>()
            .add_systems(Startup, open_from_command_line)
            .add_systems(
                Update,
                (shortcuts, drag_and_drop, handle_commands, poll_dialog).chain(),
            )
            .add_systems(PostUpdate, (track_changes, window_title).chain());
    }
}

/// What's currently open.
#[derive(Resource, Default)]
pub struct CurrentScene {
    pub name: String,
    pub description: String,
    pub path: Option<PathBuf>,
    /// The scene as last saved / loaded, for unsaved-change detection.
    baseline: Option<SceneFile>,
    needs_baseline: bool,
    dirty: bool,
}

impl CurrentScene {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

#[derive(Event, Clone)]
pub enum SceneCommand {
    /// Show the "Open scene" dialog.
    Open,
    /// Save to the current file, or ask for one.
    Save,
    /// Always ask for a file.
    SaveAs,
    /// Replace the current scene (asks first if there are unsaved changes, unless `force`).
    Load { scene: SceneFile, path: Option<PathBuf>, force: bool },
}

/// A load waiting for the user to confirm discarding unsaved changes.
#[derive(Resource, Default)]
pub struct PendingLoad(pub Option<(SceneFile, Option<PathBuf>)>);

#[derive(Resource, Default)]
pub struct Toasts {
    pub message: Option<(String, bool, Duration)>,
}

impl Toasts {
    pub fn info(&mut self, now: Duration, msg: impl Into<String>) {
        self.message = Some((msg.into(), false, now));
    }
    pub fn error(&mut self, now: Duration, msg: impl Into<String>) {
        self.message = Some((msg.into(), true, now));
    }
}

enum DialogOutcome {
    Opened(PathBuf, Result<SceneFile, String>),
    Saved { path: PathBuf, scene: SceneFile, result: Result<(), String> },
    Cancelled,
}

/// At most one native dialog at a time.
#[derive(Resource, Default)]
struct DialogTask(Option<Task<DialogOutcome>>);

/// Snapshot the ECS world as a scene file.
pub fn capture<'a>(
    current: &CurrentScene,
    magnets: impl IntoIterator<Item = (&'a MagnetBody, &'a Transform)>,
    camera: Option<&OrbitCamera>,
) -> SceneFile {
    let mut scene = SceneFile::new(current.name.clone());
    scene.description = current.description.clone();
    let mut defs: Vec<MagnetDef> = magnets.into_iter().map(|(b, t)| MagnetDef::from_ecs(b, t)).collect();
    // Stable order so saving twice produces identical files.
    defs.sort_by(|a, b| a.name.cmp(&b.name).then(a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal)));
    scene.magnets = defs;
    scene.camera = camera.map(|c| CameraDef {
        focus: c.focus.to_array().map(|x| (x * 1e3).round() / 1e3),
        yaw_deg: (c.yaw.to_degrees() * 10.0).round() / 10.0,
        pitch_deg: (c.pitch.to_degrees() * 10.0).round() / 10.0,
        distance: (c.radius * 100.0).round() / 100.0,
    });
    scene
}

fn open_from_command_line(mut commands: EventWriter<SceneCommand>, mut toasts: ResMut<Toasts>) {
    let Some(arg) = std::env::args().skip(1).find(|a| !a.starts_with('-')) else { return };
    let path = PathBuf::from(arg);
    match SceneFile::read(&path) {
        Ok(scene) => {
            commands.write(SceneCommand::Load { scene, path: Some(path), force: true });
        }
        Err(e) => toasts.error(Duration::ZERO, format!("Couldn't open {}: {e}", path.display())),
    }
}

fn shortcuts(keys: Res<ButtonInput<KeyCode>>, mut commands: EventWriter<SceneCommand>) {
    let cmd = keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight, KeyCode::ControlLeft, KeyCode::ControlRight]);
    if !cmd {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::KeyS) {
        commands.write(if shift { SceneCommand::SaveAs } else { SceneCommand::Save });
    } else if keys.just_pressed(KeyCode::KeyO) {
        commands.write(SceneCommand::Open);
    }
}

fn drag_and_drop(
    mut drops: EventReader<FileDragAndDrop>,
    mut commands: EventWriter<SceneCommand>,
    mut toasts: ResMut<Toasts>,
    time: Res<Time>,
) {
    for event in drops.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            match SceneFile::read(path_buf) {
                Ok(scene) => {
                    commands.write(SceneCommand::Load { scene, path: Some(path_buf.clone()), force: false });
                }
                Err(e) => toasts.error(time.elapsed(), format!("Couldn't open {}: {e}", file_name(path_buf))),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_commands(
    mut events: EventReader<SceneCommand>,
    mut cmds: Commands,
    mut current: ResMut<CurrentScene>,
    mut pending: ResMut<PendingLoad>,
    mut toasts: ResMut<Toasts>,
    mut dialog: ResMut<DialogTask>,
    mut names: ResMut<NameCounter>,
    mut selection: ResMut<Selection>,
    time: Res<Time>,
    magnets: Query<(&MagnetBody, &Transform)>,
    entities: Query<Entity, With<MagnetBody>>,
    mut cameras: Query<&mut OrbitCamera>,
) {
    let now = time.elapsed();
    for event in events.read().cloned().collect::<Vec<_>>() {
        match event {
            SceneCommand::Open => {
                if dialog.0.is_some() {
                    continue;
                }
                let start_dir = current.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
                dialog.0 = Some(IoTaskPool::get().spawn(async move {
                    let mut d = rfd::AsyncFileDialog::new()
                        .set_title("Open Frolic scene")
                        .add_filter("Frolic scene", &["json"]);
                    if let Some(dir) = start_dir {
                        d = d.set_directory(dir);
                    }
                    match d.pick_file().await {
                        Some(handle) => {
                            let path = handle.path().to_path_buf();
                            let result = SceneFile::read(&path).map_err(|e| e.to_string());
                            DialogOutcome::Opened(path, result)
                        }
                        None => DialogOutcome::Cancelled,
                    }
                }));
            }
            SceneCommand::Save if current.path.is_some() => {
                let path = current.path.clone().unwrap();
                let scene = capture(&current, magnets.iter(), cameras.single().ok());
                match std::fs::write(&path, scene.to_json()) {
                    Ok(()) => {
                        current.baseline = Some(without_camera(scene));
                        current.dirty = false;
                        toasts.info(now, format!("Saved {}", file_name(&path)));
                    }
                    Err(e) => toasts.error(now, format!("Couldn't save {}: {e}", file_name(&path))),
                }
            }
            SceneCommand::Save | SceneCommand::SaveAs => {
                if dialog.0.is_some() {
                    continue;
                }
                let scene = capture(&current, magnets.iter(), cameras.single().ok());
                let suggested = current
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("{}{FILE_SUFFIX}", slug(&current.name)));
                let start_dir = current.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
                dialog.0 = Some(IoTaskPool::get().spawn(async move {
                    let mut d = rfd::AsyncFileDialog::new()
                        .set_title("Save Frolic scene")
                        .add_filter("Frolic scene", &["json"])
                        .set_file_name(suggested);
                    if let Some(dir) = start_dir {
                        d = d.set_directory(dir);
                    }
                    match d.save_file().await {
                        Some(handle) => {
                            let path = handle.path().to_path_buf();
                            let result = std::fs::write(&path, scene.to_json()).map_err(|e| e.to_string());
                            DialogOutcome::Saved { path, scene, result }
                        }
                        None => DialogOutcome::Cancelled,
                    }
                }));
            }
            SceneCommand::Load { scene, path, force } => {
                if current.dirty && !force {
                    pending.0 = Some((scene, path));
                    continue;
                }
                pending.0 = None;
                // Replace the world.
                for e in &entities {
                    cmds.entity(e).despawn();
                }
                *names = NameCounter::default();
                for m in &scene.magnets {
                    names.observe(&m.name);
                    cmds.spawn(m.to_ecs());
                }
                selection.0 = None;
                if let (Some(c), Ok(mut cam)) = (scene.camera, cameras.single_mut()) {
                    cam.focus = Vec3::from_array(c.focus);
                    cam.yaw = c.yaw_deg.to_radians();
                    cam.pitch = c.pitch_deg.to_radians().clamp(-1.55, 1.55);
                    cam.radius = c.distance.clamp(1.5, 300.0);
                }
                current.name = scene.name.clone();
                current.description = scene.description.clone();
                if let Some(p) = &path {
                    toasts.info(now, format!("Opened {}", file_name(p)));
                }
                current.path = path;
                current.baseline = None;
                current.needs_baseline = true;
                current.dirty = false;
            }
        }
    }
}

fn poll_dialog(
    mut dialog: ResMut<DialogTask>,
    mut current: ResMut<CurrentScene>,
    mut toasts: ResMut<Toasts>,
    mut commands: EventWriter<SceneCommand>,
    time: Res<Time>,
) {
    let Some(task) = dialog.0.as_mut() else { return };
    let Some(outcome) = block_on(poll_once(task)) else { return };
    dialog.0 = None;
    let now = time.elapsed();
    match outcome {
        DialogOutcome::Opened(path, Ok(scene)) => {
            commands.write(SceneCommand::Load { scene, path: Some(path), force: false });
        }
        DialogOutcome::Opened(path, Err(e)) => toasts.error(now, format!("Couldn't open {}: {e}", file_name(&path))),
        DialogOutcome::Saved { path, scene, result: Ok(()) } => {
            toasts.info(now, format!("Saved {}", file_name(&path)));
            current.path = Some(path);
            current.baseline = Some(without_camera(scene));
            current.dirty = false;
        }
        DialogOutcome::Saved { path, result: Err(e), .. } => {
            toasts.error(now, format!("Couldn't save {}: {e}", file_name(&path)))
        }
        DialogOutcome::Cancelled => {}
    }
}

type MagnetChanges = (With<MagnetBody>, Or<(Changed<Transform>, Changed<MagnetBody>)>);

/// Recompute the unsaved-changes flag whenever something that's saved changes.
fn track_changes(
    mut current: ResMut<CurrentScene>,
    changed: Query<(), MagnetChanges>,
    mut removed: RemovedComponents<MagnetBody>,
    magnets: Query<(&MagnetBody, &Transform)>,
) {
    let something_changed = !changed.is_empty() || removed.read().count() > 0 || current.is_changed();
    if !something_changed && !current.needs_baseline {
        return;
    }
    // Camera moves aren't "changes" worth nagging about.
    let snapshot = without_camera(capture(&current, magnets.iter(), None));
    let cur = current.bypass_change_detection();
    if cur.needs_baseline {
        cur.baseline = Some(snapshot);
        cur.needs_baseline = false;
        cur.dirty = false;
    } else {
        cur.dirty = cur.baseline.as_ref() != Some(&snapshot);
    }
}

fn window_title(current: Res<CurrentScene>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut window) = windows.single_mut() else { return };
    let name = if current.name.is_empty() { "Untitled" } else { &current.name };
    let title = format!("{name}{} — Frolic", if current.dirty { " •" } else { "" });
    if window.title != title {
        window.title = title;
    }
}

fn without_camera(mut s: SceneFile) -> SceneFile {
    s.camera = None;
    s
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}
