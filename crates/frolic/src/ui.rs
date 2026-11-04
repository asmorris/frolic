//! The side panel: scene files and sharing, adding / editing magnets, field-line settings.

use crate::camera::OrbitCamera;
use crate::field::{field_color, FieldLines, TraceSettings, ViewSettings};
use crate::interaction::{duplicate, EditSettings};
use crate::magnets::{euler_yxz, MagnetBody, NameCounter, Selection};
use crate::persistence::{capture, CurrentScene, PendingLoad, SceneCommand, Toasts};
use crate::presets::{builtin_scenes, MagnetKind, DEFAULT_SCENE};
use crate::scene_file::SceneFile;
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText};
use bevy_egui::{EguiContextPass, EguiContexts};
use frolic_core::Shape;
use std::time::Duration;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>().add_systems(EguiContextPass, side_panel);
    }
}

/// Whether egui is using the pointer / keyboard this frame (3D input backs off).
#[derive(Resource, Default)]
pub struct UiState {
    pub pointer_over: bool,
    pub keyboard: bool,
    example: Option<usize>,
    paste: String,
}

/// Scene-file state used by the panel.
#[derive(SystemParam)]
struct FileUi<'w> {
    current: ResMut<'w, CurrentScene>,
    pending: ResMut<'w, PendingLoad>,
    toasts: ResMut<'w, Toasts>,
    commands: EventWriter<'w, SceneCommand>,
    time: Res<'w, Time>,
}

const NORTH: Color32 = Color32::from_rgb(220, 60, 50);
const SOUTH: Color32 = Color32::from_rgb(60, 110, 225);
const ERROR: Color32 = Color32::from_rgb(240, 110, 100);

#[allow(clippy::too_many_arguments)]
fn side_panel(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
    mut selection: ResMut<Selection>,
    mut names: ResMut<NameCounter>,
    mut trace: ResMut<TraceSettings>,
    mut view: ResMut<ViewSettings>,
    mut edit: ResMut<EditSettings>,
    lines: Res<FieldLines>,
    mut file: FileUi,
    mut magnets: Query<(Entity, &mut MagnetBody, &mut Transform)>,
    mut cameras: Query<&mut OrbitCamera>,
) {
    let ctx = contexts.ctx_mut().clone();
    let now = file.time.elapsed();

    egui::SidePanel::left("frolic_panel").resizable(true).default_width(330.0).show(&ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Frolic").strong());
            ui.label(RichText::new("magnetic field simulator").weak());
        });
        ui.separator();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ---------------------------------------------------------------- Scene
            egui::CollapsingHeader::new("Scene").default_open(true).show(ui, |ui| {
                egui::Grid::new("scene_meta").num_columns(2).show(ui, |ui| {
                    ui.label("Name");
                    let mut name = file.current.name.clone();
                    ui.add(egui::TextEdit::singleline(&mut name).hint_text("Untitled").desired_width(f32::INFINITY));
                    if name != file.current.name {
                        file.current.name = name;
                    }
                    ui.end_row();
                    ui.label("Notes");
                    let mut desc = file.current.description.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut desc)
                            .hint_text("Optional description")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if desc != file.current.description {
                        file.current.description = desc;
                    }
                    ui.end_row();
                });
                let location = match &file.current.path {
                    Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    None => "not saved yet".into(),
                };
                let status = if file.current.is_dirty() { format!("{location} · unsaved changes") } else { location };
                ui.label(RichText::new(status).weak().small());
                ui.horizontal(|ui| {
                    if ui.button("New").clicked() {
                        file.commands.write(SceneCommand::Load { scene: SceneFile::new("Untitled"), path: None, force: false });
                    }
                    if ui.button("Open…").on_hover_text("⌘O / Ctrl+O, or drop a file on the window").clicked() {
                        file.commands.write(SceneCommand::Open);
                    }
                    if ui.button("Save").on_hover_text("⌘S / Ctrl+S").clicked() {
                        file.commands.write(SceneCommand::Save);
                    }
                    if ui.button("Save as…").on_hover_text("⇧⌘S / Shift+Ctrl+S").clicked() {
                        file.commands.write(SceneCommand::SaveAs);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let examples = builtin_scenes();
                    let idx = ui_state.example.get_or_insert(DEFAULT_SCENE);
                    egui::ComboBox::from_id_salt("examples")
                        .width(210.0)
                        .selected_text(examples[*idx].name.as_str())
                        .show_ui(ui, |ui| {
                            for (i, s) in examples.iter().enumerate() {
                                ui.selectable_value(idx, i, s.name.as_str()).on_hover_text(&s.description);
                            }
                        });
                    if ui.button("Open example").clicked() {
                        file.commands.write(SceneCommand::Load { scene: examples[*idx].clone(), path: None, force: false });
                    }
                });
                ui.label("Add a magnet:");
                ui.horizontal_wrapped(|ui| {
                    for kind in MagnetKind::ALL {
                        if ui.button(format!("+ {}", kind.label())).clicked() {
                            let body = kind.body(&mut names);
                            let focus = cameras.single().map(|c| c.focus).unwrap_or_default();
                            let others: Vec<(Vec3, f32)> = magnets
                                .iter()
                                .map(|(_, b, t)| (t.translation, b.shape.bounding_radius() as f32))
                                .collect();
                            let pos = free_spot(focus, body.shape.bounding_radius() as f32, &others);
                            let e = commands
                                .spawn((body, Transform::from_translation(pos).with_rotation(kind.default_rotation())))
                                .id();
                            selection.0 = Some(e);
                        }
                    }
                });
            });

            // ---------------------------------------------------------------- Magnets
            egui::CollapsingHeader::new("Magnets").default_open(true).show(ui, |ui| {
                let mut list: Vec<(Entity, String, Shape)> =
                    magnets.iter().map(|(e, b, _)| (e, b.name.clone(), b.shape)).collect();
                list.sort_by(|a, b| a.1.cmp(&b.1));
                if list.is_empty() {
                    ui.label(RichText::new("No magnets: add one above.").weak());
                }
                for (e, name, shape) in list {
                    let selected = selection.0 == Some(e);
                    if ui.selectable_label(selected, format!("{name}  ·  {}", shape.name())).clicked() {
                        selection.0 = if selected { None } else { Some(e) };
                    }
                }
            });

            // ---------------------------------------------------------------- Inspector
            if let Some(entity) = selection.0 {
                if let Ok((_, mut body, mut transform)) = magnets.get_mut(entity) {
                    egui::CollapsingHeader::new("Selected magnet").default_open(true).show(ui, |ui| {
                        let mut b = body.clone();
                        let mut t = *transform;
                        let mut delete = false;
                        let mut dup = false;
                        let mut focus = false;
                        inspector(ui, &mut b, &mut t, &mut delete, &mut dup, &mut focus);
                        if b != *body {
                            *body = b;
                        }
                        if t != *transform {
                            *transform = t;
                        }
                        if focus {
                            if let Ok(mut cam) = cameras.single_mut() {
                                cam.focus = transform.translation;
                            }
                        }
                        if dup {
                            selection.0 = Some(duplicate(&mut commands, &mut names, &body, &transform));
                        }
                        if delete {
                            commands.entity(entity).despawn();
                            selection.0 = None;
                        }
                    });
                }
            }

            // ---------------------------------------------------------------- Share
            egui::CollapsingHeader::new("Share").default_open(false).show(ui, |ui| {
                ui.label(
                    RichText::new("A share code is the whole scene as one line of text: paste it into a chat, an issue, anywhere.")
                        .weak(),
                );
                if ui.button("Copy share code").clicked() {
                    let scene = capture(&file.current, magnets.iter().map(|(_, b, t)| (b, t)), cameras.single().ok());
                    let code = scene.to_share_code();
                    let len = code.len();
                    ctx.copy_text(code);
                    file.toasts.info(now, format!("Share code copied to the clipboard ({len} characters)"));
                }
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::multiline(&mut ui_state.paste)
                        .hint_text("Paste a share code (FROLIC1:…) or scene JSON")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if ui.add_enabled(!ui_state.paste.trim().is_empty(), egui::Button::new("Open pasted scene")).clicked() {
                    match SceneFile::parse_any(&ui_state.paste) {
                        Ok(scene) => {
                            file.commands.write(SceneCommand::Load { scene, path: None, force: false });
                            ui_state.paste.clear();
                        }
                        Err(e) => file.toasts.error(now, format!("Couldn't open that: {e}")),
                    }
                }
                ui.label(
                    RichText::new("Scene files (.frolic.json) can be shared directly too. To add one to the public gallery, open a pull request adding it to the repo's scenes/ folder.")
                        .weak()
                        .small(),
                );
            });

            // ---------------------------------------------------------------- Field lines
            egui::CollapsingHeader::new("Field lines").default_open(true).show(ui, |ui| {
                let mut v = view.clone();
                let mut tr = trace.clone();
                ui.checkbox(&mut v.show_lines, "Show field lines");
                ui.add(egui::Slider::new(&mut tr.line_count, 10..=800).text("lines").logarithmic(true));
                ui.add(egui::Slider::new(&mut tr.extent, 1.5..=8.0).text("extent"))
                    .on_hover_text("How far lines are followed, relative to the size of the scene.");
                ui.add(egui::Slider::new(&mut tr.quality, 0.5..=3.0).text("accuracy"))
                    .on_hover_text("Smaller integration steps (slower, smoother lines).");
                ui.add(egui::Slider::new(&mut v.line_width, 0.5..=5.0).text("width (px)"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut v.show_arrows, "Direction arrows");
                    ui.add_enabled(
                        v.show_arrows,
                        egui::DragValue::new(&mut v.arrow_spacing).range(0.5..=10.0).speed(0.05).prefix("every ").suffix(" cm"),
                    );
                });
                ui.add_space(4.0);
                ui.label("Colour = field strength |B|");
                legend(ui, &mut v);
                view.set_if_neq(v);
                trace.set_if_neq(tr);
            });

            // ---------------------------------------------------------------- Display
            egui::CollapsingHeader::new("Display").default_open(false).show(ui, |ui| {
                let mut v = view.clone();
                ui.checkbox(&mut v.show_grid, "Floor grid (1 cm cells)");
                ui.checkbox(&mut v.show_magnetization, "Magnetization arrows");
                view.set_if_neq(v);
                let mut e = edit.clone();
                ui.horizontal(|ui| {
                    let mut on = e.snap > 0.0;
                    ui.checkbox(&mut on, "Snap moves to");
                    let mut s = if on { e.snap } else { 0.5 };
                    ui.add_enabled(on, egui::DragValue::new(&mut s).range(0.05..=5.0).speed(0.01).suffix(" cm"));
                    e.snap = if on { s } else { 0.0 };
                });
                edit.set_if_neq(e);
            });

            // ---------------------------------------------------------------- Help
            egui::CollapsingHeader::new("Controls").default_open(false).show(ui, |ui| {
                for (k, d) in [
                    ("Drag magnet", "slide horizontally"),
                    ("Shift + drag magnet", "move up / down"),
                    ("Alt/⌥ + drag magnet", "rotate"),
                    ("Drag empty space", "orbit camera"),
                    ("Right-drag / Shift-drag", "pan"),
                    ("Scroll / pinch", "zoom"),
                    ("Q / E", "turn selection 15°"),
                    ("F", "focus camera on selection"),
                    ("⌘D / Ctrl+D", "duplicate selection"),
                    ("Delete / Backspace", "remove selection"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).strong());
                        ui.label(RichText::new(d).weak());
                    });
                }
            });

            ui.add_space(8.0);
            ui.separator();
            let status = if lines.busy { "updating…" } else if lines.interactive { "preview" } else { "ready" };
            ui.label(
                RichText::new(format!(
                    "{} lines · {} points · {:.1} ms · {status}",
                    lines.lines.len(),
                    lines.point_count,
                    lines.compute_time.as_secs_f64() * 1e3
                ))
                .weak()
                .small(),
            );
        });
    });

    // Confirm before replacing unsaved work.
    if let Some((scene, _)) = &file.pending.0 {
        let incoming = if scene.name.is_empty() { "Untitled".to_string() } else { scene.name.clone() };
        let current = if file.current.name.is_empty() { "Untitled".to_string() } else { file.current.name.clone() };
        let mut decision = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(&ctx, |ui| {
                ui.label(format!("“{current}” has unsaved changes."));
                ui.label(format!("Discard them and open “{incoming}”?"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Discard and open").color(ERROR)).clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => {
                if let Some((scene, path)) = file.pending.0.take() {
                    file.commands.write(SceneCommand::Load { scene, path, force: true });
                }
            }
            Some(false) => file.pending.0 = None,
            None => {}
        }
    }

    // Transient notification.
    if let Some((msg, is_error, at)) = &file.toasts.message {
        let ttl = if *is_error { Duration::from_secs(8) } else { Duration::from_secs(4) };
        if now.saturating_sub(*at) < ttl {
            egui::Area::new(egui::Id::new("toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
                .interactable(false)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        let text = RichText::new(msg.as_str());
                        ui.label(if *is_error { text.color(ERROR) } else { text });
                    });
                });
        }
    }

    ui_state.pointer_over = ctx.is_pointer_over_area() || ctx.wants_pointer_input();
    ui_state.keyboard = ctx.wants_keyboard_input();
}

fn inspector(ui: &mut egui::Ui, b: &mut MagnetBody, t: &mut Transform, delete: &mut bool, dup: &mut bool, focus: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut b.name);
    });

    // Shape.
    ui.horizontal(|ui| {
        ui.label("Shape");
        let r = b.shape.bounding_radius() / 2.0;
        let (cube, cyl, sph) = (
            Shape::Cuboid { size: DVec3::splat(r * 1.6) },
            Shape::Cylinder { radius: r, height: 2.0 * r },
            Shape::Sphere { radius: r },
        );
        for (label, s) in [("Block", cube), ("Cylinder", cyl), ("Sphere", sph)] {
            if ui.selectable_label(std::mem::discriminant(&b.shape) == std::mem::discriminant(&s), label).clicked()
                && std::mem::discriminant(&b.shape) != std::mem::discriminant(&s)
            {
                b.shape = s;
            }
        }
    });
    egui::Grid::new("dims").num_columns(2).show(ui, |ui| match &mut b.shape {
        Shape::Cuboid { size } => {
            ui.label("Size");
            ui.horizontal(|ui| {
                ui.add(len(&mut size.x).prefix("x "));
                ui.add(len(&mut size.y).prefix("y "));
                ui.add(len(&mut size.z).prefix("z "));
            });
            ui.end_row();
        }
        Shape::Cylinder { radius, height } => {
            ui.label("Radius");
            ui.add(len(radius));
            ui.end_row();
            ui.label("Height");
            ui.add(len(height));
            ui.end_row();
        }
        Shape::Sphere { radius } => {
            ui.label("Radius");
            ui.add(len(radius));
            ui.end_row();
        }
    });

    // Placement.
    ui.add_space(4.0);
    egui::Grid::new("pose").num_columns(2).show(ui, |ui| {
        ui.label("Position");
        ui.horizontal(|ui| {
            for (axis, v) in ["x ", "y ", "z "].into_iter().zip(t.translation.as_mut()) {
                ui.add(egui::DragValue::new(v).speed(0.02).prefix(axis).max_decimals(2));
            }
        });
        ui.end_row();

        let (yaw, pitch, roll) = euler_yxz(t.rotation);
        let mut deg = [yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees()];
        ui.label("Rotation");
        let changed = ui
            .horizontal(|ui| {
                let mut c = false;
                for (label, v) in ["yaw ", "pitch ", "roll "].into_iter().zip(deg.iter_mut()) {
                    c |= ui.add(egui::DragValue::new(v).speed(0.5).prefix(label).suffix("°").max_decimals(1)).changed();
                }
                c
            })
            .inner;
        if changed {
            t.rotation = Quat::from_euler(EulerRot::YXZ, deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians());
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button("Reset rotation").clicked() {
            t.rotation = Quat::IDENTITY;
        }
        if ui.button("Drop to y = 0").clicked() {
            t.translation.y = 0.0;
        }
    });

    // Magnetization.
    ui.add_space(4.0);
    ui.label(RichText::new("Magnetization").strong());
    ui.add(egui::Slider::new(&mut b.remanence, 0.0..=1.5).text("Br (T)").max_decimals(2));
    ui.horizontal_wrapped(|ui| {
        for (label, br) in [("Ferrite", 0.4), ("AlNiCo", 1.2), ("NdFeB N35", 1.2), ("N42", 1.3), ("N52", 1.45)] {
            if ui.small_button(label).clicked() {
                b.remanence = br;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("N points along local");
        for (label, d) in [
            ("+X", DVec3::X),
            ("−X", DVec3::NEG_X),
            ("+Y", DVec3::Y),
            ("−Y", DVec3::NEG_Y),
            ("+Z", DVec3::Z),
            ("−Z", DVec3::NEG_Z),
        ] {
            let on = (b.direction.normalize_or_zero() - d).length() < 1e-6;
            if ui.selectable_label(on, label).clicked() {
                b.direction = d;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("custom");
        let mut d = b.direction;
        let mut c = false;
        for (axis, v) in ["x ", "y ", "z "].into_iter().zip([&mut d.x, &mut d.y, &mut d.z]) {
            c |= ui.add(egui::DragValue::new(v).speed(0.01).range(-1.0..=1.0).prefix(axis).max_decimals(2)).changed();
        }
        if c && d.length() > 1e-3 {
            b.direction = d;
        }
        if ui.button("Flip N/S").clicked() {
            b.direction = -b.direction;
        }
    });
    ui.horizontal(|ui| {
        ui.colored_label(NORTH, "■ north");
        ui.colored_label(SOUTH, "■ south");
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        *focus = ui.button("Focus camera").clicked();
        *dup = ui.button("Duplicate").clicked();
        *delete = ui.button(RichText::new("Delete").color(ERROR)).clicked();
    });
}

fn len(v: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(0.05..=50.0).speed(0.02).suffix(" cm")
}

fn legend(ui: &mut egui::Ui, v: &mut ViewSettings) {
    let width = ui.available_width().min(300.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::hover());
    let n = 64;
    for i in 0..n {
        let x0 = rect.left() + rect.width() * i as f32 / n as f32;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32;
        let c = field_color((i as f32 + 0.5) / n as f32).to_srgba();
        let c = Color32::from_rgb((c.red * 255.0) as u8, (c.green * 255.0) as u8, (c.blue * 255.0) as u8);
        ui.painter()
            .rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1 + 0.5, rect.bottom())), 0.0, c);
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut v.log_min)
                .range(-6.0..=v.log_max - 0.1)
                .speed(0.02)
                .custom_formatter(|x, _| format_tesla(10f64.powf(x))),
        );
        ui.label("…");
        ui.add(
            egui::DragValue::new(&mut v.log_max)
                .range(v.log_min + 0.1..=1.0)
                .speed(0.02)
                .custom_formatter(|x, _| format_tesla(10f64.powf(x))),
        );
    });
}

fn format_tesla(b: f64) -> String {
    if b >= 1.0 {
        format!("{b:.2} T")
    } else if b >= 0.1 {
        format!("{:.0} mT", b * 1e3)
    } else if b >= 1e-3 {
        format!("{:.1} mT", b * 1e3)
    } else {
        format!("{:.1} µT", b * 1e6)
    }
}

/// Find a spot near `focus` that doesn't overlap existing magnets.
fn free_spot(focus: Vec3, radius: f32, others: &[(Vec3, f32)]) -> Vec3 {
    let base = Vec3::new(focus.x, 0.0, focus.z);
    for ring in 0..12 {
        let n = if ring == 0 { 1 } else { 8 * ring };
        for k in 0..n {
            let angle = std::f32::consts::TAU * k as f32 / n as f32;
            let p = base + Vec3::new(angle.cos(), 0.0, angle.sin()) * ring as f32 * (radius + 0.75);
            if others.iter().all(|(c, r)| (p - *c).length() > r + radius + 0.3) {
                return p;
            }
        }
    }
    base
}
