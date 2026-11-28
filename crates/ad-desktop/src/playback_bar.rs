//! Animation playback at the bottom of the viewport (owner, 2026-11-26):
//! the clip, previous / play-pause / next, stop, loop, speed, the time and a
//! timeline with the keys. Space plays and pauses too.
use super::{Actions, ViewerUi, rig_ui};
use crate::{
    accent::{self, Accent},
    imports::PreparedAsset,
    rig_playback::Controls,
};
use ad_document::{AnimationClip, AssetDocument, NodeId, TrackKind};
use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};

const BAR_MAX: f32 = 760.;
const MARGIN: f32 = 12.;
/// Previous, play, next, stop and the gaps between them.
const TRANSPORT: f32 = 3. * 36. + 46. + 3. * 6.;
/// Loop, speed and the time label.
const EXTRAS: f32 = 300.;
const GAP: f32 = 10.;
const SPEEDS: std::ops::RangeInclusive<f32> = 0.1..=3.;

#[derive(Clone, Copy)]
enum Icon {
    Previous,
    Play,
    Pause,
    Next,
    Stop,
}

impl ViewerUi {
    pub(super) fn playback_bar(
        &mut self,
        context: &egui::Context,
        asset: Option<&PreparedAsset>,
        actions: &mut Actions,
    ) {
        let scene = self.scene;
        let Some(document) = asset.map(PreparedAsset::document) else {
            self.bar_rect = Rect::NOTHING;
            return;
        };
        let clips = document.animation_clips();
        if clips.is_empty() || scene.width() < 420. || scene.height() < 200. {
            self.bar_rect = Rect::NOTHING;
            return;
        }
        // Centred in the viewport, left of the corner buttons; above them
        // when the viewport is too narrow for both on one line.
        let corner = self.corner_rect;
        let beside = if corner.is_positive() {
            corner.left() - MARGIN
        } else {
            scene.right() - MARGIN
        };
        let room = beside - scene.left() - MARGIN;
        let (width, centre) = if room >= 470. {
            let width = room.min(BAR_MAX);
            let x = scene
                .center()
                .x
                .min(beside - width / 2.)
                .max(scene.left() + MARGIN + width / 2.);
            (width, pos2(x, scene.bottom() - 10.))
        } else {
            let width = (scene.width() - 2. * MARGIN).min(BAR_MAX);
            let top = if corner.is_positive() {
                corner.top()
            } else {
                scene.bottom()
            };
            (width, pos2(scene.center().x, top - 8.))
        };
        let idle = self.edit.busy.is_none();
        let before = self.playback;
        let mut controls = self.playback;
        let mut play = Rect::NOTHING;
        let area = egui::Area::new(egui::Id::new("playback_bar"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(centre)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(14, 16, 22, 236))
                    .stroke(Stroke::new(
                        1.,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 26),
                    ))
                    .corner_radius(10.)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        let inner = width - 20.;
                        ui.set_width(inner);
                        ui.add_enabled_ui(idle, |ui| {
                            play = controls_row(ui, clips, &mut controls, inner);
                            ui.add_space(6.);
                            timeline(ui, document, &mut controls, actions);
                        });
                    });
            });
        self.bar_rect = area.response.rect;
        #[cfg(test)]
        {
            self.play_rect = play;
        }
        #[cfg(not(test))]
        let _ = play;
        if context
            .pointer_hover_pos()
            .is_some_and(|p| self.bar_rect.contains(p))
        {
            self.pointer_on_gizmo = true;
        }
        if controls != before {
            actions.playback = Some(controls);
        }
    }
}

/// One row — clip · transport · loop, speed, time — or, when the bar is
/// narrow, the clip on its own row above the rest. Nothing overlaps: the
/// time and then the loop label give way first.
fn controls_row(
    ui: &mut egui::Ui,
    clips: &[std::sync::Arc<AnimationClip>],
    controls: &mut Controls,
    width: f32,
) -> Rect {
    let durations: Vec<f32> = clips.iter().map(|clip| clip.duration).collect();
    let combo = width - TRANSPORT - EXTRAS - 2. * GAP;
    let mut play = Rect::NOTHING;
    if combo >= 130. {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            clip_combo(ui, clips, controls, combo.min(280.));
            play = transport(ui, clips, &durations, controls);
            extras(ui, controls, &durations);
        });
    } else {
        ui.horizontal(|ui| clip_combo(ui, clips, controls, width.min(320.)));
        ui.add_space(4.);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            play = transport(ui, clips, &durations, controls);
            extras(ui, controls, &durations);
        });
    }
    play
}

fn clip_combo(
    ui: &mut egui::Ui,
    clips: &[std::sync::Arc<AnimationClip>],
    controls: &mut Controls,
    width: f32,
) {
    accent::scope(ui, Accent::Existing, |ui| {
        let before = controls.clip;
        let response = egui::ComboBox::from_id_salt("bar_clip")
            .width(width)
            .truncate()
            .selected_text(controls.clip.map_or_else(
                || "Исходная поза".into(),
                |index| format!("{} · {}", index + 1, clips[index].name),
            ))
            .show_ui(ui, |ui| {
                // ANIM-02: find a clip by name (any part, any case).
                let search_id = egui::Id::new("bar_clip_search");
                let mut search = ui
                    .ctx()
                    .data(|d| d.get_temp::<String>(search_id))
                    .unwrap_or_default();
                if clips.len() > 5 {
                    accent::values(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut search)
                                .hint_text("Поиск клипа")
                                .desired_width(f32::INFINITY),
                        );
                    });
                }
                let needle = search.trim().to_lowercase();
                if needle.is_empty() {
                    ui.selectable_value(&mut controls.clip, None, "Исходная поза");
                }
                let mut shown = 0;
                egui::ScrollArea::vertical()
                    .max_height(320.)
                    .show(ui, |ui| {
                        for (index, clip) in clips.iter().enumerate() {
                            if !needle.is_empty() && !clip.name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            shown += 1;
                            ui.selectable_value(
                                &mut controls.clip,
                                Some(index),
                                format!("{} · {}", index + 1, clip.name),
                            );
                        }
                    });
                if shown == 0 {
                    ui.weak("Ничего не найдено");
                }
                ui.ctx().data_mut(|d| d.insert_temp(search_id, search));
            })
            .response
            .on_hover_text("Клип анимации · колёсико — соседний");
        // Index 0 is the authored pose, then the clips.
        let at = controls.clip.map_or(0, |clip| clip + 1);
        let next = accent::wheel_choice(ui, &response, at, clips.len() + 1);
        if next != at {
            controls.clip = next.checked_sub(1);
        }
        if controls.clip != before {
            controls.time = 0.;
            controls.playing = false;
        }
    });
}

/// Previous · play / pause · next · stop; returns the play button's rect.
fn transport(
    ui: &mut egui::Ui,
    clips: &[std::sync::Arc<AnimationClip>],
    durations: &[f32],
    controls: &mut Controls,
) -> Rect {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 6.;
        let many = clips.len() > 1;
        if icon_button(ui, Icon::Previous, Accent::Existing, 36., many)
            .on_hover_text("Предыдущая анимация")
            .clicked()
            && let Some(next) = controls.switched(clips.len(), -1)
        {
            *controls = next;
        }
        let toggled = controls.toggled(durations);
        let icon = if controls.playing {
            Icon::Pause
        } else {
            Icon::Play
        };
        let button = icon_button(ui, icon, Accent::Main, 46., toggled.is_some()).on_hover_text(
            if controls.playing {
                "Пауза · Пробел"
            } else {
                "Воспроизвести · Пробел"
            },
        );
        if button.clicked()
            && let Some(next) = toggled
        {
            *controls = next;
        }
        if icon_button(ui, Icon::Next, Accent::Existing, 36., many)
            .on_hover_text("Следующая анимация")
            .clicked()
            && let Some(next) = controls.switched(clips.len(), 1)
        {
            *controls = next;
        }
        let can_stop = controls.playing || controls.time != 0.;
        if icon_button(ui, Icon::Stop, Accent::Existing, 36., can_stop)
            .on_hover_text("Стоп: в начало клипа")
            .clicked()
        {
            controls.playing = false;
            controls.time = 0.;
        }
        button.rect
    })
    .inner
}

/// Loop · speed · time, right-aligned in what is left of the row.
fn extras(ui: &mut egui::Ui, controls: &mut Controls, durations: &[f32]) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.;
        let room = ui.available_width();
        if room >= EXTRAS {
            let duration = controls
                .clip
                .and_then(|clip| durations.get(clip))
                .copied()
                .unwrap_or(0.);
            ui.label(
                egui::RichText::new(format!("{:.3} / {:.3} с", controls.time, duration))
                    .monospace()
                    .color(Color32::from_rgb(236, 238, 244)),
            );
        }
        accent::scope(ui, Accent::Existing, |ui| {
            if room >= 130. {
                speed_field(ui, controls);
            }
            ui.toggle_value(
                &mut controls.looping,
                if room >= 190. {
                    "⟲ Повтор"
                } else {
                    "⟲"
                },
            )
            .on_hover_text("Повторять клип по кругу");
        });
    });
}

/// Preview speed 0.1–3×, in steps of 0.1 (drag or mouse wheel).
fn speed_field(ui: &mut egui::Ui, controls: &mut Controls) {
    accent::values(ui, |ui| {
        let response = ui
            .add(
                egui::DragValue::new(&mut controls.speed)
                    .range(SPEEDS)
                    .speed(0.02)
                    .fixed_decimals(1)
                    .suffix("×"),
            )
            .on_hover_text("Скорость предпросмотра · колёсико — шаг 0.1");
        accent::wheel_value(ui, &response, &mut controls.speed, 0.1, SPEEDS);
        // Typed or dragged values land on the 0.1 grid too.
        controls.speed = ((controls.speed * 10.).round() / 10.).clamp(0.1, 3.);
    });
}

/// A transport button with a drawn icon.
fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    accent: Accent,
    width: f32,
    enabled: bool,
) -> egui::Response {
    accent::scope(ui, accent, |ui| {
        let response = ui.add_enabled(
            enabled,
            egui::Button::new("").min_size(vec2(width, if width > 40. { 34. } else { 30. })),
        );
        let color = if enabled {
            ui.style().interact(&response).fg_stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        paint_icon(ui.painter(), response.rect.center(), icon, color);
        response
    })
}

fn paint_icon(painter: &egui::Painter, c: Pos2, icon: Icon, color: Color32) {
    let triangle = |tip: Pos2, size: f32, right: bool| {
        let back = if right { -size } else { size };
        egui::Shape::convex_polygon(
            vec![
                tip,
                tip + vec2(back * 1.1, -size),
                tip + vec2(back * 1.1, size),
            ],
            color,
            Stroke::NONE,
        )
    };
    let bar = |x: f32, half: f32| {
        egui::Shape::rect_filled(
            Rect::from_center_size(pos2(x, c.y), vec2(3., half * 2.)),
            1.,
            color,
        )
    };
    match icon {
        Icon::Play => {
            painter.add(triangle(c + vec2(7., 0.), 7.5, true));
        }
        Icon::Pause => {
            painter.add(bar(c.x - 3.5, 7.));
            painter.add(bar(c.x + 3.5, 7.));
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_center_size(c, vec2(11., 11.)), 1.5, color);
        }
        Icon::Previous => {
            painter.add(bar(c.x - 6., 6.));
            painter.add(triangle(c + vec2(-4., 0.), 6., false));
        }
        Icon::Next => {
            painter.add(triangle(c + vec2(4., 0.), 6., true));
            painter.add(bar(c.x + 6., 6.));
        }
    }
}

/// Seconds between labelled ticks so that labels are at least 64 px apart.
fn tick_step(range: f32, width: f32) -> f32 {
    const STEPS: [f32; 13] = [
        0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1., 2., 5., 10., 20., 30., 60.,
    ];
    let per_second = width / range.max(1e-3);
    STEPS
        .into_iter()
        .find(|step| step * per_second >= 64.)
        .unwrap_or(120.)
}

/// The key editor's joint and channel.
fn edited_channel(
    context: &egui::Context,
    document: &AssetDocument,
) -> Option<(NodeId, TrackKind)> {
    let node = context
        .data(|d| d.get_temp::<NodeId>(super::skeleton_ui::animation_node_id(document.id().0)))?;
    let kind = context
        .data(|d| d.get_temp::<TrackKind>(rig_ui::key_kind_id(document)))
        .unwrap_or(TrackKind::Rotation);
    Some((node, kind))
}

/// The time strip: ruler, every key of the clip (grey), the edited
/// channel's keys (pink diamonds), the played part and the red playhead.
/// Click or drag to scrub (snaps to a pink key within 6 px); right click
/// for keys at the playhead.
fn timeline(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: &mut Controls,
    actions: &mut Actions,
) {
    let clips = document.animation_clips();
    let clip = controls.clip.filter(|&clip| clip < clips.len());
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(1.), 34.),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect.expand(6.));
    painter.rect(
        rect,
        6.,
        Color32::from_rgb(26, 29, 38),
        Stroke::new(1., Color32::from_rgb(58, 64, 80)),
        egui::StrokeKind::Inside,
    );
    let Some(clip) = clip else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Исходная поза · выберите клип или нажмите Пробел",
            egui::FontId::proportional(12.),
            Color32::from_rgb(150, 158, 176),
        );
        return;
    };
    let active = &clips[clip];
    let bounds = rect.shrink2(vec2(10., 0.));
    let range = active.duration.max(controls.time).max(1e-3);
    let x = |time: f32| egui::lerp(bounds.x_range(), (time / range).clamp(0., 1.));
    let channel = edited_channel(ui.ctx(), document);
    let keys = channel
        .and_then(|(node, kind)| active.track(node, kind))
        .map_or(&[][..], |track| track.times());

    if let Some(pointer) = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() || response.dragged() || response.secondary_clicked())
    {
        let time = ((pointer.x - bounds.left()) / bounds.width().max(1.)).clamp(0., 1.) * range;
        let next = keys.partition_point(|&key| key < time);
        controls.time = [
            next.checked_sub(1).and_then(|i| keys.get(i)),
            keys.get(next),
        ]
        .into_iter()
        .flatten()
        .copied()
        .min_by(|a, b| {
            (x(*a) - pointer.x)
                .abs()
                .total_cmp(&(x(*b) - pointer.x).abs())
        })
        .filter(|&key| (x(key) - pointer.x).abs() <= 6.)
        .unwrap_or(time);
        controls.playing = false;
    }

    let main = Accent::Main.color();
    let edit = Accent::Edit.color();
    let playhead = x(controls.time);
    // Played part.
    painter.rect_filled(
        Rect::from_min_max(rect.min, pos2(playhead, rect.max.y)).shrink(1.),
        5.,
        main.gamma_multiply(0.22),
    );
    // Ruler: labelled ticks and four small ones between them.
    let step = tick_step(range, bounds.width());
    let ruler = Color32::from_rgb(120, 130, 150);
    let mut t = 0.;
    let mut major = 0;
    while t <= range + step * 1e-3 {
        let px = x(t);
        let labelled = major % 5 == 0;
        let len = if labelled { 8. } else { 4. };
        painter.line_segment(
            [pos2(px, rect.top() + 1.), pos2(px, rect.top() + 1. + len)],
            Stroke::new(
                1.,
                if labelled {
                    ruler
                } else {
                    ruler.gamma_multiply(0.6)
                },
            ),
        );
        if labelled && px < rect.right() - 24. {
            painter.text(
                pos2(px + 3., rect.top() + 1.),
                egui::Align2::LEFT_TOP,
                format!("{}", (t * 100.).round() / 100.),
                egui::FontId::proportional(10.),
                ruler,
            );
        }
        major += 1;
        t = major as f32 * step / 5.;
    }
    // Every key of the clip, one grey tick per pixel column.
    let mut columns = vec![false; bounds.width().max(1.) as usize + 1];
    for track in active.tracks() {
        for &time in track.times() {
            let column = (x(time) - bounds.left()) as usize;
            if let Some(slot) = columns.get_mut(column) {
                *slot = true;
            }
        }
    }
    for (column, _) in columns.iter().enumerate().filter(|(_, on)| **on) {
        let px = bounds.left() + column as f32;
        painter.line_segment(
            [pos2(px, rect.bottom() - 7.), pos2(px, rect.bottom() - 2.)],
            Stroke::new(1., Color32::from_rgb(110, 116, 132)),
        );
    }
    // The edited channel's keys.
    let y = rect.center().y + 3.;
    let mut last = f32::NEG_INFINITY;
    for &key in keys {
        let px = x(key);
        if px - last < 3. {
            continue;
        }
        last = px;
        let current = key == controls.time;
        let r = if current { 6. } else { 4.5 };
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(px, y - r),
                pos2(px + r, y),
                pos2(px, y + r),
                pos2(px - r, y),
            ],
            edit,
            Stroke::new(
                if current { 1.5 } else { 1. },
                if current {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(40, 16, 30)
                },
            ),
        ));
    }
    // Hover line.
    if let Some(p) = response.hover_pos()
        && !response.dragged()
    {
        painter.line_segment(
            [pos2(p.x, rect.top() + 2.), pos2(p.x, rect.bottom() - 2.)],
            Stroke::new(1., Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
        );
    }
    // Playhead with a knob on top.
    painter.line_segment(
        [
            pos2(playhead, rect.top() - 3.),
            pos2(playhead, rect.bottom() + 3.),
        ],
        Stroke::new(2.5, main),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(playhead - 6., rect.top() - 5.),
            pos2(playhead + 6., rect.top() - 5.),
            pos2(playhead, rect.top() + 3.),
        ],
        main,
        Stroke::new(1., Color32::from_rgb(60, 14, 16)),
    ));

    let hover = match channel {
        Some((node, kind)) => format!(
            "Розовые ромбы — ключи «{}» сустава {}; серые штрихи — все ключи клипа. Клик или протяжка — перемотка (липнет к ключу), ПКМ — ключи.",
            rig_ui::kind_name(kind),
            document.nodes().get(node.0).map_or("", |n| n.name.as_str())
        ),
        None => "Серые штрихи — ключи клипа. Клик или протяжка — перемотка, ПКМ — меню. Выберите сустав, чтобы видеть его ключи.".into(),
    };
    let response = response.on_hover_text(hover);
    response.context_menu(|ui| timeline_menu(ui, document, clip, keys, channel, controls, actions));
}

fn timeline_menu(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    keys: &[f32],
    channel: Option<(NodeId, TrackKind)>,
    controls: &mut Controls,
    actions: &mut Actions,
) {
    let time = controls.time;
    ui.label(egui::RichText::new(format!("{time:.3} с")).strong());
    if let Some((node, kind)) = channel
        && document.can_animate_node(node)
    {
        accent::scope(ui, Accent::Edit, |ui| {
            let name = document.nodes().get(node.0).map_or("", |n| n.name.as_str());
            if ui
                .button(format!("Ключ «{}» · {name}", rig_ui::kind_name(kind)))
                .on_hover_text("Поставить или обновить ключ текущей позы сустава в этом времени")
                .clicked()
            {
                actions.animation_key = rig_ui::key_at(document, clip, node, kind, time, false);
                ui.close();
            }
            if rig_ui::can_remove_key(document, clip, node, kind, time)
                && ui.button("Удалить этот ключ").clicked()
            {
                actions.animation_key = rig_ui::key_at(document, clip, node, kind, time, true);
                ui.close();
            }
        });
    } else {
        ui.weak("Выберите сустав, чтобы ставить ключи.");
    }
    ui.separator();
    let duration = document.animation_clips()[clip].duration;
    ui.horizontal(|ui| {
        for (label, frames) in [("◀ Кадр · ,", -1.), ("Кадр ▶ · .", 1.)] {
            if ui.button(label).clicked() {
                controls.time =
                    (((controls.time * 30.).round() + frames) / 30.).clamp(0., duration);
                controls.playing = false;
            }
        }
    });
    let previous = keys.partition_point(|&key| key < time).checked_sub(1);
    let next = keys.get(keys.partition_point(|&key| key <= time)).copied();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(previous.is_some(), egui::Button::new("◀ Пред. ключ"))
            .clicked()
            && let Some(i) = previous
        {
            controls.time = keys[i];
            ui.close();
        }
        if ui
            .add_enabled(next.is_some(), egui::Button::new("След. ключ ▶"))
            .clicked()
            && let Some(key) = next
        {
            controls.time = key;
            ui.close();
        }
    });
    ui.horizontal(|ui| {
        if ui.button("В начало").clicked() {
            controls.time = 0.;
            controls.playing = false;
            ui.close();
        }
        if ui.button("В конец").clicked() {
            controls.time = duration;
            controls.playing = false;
            ui.close();
        }
    });
    accent::scope(ui, Accent::Existing, |ui| {
        ui.checkbox(&mut controls.looping, "Повтор");
        ui.horizontal(|ui| {
            ui.label("Скорость");
            speed_field(ui, controls);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::tick_step;

    #[test]
    fn ruler_labels_stay_apart() {
        assert_eq!(tick_step(2., 640.), 0.2);
        assert_eq!(tick_step(30., 640.), 5.);
        assert_eq!(tick_step(0.1, 640.), 0.01);
        assert_eq!(tick_step(10_000., 640.), 120.);
    }
}
