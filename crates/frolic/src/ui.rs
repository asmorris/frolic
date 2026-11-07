//! The side panel: scene files and sharing, adding / editing magnets, field-line settings.

use crate::camera::OrbitCamera;
use crate::field::{field_color, FieldLines, TraceSettings, ViewSettings};
use crate::interaction::{duplicate, EditSettings};
use crate::magnets::{euler_yxz, MagnetBody, NameCounter, Selection};
use crate::persistence::{capture, CurrentScene, PendingLoad, SceneCommand, Toasts};
use crate::presets::{builtin_scenes, MagnetKind, DEFAULT_SCENE};
use crate::scene_file::SceneFile;
use crate::theme::{self, row, section, segmented, slider, split, subhead, ERROR, FAINT, MUTED, ROW_H, TEXT, WARN};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::egui::{self, vec2, Align, Color32, Layout, Margin, RichText};
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
    themed: bool,
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
    if !ui_state.themed {
        theme::apply(&ctx);
        ui_state.themed = true;
    }
    let now = file.time.elapsed();

    let frame = egui::Frame::side_top_panel(&ctx.style()).inner_margin(Margin { left: 18, right: 18, top: 16, bottom: 12 });
    egui::SidePanel::left("frolic_panel")
        .resizable(true)
        .default_width(340.0)
        .width_range(300.0..=520.0)
        .frame(frame)
        .show(&ctx, |ui| {
            // Header.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("Frolic").size(20.0).strong().color(TEXT));
                ui.label(RichText::new("Magnetic field simulator").size(12.0).color(MUTED));
            });
            ui.add_space(10.0);

            // Status line, pinned to the bottom.
            egui::TopBottomPanel::bottom("frolic_status")
                .frame(egui::Frame::new().inner_margin(Margin { top: 8, ..Margin::ZERO }))
                .show_separator_line(true)
                .show_inside(ui, |ui| {
                    let status = if lines.busy { "updating…" } else if lines.interactive { "preview" } else { "ready" };
                    ui.label(
                        RichText::new(format!(
                            "{} lines · {} points · {:.1} ms · {status}",
                            lines.lines.len(),
                            lines.point_count,
                            lines.compute_time.as_secs_f64() * 1e3
                        ))
                        .size(11.0)
                        .color(FAINT),
                    );
                });

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // Leave a gutter for the scrollbar.
                ui.set_max_width(ui.available_width() - 8.0);

                // ------------------------------------------------------------ Scene
                section(ui, "Scene", true, |ui| {
                    let mut name = file.current.name.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("Untitled scene")
                            .font(egui::FontId::proportional(15.0))
                            .margin(vec2(8.0, 6.0))
                            .desired_width(f32::INFINITY),
                    );
                    if name != file.current.name {
                        file.current.name = name;
                    }
                    let mut desc = file.current.description.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut desc)
                            .hint_text("Add a description…")
                            .margin(vec2(8.0, 6.0))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if desc != file.current.description {
                        file.current.description = desc;
                    }

                    let location = match &file.current.path {
                        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        None => "Not saved yet".into(),
                    };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if file.current.is_dirty() {
                            ui.label(RichText::new("●").size(9.0).color(WARN));
                            ui.label(RichText::new(format!("Unsaved changes · {location}")).size(11.5).color(MUTED));
                        } else {
                            ui.label(RichText::new(location).size(11.5).color(FAINT));
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.spacing_mut().button_padding.x = 6.0;
                        let w = split(ui, 4);
                        let b = |ui: &mut egui::Ui, label: &str, hint: &str| {
                            let r = ui.add_sized([w, ROW_H], egui::Button::new(label));
                            if hint.is_empty() { r } else { r.on_hover_text(hint) }
                        };
                        if b(ui, "New", "").clicked() {
                            file.commands.write(SceneCommand::Load { scene: SceneFile::new("Untitled"), path: None, force: false });
                        }
                        if b(ui, "Open…", "⌘O / Ctrl+O, or drop a file on the window").clicked() {
                            file.commands.write(SceneCommand::Open);
                        }
                        if b(ui, "Save", "⌘S / Ctrl+S").clicked() {
                            file.commands.write(SceneCommand::Save);
                        }
                        if b(ui, "Save as…", "⇧⌘S / Shift+Ctrl+S").clicked() {
                            file.commands.write(SceneCommand::SaveAs);
                        }
                    });

                    subhead(ui, "Examples");
                    ui.horizontal(|ui| {
                        let examples = builtin_scenes();
                        let idx = ui_state.example.get_or_insert(DEFAULT_SCENE);
                        let open_w = 64.0;
                        egui::ComboBox::from_id_salt("examples")
                            .width(ui.available_width() - open_w - ui.spacing().item_spacing.x)
                            .height(320.0)
                            .selected_text(examples[*idx].name.as_str())
                            .show_ui(ui, |ui| {
                                for (i, s) in examples.iter().enumerate() {
                                    ui.selectable_value(idx, i, s.name.as_str()).on_hover_text(&s.description);
                                }
                            });
                        if ui.add_sized([open_w, ROW_H], egui::Button::new("Open")).clicked() {
                            file.commands.write(SceneCommand::Load { scene: examples[*idx].clone(), path: None, force: false });
                        }
                    });
                });

                // ------------------------------------------------------------ Magnets
                section(ui, "Magnets", true, |ui| {
                    let mut list: Vec<(Entity, String, Shape, f64)> =
                        magnets.iter().map(|(e, b, _)| (e, b.name.clone(), b.shape, b.remanence)).collect();
                    list.sort_by(|a, b| a.1.cmp(&b.1));
                    if list.is_empty() {
                        ui.label(RichText::new("No magnets yet. Add one below.").color(MUTED));
                    }
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (e, name, shape, _) in list {
                            let selected = selection.0 == Some(e);
                            if theme::list_item(ui, selected, &name, shape.name()).clicked() {
                                selection.0 = if selected { None } else { Some(e) };
                            }
                        }
                    });

                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        ui.label(RichText::new("Add").color(MUTED));
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

                // ------------------------------------------------------------ Inspector
                if let Some(entity) = selection.0 {
                    if let Ok((_, mut body, mut transform)) = magnets.get_mut(entity) {
                        section(ui, "Selected magnet", true, |ui| {
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

                // ------------------------------------------------------------ Field lines
                section(ui, "Field lines", true, |ui| {
                    let mut v = view.clone();
                    let mut tr = trace.clone();
                    ui.checkbox(&mut v.show_lines, "Show field lines");
                    ui.add_enabled_ui(v.show_lines, |ui| {
                        row(ui, "Lines", |ui| slider(ui, egui::Slider::new(&mut tr.line_count, 10..=800).logarithmic(true)));
                        row(ui, "Extent", |ui| slider(ui, egui::Slider::new(&mut tr.extent, 1.5..=8.0).max_decimals(1)))
                            .on_hover_text("How far lines are followed, relative to the size of the scene.");
                        row(ui, "Accuracy", |ui| slider(ui, egui::Slider::new(&mut tr.quality, 0.5..=3.0).max_decimals(1)))
                            .on_hover_text("Smaller integration steps (slower, smoother lines).");
                        row(ui, "Width", |ui| {
                            slider(ui, egui::Slider::new(&mut v.line_width, 0.5..=5.0).max_decimals(1).suffix(" px"))
                        });
                        row(ui, "Arrows", |ui| {
                            ui.checkbox(&mut v.show_arrows, "");
                            ui.add_enabled(
                                v.show_arrows,
                                egui::DragValue::new(&mut v.arrow_spacing)
                                    .range(0.5..=10.0)
                                    .speed(0.05)
                                    .prefix("every ")
                                    .suffix(" cm"),
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Colour").color(MUTED));
                            ui.label(RichText::new("field strength |B|").size(11.5).color(FAINT));
                        });
                        legend(ui, &mut v);
                    });
                    view.set_if_neq(v);
                    trace.set_if_neq(tr);
                });

                // ------------------------------------------------------------ Display
                section(ui, "Display", false, |ui| {
                    let mut v = view.clone();
                    ui.checkbox(&mut v.show_grid, "Floor grid").on_hover_text("1 cm cells");
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

                // ------------------------------------------------------------ Share
                section(ui, "Share", false, |ui| {
                    ui.label(
                        RichText::new("A share code is the whole scene as one line of text: paste it into a chat, an issue, anywhere.")
                            .color(MUTED),
                    );
                    if ui.add_sized([ui.available_width(), ROW_H], egui::Button::new("Copy share code")).clicked() {
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
                            .margin(vec2(8.0, 6.0))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    let enabled = !ui_state.paste.trim().is_empty();
                    if ui.add_enabled(enabled, egui::Button::new("Open pasted scene").min_size(vec2(0.0, ROW_H))).clicked() {
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
                            .size(11.5)
                            .color(FAINT),
                    );
                });

                // ------------------------------------------------------------ Help
                section(ui, "Controls", false, |ui| {
                    egui::Grid::new("controls").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                        for (k, d) in [
                            ("Drag", "slide a magnet horizontally"),
                            ("Shift + drag", "move it up / down"),
                            ("⌥ / Alt + drag", "rotate it"),
                            ("Drag empty space", "orbit the camera"),
                            ("Right-drag", "pan"),
                            ("Scroll / pinch", "zoom"),
                            ("Q  E", "turn selection 15°"),
                            ("F", "focus camera on selection"),
                            ("⌘D / Ctrl+D", "duplicate selection"),
                            ("Delete", "remove selection"),
                        ] {
                            theme::kbd(ui, k);
                            ui.label(RichText::new(d).color(MUTED));
                            ui.end_row();
                        }
                    });
                });
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
                ui.label(RichText::new(format!("Discard them and open “{incoming}”?")).color(MUTED));
                ui.add_space(8.0);
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
                    egui::Frame::popup(ui.style()).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
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
    // Tighter padding so three value boxes fit on one row.
    ui.spacing_mut().button_padding.x = 6.0;
    row(ui, "Name", |ui| ui.add(egui::TextEdit::singleline(&mut b.name).margin(vec2(8.0, 4.0)).desired_width(f32::INFINITY)));

    // Shape.
    row(ui, "Shape", |ui| {
        let r = b.shape.bounding_radius() / 2.0;
        let shapes = [
            ("Block", Shape::Cuboid { size: DVec3::splat(r * 1.6) }),
            ("Cylinder", Shape::Cylinder { radius: r, height: 2.0 * r }),
            ("Sphere", Shape::Sphere { radius: r }),
        ];
        let same = |s: &Shape| std::mem::discriminant(&b.shape) == std::mem::discriminant(s);
        let opts: Vec<(&str, bool)> = shapes.iter().map(|(l, s)| (*l, same(s))).collect();
        if let Some(i) = segmented(ui, &opts) {
            if !same(&shapes[i].1) {
                b.shape = shapes[i].1;
            }
        }
    });
    match &mut b.shape {
        Shape::Cuboid { size } => {
            row(ui, "Size", |ui| {
                let w = split(ui, 3);
                for (axis, v) in ["x ", "y ", "z "].into_iter().zip([&mut size.x, &mut size.y, &mut size.z]) {
                    ui.add_sized([w, ROW_H], len(v).prefix(axis));
                }
            });
        }
        Shape::Cylinder { radius, height } => {
            row(ui, "Radius", |ui| ui.add(len(radius).suffix(" cm")));
            row(ui, "Height", |ui| ui.add(len(height).suffix(" cm")));
        }
        Shape::Sphere { radius } => {
            row(ui, "Radius", |ui| ui.add(len(radius).suffix(" cm")));
        }
    }

    // Placement.
    subhead(ui, "Placement");
    row(ui, "Position", |ui| {
        let w = split(ui, 3);
        for (axis, v) in ["x ", "y ", "z "].into_iter().zip(t.translation.as_mut()) {
            ui.add_sized([w, ROW_H], egui::DragValue::new(v).speed(0.02).prefix(axis).max_decimals(2));
        }
    });
    let (yaw, pitch, roll) = euler_yxz(t.rotation);
    let mut deg = [yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees()];
    let mut changed = false;
    for (label, v) in ["Yaw", "Pitch", "Roll"].into_iter().zip(deg.iter_mut()) {
        changed |= row(ui, label, |ui| slider(ui, egui::Slider::new(v, -180.0..=180.0).suffix("°").max_decimals(1)))
            .changed();
    }
    if changed {
        t.rotation = Quat::from_euler(EulerRot::YXZ, deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians());
    }
    row(ui, "", |ui| {
        if ui.small_button("Reset rotation").clicked() {
            t.rotation = Quat::IDENTITY;
        }
        if ui.small_button("Drop to floor").on_hover_text("Set y = 0").clicked() {
            t.translation.y = 0.0;
        }
    });

    // Magnetization.
    subhead(ui, "Magnetization");
    row(ui, "Strength", |ui| slider(ui, egui::Slider::new(&mut b.remanence, 0.0..=1.5).suffix(" T").max_decimals(2)))
        .on_hover_text("Remanence Br");
    row(ui, "Material", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (label, hint, br) in [
                ("Ferrite", "Ferrite, 0.4 T", 0.4),
                ("AlNiCo", "AlNiCo, 1.2 T", 1.2),
                ("N35", "NdFeB N35, 1.2 T", 1.2),
                ("N42", "NdFeB N42, 1.3 T", 1.3),
                ("N52", "NdFeB N52, 1.45 T", 1.45),
            ] {
                if ui.small_button(label).on_hover_text(hint).clicked() {
                    b.remanence = br;
                }
            }
        });
    });
    row(ui, "North", |ui| {
        let dirs = [
            ("+X", DVec3::X),
            ("−X", DVec3::NEG_X),
            ("+Y", DVec3::Y),
            ("−Y", DVec3::NEG_Y),
            ("+Z", DVec3::Z),
            ("−Z", DVec3::NEG_Z),
        ];
        let opts: Vec<(&str, bool)> =
            dirs.iter().map(|(l, d)| (*l, (b.direction.normalize_or_zero() - *d).length() < 1e-6)).collect();
        if let Some(i) = segmented(ui, &opts) {
            b.direction = dirs[i].1;
        }
    });
    row(ui, "Custom", |ui| {
        let mut d = b.direction;
        let mut c = false;
        let w = split(ui, 3);
        for (axis, v) in ["x ", "y ", "z "].into_iter().zip([&mut d.x, &mut d.y, &mut d.z]) {
            c |= ui
                .add_sized([w, ROW_H], egui::DragValue::new(v).speed(0.01).range(-1.0..=1.0).prefix(axis).max_decimals(2))
                .changed();
        }
        if c && d.length() > 1e-3 {
            b.direction = d;
        }
    });
    row(ui, "", |ui| {
        if ui.small_button("Flip N / S").clicked() {
            b.direction = -b.direction;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("south").size(11.5).color(MUTED));
            ui.label(RichText::new("■").color(SOUTH));
            ui.label(RichText::new("north").size(11.5).color(MUTED));
            ui.label(RichText::new("■").color(NORTH));
        });
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let w = split(ui, 3);
        *focus = ui.add_sized([w, ROW_H], egui::Button::new("Focus")).on_hover_text("Focus the camera (F)").clicked();
        *dup = ui.add_sized([w, ROW_H], egui::Button::new("Duplicate")).clicked();
        *delete = ui.add_sized([w, ROW_H], egui::Button::new(RichText::new("Delete").color(ERROR))).clicked();
    });
}

fn len(v: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(0.05..=50.0).speed(0.02).max_decimals(2)
}

fn legend(ui: &mut egui::Ui, v: &mut ViewSettings) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), egui::Sense::hover());
    let n = 48;
    for i in 0..n {
        let x0 = rect.left() + rect.width() * i as f32 / n as f32;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32;
        let c = field_color((i as f32 + 0.5) / n as f32).to_srgba();
        let c = Color32::from_rgb((c.red * 255.0) as u8, (c.green * 255.0) as u8, (c.blue * 255.0) as u8);
        let radius = egui::CornerRadius {
            nw: if i == 0 { 4 } else { 0 },
            sw: if i == 0 { 4 } else { 0 },
            ne: if i == n - 1 { 4 } else { 0 },
            se: if i == n - 1 { 4 } else { 0 },
        };
        let x1 = if i == n - 1 { x1 } else { x1 + 0.5 };
        ui.painter()
            .rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())), radius, c);
    }
    ui.horizontal(|ui| {
        let min = v.log_max - 0.1;
        ui.add(
            egui::DragValue::new(&mut v.log_min)
                .range(-6.0..=min)
                .speed(0.02)
                .custom_formatter(|x, _| format_tesla(10f64.powf(x))),
        )
        .on_hover_text("Drag to change the weakest field shown");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let max = v.log_min + 0.1;
            ui.add(
                egui::DragValue::new(&mut v.log_max)
                    .range(max..=1.0)
                    .speed(0.02)
                    .custom_formatter(|x, _| format_tesla(10f64.powf(x))),
            )
            .on_hover_text("Drag to change the strongest field shown");
        });
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
