//! Colours by meaning (owner, 2026-11-26): yellow for new animations, blue
//! for existing ones, pink for editing, red for the general accent. Only
//! the words take the colour; numbers stay white (owner, the same day).
//! Also: section cards, pastel bold numbers for Info and mouse-wheel steps
//! for fields and choices.
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
/// Neutral button background and frame.
const FILL: Color32 = Color32::from_rgb(40, 44, 54);
const FRAME: Color32 = Color32::from_rgb(64, 70, 86);
/// Numbers and text inside fields.
pub const VALUE: Color32 = Color32::from_rgb(246, 247, 250);

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
    /// Words on a dark background in this colour.
    pub fn text(self) -> Color32 {
        mix(self.color(), Color32::WHITE, 0.3)
    }
    /// Buttons, combo boxes and toggles inside: the words in the colour on
    /// a neutral background, a tinted background on hover and press, a
    /// tinted fill when selected. Main (play) keeps a filled red button.
    pub fn apply(self, visuals: &mut Visuals) {
        let c = self.color();
        let filled = self == Self::Main;
        let w = &mut visuals.widgets;
        for (state, tint, frame, text) in [
            (&mut w.inactive, 0.0, 0.0, 0.3),
            (&mut w.hovered, 0.2, 0.75, 0.55),
            (&mut w.active, 0.32, 1.0, 0.75),
            (&mut w.open, 0.2, 0.75, 0.55),
        ] {
            let base = if filled { mix(c, DARK, 0.45) } else { FILL };
            state.weak_bg_fill = mix(base, c, tint);
            state.bg_fill = state.weak_bg_fill;
            state.bg_stroke = Stroke::new(1., mix(FRAME, c, frame));
            state.fg_stroke = Stroke::new(
                1.5,
                if filled {
                    Color32::WHITE
                } else {
                    mix(c, Color32::WHITE, text)
                },
            );
            state.corner_radius = 5.into();
        }
        visuals.selection.bg_fill = mix(c, DARK, 0.5);
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

/// Fields with numbers or names: white text on the neutral background,
/// whatever accent is around them.
pub fn values<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        for state in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            state.fg_stroke = Stroke::new(1.5, VALUE);
            state.weak_bg_fill = Color32::from_rgb(22, 25, 32);
            state.bg_fill = state.weak_bg_fill;
        }
        v.widgets.inactive.bg_stroke = Stroke::new(1., FRAME);
        v.override_text_color = Some(VALUE);
        add(ui)
    })
    .inner
}

/// A section: a framed card with a coloured title.
pub fn card<R>(
    ui: &mut egui::Ui,
    accent: Accent,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(Color32::from_rgb(30, 33, 41))
        .stroke(Stroke::new(1., Color32::from_rgb(50, 55, 68)))
        .corner_radius(8.)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, accent, title);
            ui.add_space(4.);
            scope(ui, accent, add)
        })
        .inner
}

/// A section heading in the accent's colour.
pub fn heading(ui: &mut egui::Ui, accent: Accent, text: &str) {
    ui.label(egui::RichText::new(text).strong().color(accent.text()));
}

/// The label column of a form grid.
pub fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(Color32::from_rgb(178, 186, 202)));
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

/// Mouse wheel over `response`: +1 per notch up, -1 down (0 otherwise).
/// While the pointer is over it the scroll is taken, so the panel or the
/// camera does not move as well.
pub fn wheel_steps(ui: &egui::Ui, response: &egui::Response) -> i32 {
    if !response.enabled() || !response.contains_pointer() {
        return 0;
    }
    ui.input_mut(|i| {
        let dy: f32 = i
            .raw
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                _ => None,
            })
            .sum();
        i.smooth_scroll_delta = egui::Vec2::ZERO;
        i.raw
            .events
            .retain(|event| !matches!(event, egui::Event::MouseWheel { .. }));
        if dy > 0. {
            1
        } else if dy < 0. {
            -1
        } else {
            0
        }
    })
}

/// A number field the wheel also changes by `step` within `range`.
pub fn wheel_value(
    ui: &egui::Ui,
    response: &egui::Response,
    value: &mut f32,
    step: f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let steps = wheel_steps(ui, response);
    if steps == 0 {
        return false;
    }
    // Exact steps from the current value; rounding to 1e-4 keeps 0.1 steps
    // from drifting (1.1, not 1.0999999).
    let next = ((*value + steps as f32 * step) * 1e4).round() / 1e4;
    let next = next.clamp(*range.start(), *range.end());
    let changed = next != *value;
    *value = next;
    changed
}

/// A choice among `count` items the wheel moves through (down: next).
pub fn wheel_choice(ui: &egui::Ui, response: &egui::Response, index: usize, count: usize) -> usize {
    match wheel_steps(ui, response) {
        0 => index,
        s if s < 0 => (index + 1).min(count.saturating_sub(1)),
        _ => index.saturating_sub(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_differ_and_colour_only_the_words() {
        let all = [Accent::New, Accent::Existing, Accent::Edit, Accent::Main];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.color(), b.color());
            }
            let mut visuals = Visuals::dark();
            a.apply(&mut visuals);
            let idle = visuals.widgets.inactive;
            if *a == Accent::Main {
                assert_ne!(idle.weak_bg_fill, FILL, "play stays a filled button");
            } else {
                assert_eq!(idle.weak_bg_fill, FILL, "neutral background");
                assert_ne!(idle.fg_stroke.color, VALUE, "coloured words");
            }
            assert_eq!(mix(a.color(), DARK, 0.), a.color());
        }
    }

    #[test]
    fn the_wheel_steps_values_and_choices() {
        let context = egui::Context::default();
        let mut results = Vec::new();
        for (frame, dy) in [(0, 0.), (1, 50.), (2, -50.)] {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300., 200.),
                )),
                ..Default::default()
            };
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(20., 20.)));
            if dy != 0. {
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., dy),
                    modifiers: Default::default(),
                    phase: egui::TouchPhase::Move,
                });
            }
            let mut output = context.run_ui(input, |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    let mut value = 1.0f32;
                    let response = ui.add(egui::DragValue::new(&mut value));
                    let changed = wheel_value(ui, &response, &mut value, 0.1, 0.1..=3.);
                    let choice = wheel_choice(ui, &response, 1, 3);
                    results.push((frame, changed, value, choice));
                });
            });
            output.textures_delta.clear();
        }
        let up = results.iter().find(|r| r.0 == 1).unwrap();
        assert!(up.1 && (up.2 - 1.1).abs() < 1e-5, "{up:?}");
        let down = results.iter().find(|r| r.0 == 2).unwrap();
        assert!(down.1 && (down.2 - 0.9).abs() < 1e-5, "{down:?}");
    }
}
