//! Field-line computation (on a background thread) and rendering.
//!
//! Whenever a magnet moves or changes, a snapshot of the scene is sent to a worker
//! thread. The worker always skips to the newest snapshot, so a fast drag never builds
//! up a backlog; while dragging a cheaper trace is requested, and a full-quality trace
//! follows as soon as the drag ends.

use crate::interaction::DragState;
use crate::magnets::{MagnetBody, Selection};
use bevy::prelude::*;
use frolic_core::{trace_field_lines, FieldLine, FieldScene, TraceParams};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct FieldPlugin;

impl Plugin for FieldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TraceSettings>()
            .init_resource::<ViewSettings>()
            .init_resource::<FieldLines>()
            .init_gizmo_group::<FieldLineGizmos>()
            .insert_resource(FieldWorker::spawn())
            .add_systems(
                PostUpdate,
                (request_recompute, receive_results).chain().before(TransformSystem::TransformPropagate),
            )
            .add_systems(Update, (configure_gizmos, draw_field_lines, draw_scene_helpers));
    }
}

/// Settings that affect the computed lines (changing them triggers a recompute).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct TraceSettings {
    pub line_count: usize,
    /// How far lines may wander, in multiples of the scene's radius.
    pub extent: f64,
    /// Accuracy multiplier: smaller steps and more steps per line.
    pub quality: f64,
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self { line_count: 120, extent: 3.0, quality: 1.0 }
    }
}

impl TraceSettings {
    fn params(&self) -> TraceParams {
        let d = TraceParams::default();
        TraceParams {
            line_count: self.line_count,
            bounds_scale: self.extent,
            step_fraction: d.step_fraction / self.quality,
            min_step: d.min_step / self.quality,
            max_step: d.max_step / self.quality,
            max_steps: (d.max_steps as f64 * self.quality * self.extent / 3.0) as usize,
            ..d
        }
    }
}

/// Purely visual settings.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ViewSettings {
    pub show_lines: bool,
    pub line_width: f32,
    pub show_arrows: bool,
    pub arrow_spacing: f32,
    /// Colour scale range, log10(|B| / tesla).
    pub log_min: f32,
    pub log_max: f32,
    pub show_grid: bool,
    pub show_magnetization: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            show_lines: true,
            line_width: 1.6,
            show_arrows: true,
            arrow_spacing: 3.0,
            log_min: -3.3,
            log_max: -0.4,
            show_grid: true,
            show_magnetization: false,
        }
    }
}

/// The most recent field-line result, in render-ready form.
#[derive(Resource, Default)]
pub struct FieldLines {
    pub lines: Vec<RenderLine>,
    pub compute_time: Duration,
    pub point_count: usize,
    pub interactive: bool,
    pub busy: bool,
    sent_generation: u64,
    shown_generation: u64,
    last_sent_interactive: bool,
}

pub struct RenderLine {
    pub points: Vec<Vec3>,
    /// log10(|B| / tesla) per point.
    pub log_b: Vec<f32>,
}

impl From<&FieldLine> for RenderLine {
    fn from(l: &FieldLine) -> Self {
        Self {
            points: l.points.iter().map(|p| p.as_vec3()).collect(),
            log_b: l.magnitudes.iter().map(|m| (m.max(1e-12)).log10() as f32).collect(),
        }
    }
}

struct Job {
    generation: u64,
    scene: FieldScene,
    params: TraceParams,
    interactive: bool,
}

struct JobResult {
    generation: u64,
    lines: Vec<FieldLine>,
    elapsed: Duration,
    interactive: bool,
}

#[derive(Resource)]
struct FieldWorker {
    jobs: Sender<Job>,
    results: Mutex<Receiver<JobResult>>,
}

impl FieldWorker {
    fn spawn() -> Self {
        let (job_tx, job_rx) = channel::<Job>();
        let (res_tx, res_rx) = channel::<JobResult>();
        std::thread::Builder::new()
            .name("field-lines".into())
            .spawn(move || {
                while let Ok(mut job) = job_rx.recv() {
                    // Coalesce: only the newest snapshot matters.
                    while let Ok(newer) = job_rx.try_recv() {
                        job = newer;
                    }
                    let start = Instant::now();
                    let lines = trace_field_lines(&job.scene, &job.params);
                    let result = JobResult {
                        generation: job.generation,
                        lines,
                        elapsed: start.elapsed(),
                        interactive: job.interactive,
                    };
                    if res_tx.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn field-line worker");
        Self { jobs: job_tx, results: Mutex::new(res_rx) }
    }
}

type ChangedMagnets = (With<MagnetBody>, Or<(Changed<Transform>, Changed<MagnetBody>)>);

fn request_recompute(
    changed: Query<(), ChangedMagnets>,
    mut removed: RemovedComponents<MagnetBody>,
    magnets: Query<(&MagnetBody, &Transform)>,
    settings: Res<TraceSettings>,
    drag: Res<DragState>,
    worker: Res<FieldWorker>,
    mut state: ResMut<FieldLines>,
) {
    let dragging = drag.is_manipulating();
    let dirty = !changed.is_empty() || removed.read().count() > 0 || settings.is_changed();
    let refine = state.last_sent_interactive && !dragging;
    if !dirty && !refine {
        return;
    }
    let mut scene = FieldScene::new();
    for (body, transform) in &magnets {
        scene.add(body.to_core(transform));
    }
    let full = settings.params();
    let params = if dragging { full.interactive() } else { full };
    state.sent_generation += 1;
    state.last_sent_interactive = dragging;
    state.busy = true;
    let _ = worker.jobs.send(Job { generation: state.sent_generation, scene, params, interactive: dragging });
}

fn receive_results(worker: Res<FieldWorker>, mut state: ResMut<FieldLines>) {
    let Ok(rx) = worker.results.lock() else { return };
    let mut newest: Option<JobResult> = None;
    while let Ok(r) = rx.try_recv() {
        newest = Some(r);
    }
    let Some(r) = newest else { return };
    if r.generation < state.shown_generation {
        return;
    }
    state.lines = r.lines.iter().map(RenderLine::from).collect();
    state.point_count = r.lines.iter().map(|l| l.points.len()).sum();
    state.compute_time = r.elapsed;
    state.interactive = r.interactive;
    state.shown_generation = r.generation;
    state.busy = r.generation != state.sent_generation;
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct FieldLineGizmos;

fn configure_gizmos(view: Res<ViewSettings>, mut store: ResMut<GizmoConfigStore>) {
    if !view.is_changed() {
        return;
    }
    let (config, _) = store.config_mut::<FieldLineGizmos>();
    config.line.width = view.line_width;
    config.line.joints = GizmoLineJoint::Miter;
}

/// Perceptual-ish colour scale for |B|: deep blue → cyan → green → yellow → white.
pub fn field_color(t: f32) -> Color {
    const STOPS: [(f32, [f32; 3]); 5] = [
        (0.00, [0.22, 0.24, 0.78]),
        (0.30, [0.10, 0.62, 0.92]),
        (0.55, [0.30, 0.88, 0.55]),
        (0.80, [0.98, 0.84, 0.25]),
        (1.00, [1.00, 0.98, 0.90]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in STOPS.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            let k = (t - t0) / (t1 - t0);
            return Color::srgb(
                c0[0] + (c1[0] - c0[0]) * k,
                c0[1] + (c1[1] - c0[1]) * k,
                c0[2] + (c1[2] - c0[2]) * k,
            );
        }
    }
    Color::srgb(1.0, 0.98, 0.9)
}

fn draw_field_lines(
    lines: Res<FieldLines>,
    view: Res<ViewSettings>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut gizmos: Gizmos<FieldLineGizmos>,
) {
    if !view.show_lines {
        return;
    }
    let eye = camera.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let span = (view.log_max - view.log_min).max(1e-3);
    let color = |log_b: f32| field_color((log_b - view.log_min) / span);

    for line in &lines.lines {
        gizmos.linestrip_gradient(line.points.iter().zip(&line.log_b).map(|(p, b)| (*p, color(*b))));

        if !view.show_arrows || line.points.len() < 3 {
            continue;
        }
        // Chevrons pointing along B at regular arc-length intervals.
        let mut next_at = 0.5 * view.arrow_spacing;
        let mut s = 0.0;
        for i in 0..line.points.len() - 1 {
            let (a, b) = (line.points[i], line.points[i + 1]);
            let seg = b - a;
            let len = seg.length();
            if len <= 0.0 {
                continue;
            }
            while s + len >= next_at {
                let p = a + seg * ((next_at - s) / len);
                let dir = seg / len;
                let to_eye = (eye - p).normalize_or_zero();
                let side = dir.cross(to_eye).normalize_or_zero();
                let size = 0.014 * (eye - p).length().clamp(4.0, 60.0);
                let c = color(line.log_b[i]);
                gizmos.line(p, p - dir * size + side * size * 0.6, c);
                gizmos.line(p, p - dir * size - side * size * 0.6, c);
                next_at += view.arrow_spacing;
            }
            s += len;
        }
    }
}

fn draw_scene_helpers(
    view: Res<ViewSettings>,
    selection: Res<Selection>,
    magnets: Query<(Entity, &MagnetBody, &Transform)>,
    mut gizmos: Gizmos,
) {
    if view.show_grid {
        let floor = magnets
            .iter()
            .map(|(_, b, t)| t.translation.y - b.shape.bounding_radius() as f32)
            .fold(-1.0f32, f32::min)
            .floor()
            - 1.0;
        gizmos.grid(
            Isometry3d::new(Vec3::new(0.0, floor, 0.0), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            UVec2::splat(60),
            Vec2::splat(1.0),
            Color::srgba(0.55, 0.6, 0.7, 0.14),
        );
    }
    for (entity, body, transform) in &magnets {
        if Some(entity) == selection.0 {
            let half = body.shape.half_extents().as_vec3();
            gizmos.cuboid(
                Transform::from_translation(transform.translation)
                    .with_rotation(transform.rotation)
                    .with_scale(2.0 * half + Vec3::splat(0.12)),
                Color::srgb(1.0, 0.85, 0.3),
            );
        }
        if view.show_magnetization {
            let dir = transform.rotation * body.direction.normalize_or_zero().as_vec3();
            let len = body.shape.bounding_radius() as f32 * 1.3;
            gizmos
                .arrow(transform.translation - dir * len, transform.translation + dir * len, Color::srgb(1.0, 0.45, 0.35))
                .with_tip_length(0.3);
        }
    }
}
