//! Colours by meaning (owner, 2026-11-26): yellow for new animations, blue
//! for existing ones, pink for editing, red for the general accent; pastel
//! bold numbers for the Info values.
use egui::{Color32, Stroke, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    /// Creating an animation: a new clip, a copy.
    New,
    /// Existing animations: choose, play, rename, delete, retime.
    Existing,
    /// Editing: keys, channels, the joint gizmo.
    Edit,
    /// The general accent: play / pause, the playhead.
    Main,
}

const DARK: Color32 = Color32::from_rgb(18, 20, 26);

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl Accent {
    pub fn color(self) -> Color32 {
        match self {
            Self::New => Color32::from_rgb(242, 196, 58),
            Self::Existing => Color32::from_rgb(84, 152, 245),
            Self::Edit => Color32::from_rgb(238, 110, 182),
            Self::Main => Color32::from_rgb(232, 66, 66),
        }
    }
    /// Text on a dark background in this colour.
    pub fn text(self) -> Color32 {
        mix(self.color(), Color32::WHITE, 0.45)
    }
    /// Buttons, combo boxes, toggles and drag values take this colour:
    /// a tinted fill, a coloured frame, brighter on hover and press.
    pub fn apply(self, visuals: &mut Visuals) {
        let c = self.color();
        let w = &mut visuals.widgets;
        for (state, fill, frame, text) in [
            (&mut w.inactive, 0.74, 0.35, 0.45),
            (&mut w.hovered, 0.56, 0.0, 0.7),
            (&mut w.active, 0.42, 0.0, 0.85),
            (&mut w.open, 0.56, 0.0, 0.7),
        ] {
            state.weak_bg_fill = mix(c, DARK, fill);
            state.bg_fill = mix(c, DARK, fill);
            state.bg_stroke = Stroke::new(1., mix(c, DARK, frame));
            state.fg_stroke = Stroke::new(1.5, mix(c, Color32::WHITE, text));
            state.corner_radius = 5.into();
        }
        visuals.selection.bg_fill = mix(c, DARK, 0.25);
        visuals.selection.stroke = Stroke::new(1., Color32::WHITE);
    }
}

/// Runs `add` with every widget inside coloured by `accent`.
pub fn scope<R>(ui: &mut egui::Ui, accent: Accent, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        accent.apply(ui.visuals_mut());
        add(ui)
    })
    .inner
}

/// A section heading in the accent's colour.
pub fn heading(ui: &mut egui::Ui, accent: Accent, text: &str) {
    ui.label(egui::RichText::new(text).strong().color(accent.text()));
}

/// Pastel colours for the numbers in Info.
pub mod pastel {
    use egui::Color32;
    pub const SKY: Color32 = Color32::from_rgb(158, 204, 255);
    pub const MINT: Color32 = Color32::from_rgb(150, 228, 180);
    pub const PEACH: Color32 = Color32::from_rgb(255, 196, 152);
    pub const LAVENDER: Color32 = Color32::from_rgb(200, 180, 255);
    pub const LEMON: Color32 = Color32::from_rgb(242, 228, 140);
    pub const ROSE: Color32 = Color32::from_rgb(250, 172, 196);
}

/// A number drawn a little heavier (twice, a fraction of a pixel apart:
/// the bundled font has no bold face) in `color`.
pub fn bold(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let font = egui::FontId::new(body.size + 0.5, body.family);
    let galley = ui.painter().layout_no_wrap(text.into(), font, color);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + egui::vec2(1., 0.), egui::Sense::hover());
    ui.painter().galley(rect.min, galley.clone(), color);
    ui.painter()
        .galley(rect.min + egui::vec2(0.65, 0.), galley, color);
    response
}

/// «label value unit» on one line; only the value is bold and coloured.
pub fn stat(
    ui: &mut egui::Ui,
    label: &str,
    value: impl Into<String>,
    unit: &str,
    color: Color32,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.;
        ui.label(label);
        bold(ui, value, color);
        if !unit.is_empty() {
            ui.label(unit);
        }
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_differ_and_tint_widgets() {
        let all = [Accent::New, Accent::Existing, Accent::Edit, Accent::Main];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.color(), b.color());
            }
            let mut visuals = Visuals::dark();
            a.apply(&mut visuals);
            assert_ne!(
                visuals.widgets.inactive.weak_bg_fill,
                Visuals::dark().widgets.inactive.weak_bg_fill
            );
            assert_eq!(mix(a.color(), DARK, 0.), a.color());
        }
    }
}
