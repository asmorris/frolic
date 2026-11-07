//! Look and feel of the side panel: palette, spacing, and a few small layout helpers
//! (collapsible sections, labelled property rows, segmented controls) that keep the
//! panel calm and aligned.

use bevy_egui::egui::{
    self, collapsing_header::CollapsingState, vec2, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout,
    Margin, RichText, Sense, Stroke, TextStyle,
};

pub const PANEL: Color32 = Color32::from_rgb(19, 20, 23);
pub const SURFACE: Color32 = Color32::from_rgb(33, 35, 40);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(43, 46, 53);
pub const BORDER: Color32 = Color32::from_rgb(37, 39, 45);
pub const TEXT: Color32 = Color32::from_rgb(224, 226, 232);
pub const MUTED: Color32 = Color32::from_rgb(136, 141, 154);
pub const FAINT: Color32 = Color32::from_rgb(96, 101, 114);
pub const ACCENT: Color32 = Color32::from_rgb(94, 142, 255);
pub const ACCENT_BG: Color32 = Color32::from_rgb(36, 52, 92);
pub const WARN: Color32 = Color32::from_rgb(232, 178, 92);
pub const ERROR: Color32 = Color32::from_rgb(240, 110, 100);

/// Width of the label column in property rows.
pub const LABEL_W: f32 = 84.0;
/// Height of a standard control.
pub const ROW_H: f32 = 26.0;

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = vec2(8.0, 8.0);
    s.button_padding = vec2(10.0, 4.0);
    s.interact_size = vec2(36.0, ROW_H);
    s.indent = 14.0;
    s.slider_rail_height = 4.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 8.0;
    s.menu_margin = Margin::same(6);
    s.window_margin = Margin::same(16);
    s.scroll.bar_width = 6.0;
    s.scroll.floating_allocated_width = 0.0;

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(26, 28, 32);
    v.extreme_bg_color = Color32::from_rgb(27, 28, 32);
    v.faint_bg_color = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow.color = Color32::from_black_alpha(90);
    v.popup_shadow.color = Color32::from_black_alpha(90);
    v.selection.bg_fill = ACCENT_BG;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;
    v.collapsing_header_frame = false;
    v.interact_cursor = Some(CursorIcon::PointingHand);

    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.corner_radius = r;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);

    w.inactive.corner_radius = r;
    w.inactive.bg_fill = SURFACE;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.5, Color32::from_rgb(200, 204, 214));
    w.inactive.expansion = 0.0;

    w.hovered.corner_radius = r;
    w.hovered.bg_fill = SURFACE_HOVER;
    w.hovered.weak_bg_fill = SURFACE_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(58, 62, 72));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.expansion = 0.0;

    w.active.corner_radius = r;
    w.active.bg_fill = Color32::from_rgb(50, 54, 63);
    w.active.weak_bg_fill = Color32::from_rgb(50, 54, 63);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.expansion = 0.0;

    w.open.corner_radius = r;
    w.open.bg_fill = SURFACE_HOVER;
    w.open.weak_bg_fill = SURFACE_HOVER;
    w.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(58, 62, 72));
    w.open.fg_stroke = Stroke::new(1.5, TEXT);

    style.visuals = v;
    ctx.set_style(style);
}

/// A collapsible section: a quiet uppercase title with a chevron, separated from the
/// previous section by a hairline.
pub fn section(ui: &mut egui::Ui, title: &str, default_open: bool, add: impl FnOnce(&mut egui::Ui)) {
    ui.add(egui::Separator::default().spacing(4.0));
    let id = ui.make_persistent_id(("section", title));
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    if resp.clicked() {
        state.toggle(ui);
    }
    let color = if resp.hovered() { TEXT } else { MUTED };
    let galley = egui::WidgetText::from(RichText::new(title.to_uppercase()).size(11.0).strong().extra_letter_spacing(0.9))
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Small);
    ui.painter().galley(rect.left_center() - vec2(0.0, galley.size().y / 2.0), galley, color);

    // Chevron: points right when closed, down when open.
    let openness = state.openness(ui.ctx());
    let c = rect.right_center() - vec2(6.0, 0.0);
    let angle = openness * std::f32::consts::FRAC_PI_2;
    let rot = egui::emath::Rot2::from_angle(angle);
    let pts = [vec2(-2.0, -4.0), vec2(2.0, 0.0), vec2(-2.0, 4.0)].map(|p| c + rot * p);
    ui.painter().line(pts.to_vec(), Stroke::new(1.5, color));

    state.show_body_unindented(ui, |ui| {
        add(ui);
        ui.add_space(10.0);
    });
}

/// A small heading inside a section, for grouping related rows.
pub fn subhead(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).size(12.5).strong().color(TEXT));
}

/// A property row: a muted label in a fixed-width column, then the controls.
pub fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(LABEL_W, ROW_H), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(vec2(LABEL_W, ROW_H));
            ui.add(egui::Label::new(RichText::new(label).color(MUTED)).truncate());
        });
        add(ui)
    })
    .inner
}

/// Width for each of `n` equal controls filling the rest of the line.
pub fn split(ui: &egui::Ui, n: usize) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).floor()
}

/// A slider that fills the rest of the row, with its value box at the right.
pub fn slider(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.spacing_mut().slider_width = (ui.available_width() - 72.0).max(60.0);
    ui.add(slider)
}

/// A row of mutually exclusive options; returns the index clicked, if any.
pub fn segmented(ui: &mut egui::Ui, options: &[(&str, bool)]) -> Option<usize> {
    let mut clicked = None;
    egui::Frame::new().fill(SURFACE).corner_radius(7).inner_margin(Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let n = options.len().max(1) as f32;
        let w = ((ui.available_width() - 2.0 * (n - 1.0)) / n).floor();
        ui.horizontal(|ui| {
            for (i, (label, on)) in options.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H - 4.0), Sense::click());
                let fill = if *on {
                    Color32::from_rgb(58, 62, 72)
                } else if resp.hovered() {
                    SURFACE_HOVER
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, 5, fill);
                let color = if *on || resp.hovered() { TEXT } else { MUTED };
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, *label, FontId::proportional(12.5), color);
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
        });
    });
    clicked
}

/// A full-width list row with a primary label on the left and a muted detail on the right.
pub fn list_item(ui: &mut egui::Ui, selected: bool, label: &str, detail: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let fill = if selected {
        ACCENT_BG
    } else if resp.hovered() {
        SURFACE
    } else {
        Color32::TRANSPARENT
    };
    let p = ui.painter();
    p.rect_filled(rect, 6, fill);
    p.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(13.5), TEXT);
    p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, detail, FontId::proportional(12.0), MUTED);
    resp
}

/// A keyboard key, drawn as a small rounded chip.
pub fn kbd(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(SURFACE).corner_radius(4).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.label(RichText::new(text).size(11.5).color(TEXT));
    });
}
