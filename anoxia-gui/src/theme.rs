//! Fonts, colours and small reusable widgets.

use anoxia_core::safety::{Priority, RiskLevel};
use eframe::egui::{self, Color32, RichText, Ui};

/// Accent colour (petrol blue).
pub const ACCENT: Color32 = Color32::from_rgb(21, 101, 192);
/// Warning colour (amber, dark enough for text on white).
pub const WARNING: Color32 = Color32::from_rgb(180, 83, 9);
/// Error colour.
pub const DANGER: Color32 = Color32::from_rgb(183, 28, 28);
/// Life-threatening colour.
pub const VITAL: Color32 = Color32::from_rgb(74, 20, 140);
/// Secondary text.
pub const MUTED: Color32 = Color32::from_rgb(96, 110, 120);

/// Install fonts (Segoe UI on Windows for full Unicode coverage, e.g. "O₂")
/// and the visual style.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [("segoe", r"C:\Windows\Fonts\segoeui.ttf"), ("segoe_symbol", r"C:\Windows\Fonts\seguisym.ttf")];
    let mut inserted = Vec::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.to_owned(), egui::FontData::from_owned(bytes));
            inserted.push(name);
        }
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        for (i, name) in inserted.iter().enumerate() {
            if *name == "segoe" {
                family.insert(i, (*name).to_owned());
            } else {
                family.push((*name).to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
    apply_visuals(ctx, false);
}

/// Light or dark visuals with larger default text.
pub fn apply_visuals(ctx: &egui::Context, dark: bool) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::WHITE);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.interact_size.y = 24.0;
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_style(style);
}

/// Colour of a risk level.
#[must_use]
pub fn risk_color(level: RiskLevel) -> Color32 {
    let (r, g, b) = level.color();
    Color32::from_rgb(r, g, b)
}

/// Colour of a recommendation priority.
#[must_use]
pub fn priority_color(p: Priority) -> Color32 {
    match p {
        Priority::Critical => DANGER,
        Priority::High => WARNING,
        Priority::Medium => ACCENT,
        Priority::Info => MUTED,
    }
}

/// Section frame.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(10.0)).rounding(6.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

/// Section title.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).strong().size(16.0));
}

/// Small grey hint text.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(MUTED));
}

/// A colour that stays readable as text on the current background: the
/// accent colours are dark enough on white, lightened on a dark theme.
#[must_use]
pub fn readable(color: Color32, dark: bool) -> Color32 {
    if dark {
        let k = 0.4_f32;
        let mix = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * k).round().clamp(0.0, 255.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Color32::from_rgb(mix(color.r()) as u8, mix(color.g()) as u8, mix(color.b()) as u8)
    } else {
        color
    }
}

/// Coloured message box with a solid bar on the left.
pub fn callout(ui: &mut Ui, color: Color32, text: &str) {
    callout_titled(ui, color, "", text);
}

/// Message box with a short bold title ("Danger", "Attention") above the text.
pub fn callout_titled(ui: &mut Ui, color: Color32, title: &str, text: &str) {
    let dark = ui.visuals().dark_mode;
    let body = if dark { Color32::from_rgb(232, 234, 236) } else { Color32::from_rgb(40, 48, 56) };
    let response = egui::Frame::none()
        .fill(color.gamma_multiply(if dark { 0.22 } else { 0.08 }))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
        .inner_margin(egui::Margin { left: 16.0, right: 10.0, top: 7.0, bottom: 7.0 })
        .rounding(6.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).strong().size(12.5).color(readable(color, dark)));
            }
            ui.label(RichText::new(text).color(body));
        });
    let rect = response.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 5.0, rect.max.y)),
        egui::Rounding { nw: 6.0, sw: 6.0, ne: 0.0, se: 0.0 },
        color,
    );
}

/// Application icon (generated: O₂ drop on a blue disc).
#[must_use]
pub fn app_icon() -> egui::IconData {
    let size = 64u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let c = 31.5_f32;
    for y in 0..size {
        for x in 0..size {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let r = (dx * dx + dy * dy).sqrt();
            // Drop shape: circle at the bottom, cone at the top
            let in_drop = {
                let (cx, cy, rad) = (c, c + 8.0, 16.0);
                #[allow(clippy::cast_precision_loss)]
                let (px, py) = (x as f32, y as f32);
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let cone = py < cy && py > 6.0 && (px - cx).abs() < (py - 6.0) / (cy - 6.0) * rad;
                d < rad || cone
            };
            let px = if r > 31.0 {
                [0, 0, 0, 0]
            } else if in_drop {
                [255, 255, 255, 255]
            } else {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let shade = (180.0 - r * 2.0) as u8;
                [21, shade.saturating_sub(60), 192, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    egui::IconData { rgba, width: size, height: size }
}
