use crate::{imports::PreparedAsset, rig_playback::Controls, viewer_ui::Actions};
use ad_document::{
    AnimationClip, AnimationKey, AssetDocument, ChannelValue, ClipEdit, NodeId, TrackKind,
};
use std::sync::Arc;

pub(super) fn show(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    mut controls: Controls,
    actions: &mut Actions,
) {
    let Some(asset) = asset else {
        ui.label("Откройте GLB со скином или анимацией.");
        return;
    };
    let document = asset.document();
    let clips = document.animation_clips();
    let before = controls;
    if document.is_read_only() {
        new_clip_button(ui, document, actions);
    }
    if !clips.is_empty() {
        clip_controls(ui, clips, &mut controls);
        if let Some(clip) = controls.clip {
            ui.add_enabled_ui(!controls.playing, |ui| {
                clip_properties(ui, document, clip, actions);
                key_editor(ui, document, clip, &mut controls, actions);
            });
        }
        ui.separator();
    }
    let node_id = egui::Id::new(("animation_node", document.id().0));
    let Some(rig) = document.rig() else {
        ui.label("У этой модели нет скина.");
        actions.playback = (controls != before).then_some(controls);
        return;
    };

    ui.label("Скин и веса");
    let selector_id = ui.id().with(("skin", document.id().0));
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(selector_id))
        .unwrap_or(0)
        .min(rig.skins.len() - 1);
    egui::ComboBox::from_id_salt(selector_id)
        .selected_text(format!("{} · {}", selected + 1, rig.skins[selected].name))
        .show_ui(ui, |ui| {
            for (index, skin) in rig.skins.iter().enumerate() {
                ui.selectable_value(
                    &mut selected,
                    index,
                    format!("{} · {}", index + 1, skin.name),
                );
            }
        });
    ui.ctx()
        .data_mut(|data| data.insert_temp(selector_id, selected));

    let skin = &rig.skins[selected];
    let report = &rig.reports[selected];
    ui.label(format!("Суставов: {}", skin.joints.len()));
    if let Some(root) = skin.skeleton {
        ui.label(format!("Корень скелета: {}", document.nodes()[root.0].name));
    }
    ui.label(format!(
        "Суставов без весов: {}",
        report.unused_joints.len()
    ));
    if report.normalized_vertices > 0 {
        ui.label(format!(
            "Вершин с нормализованными весами: {}",
            report.normalized_vertices
        ));
    }
    ui.separator();

    let row_height = ui.text_style_height(&egui::TextStyle::Body);
    egui::ScrollArea::vertical()
        .id_salt((selector_id, selected))
        .auto_shrink([false, false])
        .show_rows(ui, row_height, skin.joints.len(), |ui, rows| {
            for slot in rows {
                let node = skin.joints[slot];
                let status = if report.unused_joints.binary_search(&slot).is_ok() {
                    " · без веса"
                } else {
                    ""
                };
                let label = format!("{} · {}{}", slot, document.nodes()[node.0].name, status);
                let selected = ui.ctx().data(|data| data.get_temp::<NodeId>(node_id));
                let response = ui
                    .add_enabled(
                        controls.clip.is_some()
                            && !controls.playing
                            && document.can_animate_node(node),
                        egui::Button::selectable(selected == Some(node), label).truncate(),
                    )
                    .on_hover_text(format!("Узел glTF №{}", node.0));
                if response.clicked() {
                    ui.ctx().data_mut(|data| data.insert_temp(node_id, node));
                    actions.animation_preview = Some(None);
                }
            }
        });
    actions.playback = (controls != before).then_some(controls);
}

fn clip_controls(ui: &mut egui::Ui, clips: &[Arc<AnimationClip>], controls: &mut Controls) {
    let before = *controls;
    egui::ComboBox::from_id_salt("animation_clip")
        .selected_text(controls.clip.map_or_else(
            || "Исходная поза".into(),
            |index| format!("{} · {}", index + 1, clips[index].name),
        ))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut controls.clip, None, "Исходная поза");
            for (index, clip) in clips.iter().enumerate() {
                ui.selectable_value(
                    &mut controls.clip,
                    Some(index),
                    format!("{} · {}", index + 1, clip.name),
                );
            }
        });
    if controls.clip != before.clip {
        controls.time = 0.;
        controls.playing = false;
    }
    if let Some(index) = controls.clip {
        let duration = clips[index].duration;
        ui.horizontal(|ui| {
            let label = if controls.playing {
                "Пауза"
            } else {
                "Воспроизвести"
            };
            if ui
                .add_enabled(duration > 0., egui::Button::new(label))
                .clicked()
            {
                if !controls.playing && controls.time >= duration {
                    controls.time = 0.;
                }
                controls.playing = !controls.playing;
            }
            if ui
                .add_enabled(
                    controls.playing || controls.time != 0.,
                    egui::Button::new("Стоп"),
                )
                .clicked()
            {
                controls.playing = false;
                controls.time = 0.;
            }
            ui.checkbox(&mut controls.looping, "Повтор");
        });
        egui::ComboBox::from_id_salt("animation_preview_speed")
            .selected_text(format!("{}×", controls.speed))
            .show_ui(ui, |ui| {
                for speed in [0.25, 0.5, 1., 1.5, 2.] {
                    ui.selectable_value(&mut controls.speed, speed, format!("{speed}×"));
                }
            })
            .response
            .on_hover_text("Скорость предпросмотра");
        if ui
            .add(
                egui::Slider::new(&mut controls.time, 0. ..=duration)
                    .fixed_decimals(3)
                    .clamping(egui::SliderClamping::Edits)
                    .suffix(" с"),
            )
            .changed()
        {
            controls.playing = false;
        }
    }
}

fn new_clip_button(ui: &mut egui::Ui, document: &AssetDocument, actions: &mut Actions) {
    let node_id = egui::Id::new(("animation_node", document.id().0));
    let node = ui
        .ctx()
        .data(|data| data.get_temp::<NodeId>(node_id))
        .filter(|&node| document.can_animate_node(node))
        .or_else(|| {
            document
                .rig()
                .into_iter()
                .flat_map(|rig| &rig.skins)
                .flat_map(|skin| skin.joints.iter().copied())
                .find(|&node| document.can_animate_node(node))
        })
        .or_else(|| {
            (0..document.nodes().len())
                .map(NodeId)
                .find(|&node| document.can_animate_node(node))
        });
    if ui
        .add_enabled(node.is_some(), egui::Button::new("Новый клип"))
        .on_hover_text("Создать клип с ключом поворота в исходной позе")
        .clicked()
        && let Some(node) = node
    {
        let clips = document.animation_clips();
        let mut number = clips.len() + 1;
        let name = loop {
            let name = format!("Клип {number}");
            if !clips.iter().any(|clip| clip.name == name) {
                break name;
            }
            number += 1;
        };
        actions.clip_edit = Some(ClipEdit::Create { name, node });
    }
}

#[derive(Clone)]
struct ClipDraft {
    clip_id: u64,
    revision: u64,
    name: String,
    duration: f32,
}

fn clip_properties(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    actions: &mut Actions,
) {
    let active = &document.animation_clips()[clip];
    let id = egui::Id::new(("clip_properties", document.id().0));
    egui::CollapsingHeader::new("Свойства клипа")
        .id_salt(id)
        .show(ui, |ui| {
            let mut draft = ui
                .ctx()
                .data(|data| data.get_temp::<ClipDraft>(id))
                .filter(|draft| {
                    draft.clip_id == active.id() && draft.revision == document.revision()
                })
                .unwrap_or_else(|| ClipDraft {
                    clip_id: active.id(),
                    revision: document.revision(),
                    name: active.name.clone(),
                    duration: active.duration,
                });
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .char_limit(120)
                    .hint_text("Название клипа"),
            );
            let name = draft.name.trim();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !name.is_empty() && name != active.name,
                        egui::Button::new("Переименовать"),
                    )
                    .clicked()
                {
                    actions.clip_edit = Some(ClipEdit::Rename {
                        clip,
                        name: name.to_owned(),
                    });
                }
                if ui.button("Дублировать").clicked() {
                    let name = if name.is_empty() || name == active.name {
                        let stem: String = active.name.trim().chars().take(112).collect();
                        format!("{stem} (копия)").trim().to_owned()
                    } else {
                        name.to_owned()
                    };
                    actions.clip_edit = Some(ClipEdit::Duplicate { clip, name });
                }
                if ui.button("Удалить клип").clicked() {
                    actions.clip_edit = Some(ClipEdit::Delete { clip });
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Длительность");
                ui.add(
                    egui::DragValue::new(&mut draft.duration)
                        .range(0. ..=f32::MAX)
                        .speed(0.01)
                        .max_decimals(6)
                        .suffix(" с"),
                );
                let can_retime = active.duration > 0.
                    && draft.duration > 0.
                    && draft.duration != active.duration;
                if ui
                    .add_enabled(can_retime, egui::Button::new("Применить"))
                    .clicked()
                {
                    actions.clip_edit = Some(ClipEdit::Retime {
                        clip,
                        duration: draft.duration,
                    });
                }
            });
            ui.ctx().data_mut(|data| data.insert_temp(id, draft));
        });
}

#[derive(Clone, Copy, PartialEq)]
struct KeyPosition {
    revision: u64,
    clip: usize,
    node: NodeId,
    kind: TrackKind,
    time: f32,
}

#[derive(Clone, Copy)]
struct Draft {
    position: KeyPosition,
    axes: [f32; 3],
}

fn key_editor(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    controls: &mut Controls,
    actions: &mut Actions,
) {
    let active = &document.animation_clips()[clip];
    let node_id = egui::Id::new(("animation_node", document.id().0));
    let kind_id = node_id.with("kind");
    let draft_id = node_id.with("draft");
    let mut node = ui
        .ctx()
        .data(|data| data.get_temp::<NodeId>(node_id))
        .unwrap_or_else(|| active.tracks()[0].node());
    let mut kind = ui
        .ctx()
        .data(|data| data.get_temp::<TrackKind>(kind_id))
        .unwrap_or_else(|| active.tracks()[0].kind());
    let previous_selection = (node, kind);
    let node_label = |index: usize| format!("{} · {}", index, document.nodes()[index].name);
    egui::ComboBox::from_id_salt(node_id)
        .selected_text(node_label(node.0))
        .truncate()
        .show_ui(ui, |ui| {
            let row = ui.text_style_height(&egui::TextStyle::Body) + 6.;
            egui::ScrollArea::vertical().max_height(240.).show_rows(
                ui,
                row,
                document.nodes().len(),
                |ui, rows| {
                    for index in rows {
                        let choice = NodeId(index);
                        if ui
                            .add_enabled(
                                document.can_animate_node(choice),
                                egui::Button::selectable(node == choice, node_label(index))
                                    .truncate(),
                            )
                            .clicked()
                        {
                            node = choice;
                            ui.close();
                        }
                    }
                },
            );
        });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut kind, TrackKind::Translation, "Сдвиг");
        ui.selectable_value(&mut kind, TrackKind::Rotation, "Поворот °");
        ui.selectable_value(&mut kind, TrackKind::Scale, "Масштаб");
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(node_id, node);
        data.insert_temp(kind_id, kind);
    });
    if previous_selection != (node, kind) {
        actions.animation_preview = Some(None);
    }

    let times = active
        .track(node, kind)
        .map_or(&[][..], |track| track.times());
    key_timeline(ui, times, active.duration, controls);
    ui.horizontal(|ui| {
        let previous = times
            .partition_point(|&time| time < controls.time)
            .checked_sub(1);
        let next = times
            .get(times.partition_point(|&time| time <= controls.time))
            .copied();
        if ui
            .add_enabled(previous.is_some(), egui::Button::new("◀"))
            .on_hover_text("Предыдущий ключ")
            .clicked()
        {
            controls.time = times[previous.expect("enabled when previous exists")];
        }
        if ui
            .add_enabled(next.is_some(), egui::Button::new("▶"))
            .on_hover_text("Следующий ключ")
            .clicked()
        {
            controls.time = next.expect("enabled when next exists");
        }
        ui.add(
            egui::DragValue::new(&mut controls.time)
                .speed(0.01)
                .range(0. ..=f32::MAX)
                .max_decimals(6)
                .suffix(" с"),
        )
        .on_hover_text("Время ключа. Можно ввести время за пределами клипа.");
        ui.weak(format!("{} ключ.", times.len()));
    });

    let position = KeyPosition {
        revision: document.revision(),
        clip,
        node,
        kind,
        time: controls.time,
    };
    let Ok(value) = document.animation_value(clip, node, kind, position.time) else {
        return;
    };
    let initial = if kind == TrackKind::Rotation {
        let (x, y, z) = glam::Quat::from_array(value).to_euler(glam::EulerRot::XYZ);
        [x, y, z].map(f32::to_degrees)
    } else {
        [value[0], value[1], value[2]]
    };
    let mut draft = ui
        .ctx()
        .data(|data| data.get_temp::<Draft>(draft_id))
        .filter(|draft| draft.position == position)
        .unwrap_or(Draft {
            position,
            axes: initial,
        });
    let mut changed = false;
    ui.horizontal(|ui| {
        for (axis, label) in draft.axes.iter_mut().zip(["X ", "Y ", "Z "]) {
            changed |= ui
                .add(
                    egui::DragValue::new(axis)
                        .prefix(label)
                        .speed(0.02)
                        .max_decimals(4),
                )
                .changed();
        }
    });
    let value = if draft.axes == initial {
        value
    } else if kind == TrackKind::Rotation {
        let [x, y, z] = draft.axes.map(f32::to_radians);
        glam::Quat::from_euler(glam::EulerRot::XYZ, x, y, z).to_array()
    } else {
        let [x, y, z] = draft.axes;
        [x, y, z, 0.]
    };
    let channel = ChannelValue { node, kind, value };
    if changed {
        actions.animation_preview = Some((draft.axes != initial).then_some(channel));
    }
    ui.horizontal(|ui| {
        let exists =
            times.get(times.partition_point(|&time| time < position.time)) == Some(&position.time);
        if ui
            .button(if exists {
                "Обновить ключ"
            } else {
                "Добавить ключ"
            })
            .clicked()
        {
            actions.animation_key = Some(AnimationKey {
                clip,
                time: position.time,
                channel,
                remove: false,
            });
        }
        let can_remove = exists && (times.len() > 1 || active.tracks().len() > 1);
        if ui
            .add_enabled(can_remove, egui::Button::new("Удалить"))
            .clicked()
        {
            actions.animation_key = Some(AnimationKey {
                clip,
                time: position.time,
                channel,
                remove: true,
            });
        }
        if ui
            .add_enabled(draft.axes != initial, egui::Button::new("Сбросить"))
            .clicked()
        {
            draft.axes = initial;
            actions.animation_preview = Some(None);
        }
    });
    ui.ctx().data_mut(|data| data.insert_temp(draft_id, draft));
}

fn key_timeline(ui: &mut egui::Ui, times: &[f32], duration: f32, controls: &mut Controls) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.), 20.),
        egui::Sense::click_and_drag(),
    );
    let bounds = rect.shrink2(egui::vec2(6., 1.));
    let range = duration.max(controls.time).max(0.001);
    let x = |time: f32| egui::lerp(bounds.x_range(), time / range);
    if let Some(pointer) = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() || response.dragged())
    {
        let time = ((pointer.x - bounds.left()) / bounds.width().max(1.)).clamp(0., 1.) * range;
        let next = times.partition_point(|&key| key < time);
        controls.time = [
            next.checked_sub(1).and_then(|i| times.get(i)),
            times.get(next),
        ]
        .into_iter()
        .flatten()
        .copied()
        .min_by(|a, b| {
            (x(*a) - pointer.x)
                .abs()
                .total_cmp(&(x(*b) - pointer.x).abs())
        })
        .filter(|&key| (x(key) - pointer.x).abs() <= 5.)
        .unwrap_or(time);
    }
    let paint = ui.painter();
    let y = bounds.center().y;
    let visuals = ui.visuals();
    paint.rect_filled(rect, 4., visuals.extreme_bg_color);
    let spacing = 3. / bounds.width().max(1.) * range;
    let mut remaining = times;
    while let Some((&key, rest)) = remaining.split_first() {
        paint.circle_filled(
            egui::pos2(x(key), y),
            2.5,
            visuals.widgets.active.fg_stroke.color,
        );
        remaining = &rest[rest.partition_point(|&time| time - key < spacing)..];
    }
    let playhead = x(controls.time);
    paint.line_segment(
        [
            egui::pos2(playhead, bounds.top()),
            egui::pos2(playhead, bounds.bottom()),
        ],
        egui::Stroke::new(2., visuals.selection.stroke.color),
    );
    response.on_hover_text("Ключи выбранного канала. Нажмите на метку, чтобы перейти к ключу.");
}
