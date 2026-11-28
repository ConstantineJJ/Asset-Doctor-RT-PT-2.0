use crate::{
    accent::{self, Accent, pastel},
    imports::PreparedAsset,
    rig_playback::Controls,
    root_motion::PathState,
    viewer_ui::Actions,
};
use ad_document::{
    AnimationClip, AnimationKey, AssetDocument, ChannelValue, ClipEdit, JointEdit, NodeId,
    TrackKind,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    mut controls: Controls,
    local_axes: &mut bool,
    gizmo_size: &mut f32,
    skeleton_on: &mut bool,
    root_path: &mut PathState,
    fit: &mut crate::joint_fit::FitState,
    actions: &mut Actions,
) {
    let Some(asset) = asset else {
        ui.label("Откройте GLB со скином или анимацией.");
        return;
    };
    let document = asset.document();
    let before = controls;
    // Owner 2026-11-26: the top third creates clips and edits keys; the
    // joint tree and the skin take the rest. Playback is at the bottom of
    // the viewport. A right click on free space opens the Rigging menu.
    let top = (ui.available_height() / 3.).max(150.);
    let panel = ui
        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("rig_top", document.id().0))
                .max_height(top)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    animation_section(ui, document, &mut controls, local_axes, gizmo_size, actions);
                    if let Some(clip) = controls.clip {
                        ui.add_space(8.);
                        root_motion_card(ui, document, clip, controls.time, root_path);
                    }
                });
            ui.separator();
            rig_section(ui, document, controls, fit, actions);
        })
        .response;
    panel.context_menu(|ui| {
        panel_menu(
            ui,
            document,
            &mut controls,
            local_axes,
            skeleton_on,
            actions,
        );
    });
    if controls != before {
        actions.playback = Some(controls);
    }
}

/// Creating (yellow), the chosen clip (blue) and its keys (pink), each a
/// card with a form: labels on the left, controls on the right.
fn animation_section(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: &mut Controls,
    local_axes: &mut bool,
    gizmo_size: &mut f32,
    actions: &mut Actions,
) {
    let clips = document.animation_clips();
    let clip = controls.clip.filter(|&clip| clip < clips.len());
    roomy(ui);
    accent::card(ui, Accent::New, "Новая анимация", |ui| {
        ui.horizontal_wrapped(|ui| {
            if document.is_read_only() {
                let create = new_clip_edit(ui.ctx(), document);
                if ui
                    .add_enabled(create.is_some(), egui::Button::new("＋ Новый клип"))
                    .on_hover_text("Создать клип с ключом поворота в исходной позе")
                    .clicked()
                {
                    actions.clip_edit = create;
                }
            }
            if ui
                .add_enabled(
                    clip.is_some() && !controls.playing,
                    egui::Button::new("Дублировать клип"),
                )
                .on_hover_text("Новый клип — копия выбранного")
                .clicked()
                && let Some(clip) = clip
            {
                actions.clip_edit = Some(duplicate_edit(&clips[clip], clip, ""));
            }
        });
    });
    ui.add_space(8.);
    let Some(clip) = clip else {
        ui.weak(if clips.is_empty() {
            "В модели нет клипов. Создайте клип, чтобы ставить ключи."
        } else {
            "Выберите клип в панели внизу вьюпорта, чтобы править его и ставить ключи."
        });
        return;
    };
    if controls.playing {
        ui.label(
            egui::RichText::new("Идёт воспроизведение. Пауза (Пробел) — чтобы править.")
                .color(Accent::Main.text()),
        );
        ui.add_space(6.);
    }
    ui.add_enabled_ui(!controls.playing, |ui| {
        let title = format!("Клип «{}»", clips[clip].name);
        accent::card(ui, Accent::Existing, &title, |ui| {
            clip_properties(ui, document, clip, actions);
        });
        ui.add_space(8.);
        accent::card(ui, Accent::Edit, "Ключи и поза", |ui| {
            key_editor(
                ui, document, clip, controls, local_axes, gizmo_size, actions,
            );
        });
    });
}

/// ANIM-05: where a joint goes during the clip (the skeleton's root or the
/// selected joint): displacement, distance on the floor, path, speed, turn
/// and height; the path is drawn in the viewport.
fn root_motion_card(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    time: f32,
    state: &mut PathState,
) {
    use crate::root_motion::{measure, root_joint};
    roomy(ui);
    accent::card(
        ui,
        Accent::Existing,
        "Root motion — перемещение за клип",
        |ui| {
            let selected = ui.ctx().data(|d| {
                d.get_temp::<NodeId>(crate::viewer_ui::skeleton_ui::joint_id(document.id().0))
            });
            let node = if state.selected_joint {
                selected
            } else {
                root_joint(document, clip)
            };
            let stale = state.motion.as_ref().is_none_or(|m| {
                (m.document, m.revision, m.clip, Some(m.node))
                    != (document.id().0, document.revision(), clip, node)
            });
            if stale {
                state.motion = node.and_then(|node| measure(document, node, clip));
            }
            let white = |ui: &mut egui::Ui, value: String| {
                accent::bold(ui, value, accent::VALUE);
            };
            form(egui::Id::new(("root_form", document.id().0))).show(ui, |ui| {
            accent::field_label(ui, "Сустав");
            let group = ui
                .horizontal(|ui| {
                    ui.selectable_value(&mut state.selected_joint, false, "Корень скелета");
                    ui.selectable_value(&mut state.selected_joint, true, "Выбранный");
                })
                .response;
            if accent::wheel_steps(ui, &group) != 0 {
                state.selected_joint = !state.selected_joint;
            }
            ui.end_row();
            let Some(motion) = state.motion.clone() else {
                ui.label("");
                ui.weak(if state.selected_joint {
                    "Выберите сустав в дереве или во вьюпорте."
                } else {
                    "Траекторию не удалось построить."
                });
                ui.end_row();
                return;
            };
            let s = motion.summary;
            ui.label("");
            ui.weak(
                document
                    .nodes()
                    .get(motion.node.0)
                    .map_or("", |n| n.name.as_str()),
            );
            ui.end_row();
            accent::field_label(ui, "Смещение");
            ui.horizontal(|ui| {
                for (value, (letter, color)) in s.delta.iter().zip(AXES) {
                    ui.label(egui::RichText::new(letter).strong().color(color));
                    white(ui, format!("{value:.3}"));
                    ui.add_space(4.);
                }
                ui.label("м");
            });
            ui.end_row();
            accent::field_label(ui, "По полу");
            ui.horizontal(|ui| {
                white(ui, format!("{:.3}", s.horizontal));
                ui.label("м ·");
                white(ui, format!("{:.2}", s.speed));
                ui.label("м/с");
            })
            .response
            .on_hover_text("Расстояние от начала до конца по горизонтали и средняя скорость за клип");
            ui.end_row();
            accent::field_label(ui, "Путь");
            ui.horizontal(|ui| {
                white(ui, format!("{:.3}", s.length));
                ui.label("м");
            })
            .response
            .on_hover_text("Длина траектории с подъёмами и покачиваниями (30 точек в секунду)");
            ui.end_row();
            accent::field_label(ui, "Поворот");
            ui.horizontal(|ui| {
                white(ui, format!("{:.1}", s.yaw));
                ui.label("° вокруг Y");
            });
            ui.end_row();
            accent::field_label(ui, "Высота");
            ui.horizontal(|ui| {
                white(ui, format!("{:.3}", s.min_y));
                ui.label("…");
                white(ui, format!("{:.3}", s.max_y));
                ui.label("м");
            });
            ui.end_row();
            accent::field_label(ui, "Сейчас");
            ui.horizontal(|ui| {
                if let Some(p) = motion.at(time) {
                    for (value, (letter, color)) in p.iter().zip(AXES) {
                        ui.label(egui::RichText::new(letter).strong().color(color));
                        white(ui, format!("{value:.3}"));
                        ui.add_space(4.);
                    }
                }
            });
            ui.end_row();
            accent::field_label(ui, "Во вьюпорте");
            ui.checkbox(&mut state.show, "Показать путь")
                .on_hover_text("Синяя линия — путь сустава за клип, зелёная точка — начало, красная — конец, белое кольцо — сейчас");
            ui.end_row();
        });
        },
    );
}

/// More air between rows and taller controls in the Rigging forms.
fn roomy(ui: &mut egui::Ui) {
    let spacing = ui.spacing_mut();
    spacing.item_spacing = egui::vec2(8., 8.);
    spacing.interact_size.y = 24.;
    spacing.button_padding = egui::vec2(8., 4.);
}

/// Width of the label column, the same in every Rigging form.
const LABELS: f32 = 92.;

fn form(id: egui::Id) -> egui::Grid {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(LABELS)
        .spacing([10., 10.])
}

/// The skin report and the joint tree.
fn rig_section(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: Controls,
    fit: &mut crate::joint_fit::FitState,
    actions: &mut Actions,
) {
    let Some(rig) = document.rig() else {
        ui.label("У этой модели нет скина.");
        return;
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Скин и веса").strong());
        if rig_changed(document) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                reset_button(ui, actions);
            });
        }
    });
    let selector_id = ui.id().with(("skin", document.id().0));
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(selector_id))
        .unwrap_or(0)
        .min(rig.skins.len() - 1);
    let combo = egui::ComboBox::from_id_salt(selector_id)
        .selected_text(format!("{} · {}", selected + 1, rig.skins[selected].name))
        .show_ui(ui, |ui| {
            for (index, skin) in rig.skins.iter().enumerate() {
                ui.selectable_value(
                    &mut selected,
                    index,
                    format!("{} · {}", index + 1, skin.name),
                );
            }
        })
        .response;
    selected = accent::wheel_choice(ui, &combo, selected, rig.skins.len());
    ui.ctx()
        .data_mut(|data| data.insert_temp(selector_id, selected));

    let skin = &rig.skins[selected];
    let report = &rig.reports[selected];
    ui.horizontal_wrapped(|ui| {
        accent::stat(
            ui,
            "Суставов:",
            skin.joints.len().to_string(),
            "",
            pastel::SKY,
        );
        accent::stat(
            ui,
            "без весов:",
            report.unused_joints.len().to_string(),
            "",
            pastel::ROSE,
        );
    });
    if let Some(root) = skin.skeleton {
        ui.label(format!("Корень скелета: {}", document.nodes()[root.0].name));
    }
    if report.normalized_vertices > 0 {
        accent::stat(
            ui,
            "Вершин с нормализованными весами:",
            report.normalized_vertices.to_string(),
            "",
            pastel::PEACH,
        );
    }
    ui.add_space(6.);
    joint_card(ui, document, actions);
    ui.add_space(6.);
    fit_card(ui, document, fit, actions);
    ui.add_space(6.);
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Суставы").strong());
        ui.weak("· клик во вьюпорте тоже выбирает · ПКМ — меню");
    });
    egui::ScrollArea::vertical()
        .id_salt((selector_id, selected))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            joint_tree(ui, document, skin, report, controls, actions)
        });
}

/// Right click on free space in the Rigging tab.
fn panel_menu(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: &mut Controls,
    local_axes: &mut bool,
    skeleton_on: &mut bool,
    actions: &mut Actions,
) {
    let clips = document.animation_clips();
    let clip = controls.clip.filter(|&clip| clip < clips.len());
    let durations: Vec<f32> = clips.iter().map(|clip| clip.duration).collect();
    accent::scope(ui, Accent::Main, |ui| {
        if let Some(next) = controls.toggled(&durations)
            && ui
                .button(if controls.playing {
                    "Пауза · Пробел"
                } else {
                    "Воспроизвести · Пробел"
                })
                .clicked()
        {
            *controls = next;
            ui.close();
        }
    });
    accent::scope(ui, Accent::Existing, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(clips.len() > 1, egui::Button::new("◀ Пред. клип"))
                .clicked()
                && let Some(next) = controls.switched(clips.len(), -1)
            {
                *controls = next;
                ui.close();
            }
            if ui
                .add_enabled(clips.len() > 1, egui::Button::new("След. клип ▶"))
                .clicked()
                && let Some(next) = controls.switched(clips.len(), 1)
            {
                *controls = next;
                ui.close();
            }
        });
        if let Some(clip) = clip
            && ui
                .add_enabled(!controls.playing, egui::Button::new("Удалить клип"))
                .clicked()
        {
            actions.clip_edit = Some(ClipEdit::Delete { clip });
            ui.close();
        }
    });
    accent::scope(ui, Accent::New, |ui| {
        if document.is_read_only() {
            let create = new_clip_edit(ui.ctx(), document);
            if ui
                .add_enabled(create.is_some(), egui::Button::new("＋ Новый клип"))
                .clicked()
            {
                actions.clip_edit = create;
                ui.close();
            }
        }
        if let Some(clip) = clip
            && ui.button("Дублировать клип").clicked()
        {
            actions.clip_edit = Some(duplicate_edit(&clips[clip], clip, ""));
            ui.close();
        }
    });
    ui.separator();
    accent::scope(ui, Accent::Edit, |ui| {
        ui.horizontal(|ui| {
            ui.label("Gizmo сустава:");
            ui.selectable_value(local_axes, true, "Локальные");
            ui.selectable_value(local_axes, false, "Мировые");
        });
    });
    if ui
        .button(if *skeleton_on {
            "Скрыть риг · Z"
        } else {
            "Показать риг · Z"
        })
        .clicked()
    {
        *skeleton_on = !*skeleton_on;
        ui.close();
    }
    if rig_changed(document) && reset_button(ui, actions) {
        ui.close();
    }
}

/// Right click on a joint (tree row or the skeleton in the viewport): select
/// it and key its current pose at the playhead.
pub(super) fn joint_menu(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    node: NodeId,
    controls: Controls,
    actions: &mut Actions,
) {
    use crate::viewer_ui::skeleton_ui::select_joint;
    let animatable = document.can_animate_node(node);
    let name = document
        .nodes()
        .get(node.0)
        .map_or_else(String::new, |n| n.name.clone());
    ui.label(
        egui::RichText::new(if name.is_empty() {
            "Сустав без имени"
        } else {
            &name
        })
        .strong(),
    );
    if ui.button("Выбрать сустав").clicked() {
        select_joint(ui.ctx(), document.id().0, node, animatable, actions);
        ui.close();
    }
    accent::scope(ui, Accent::Edit, |ui| {
        if ui
            .button("Переименовать…")
            .on_hover_text("Поле имени в карточке «Сустав» над деревом")
            .clicked()
        {
            select_joint(ui.ctx(), document.id().0, node, animatable, actions);
            ui.ctx()
                .data_mut(|d| d.insert_temp(rename_focus_id(document), true));
            ui.close();
        }
        let child = default_child(document, node, ChildDirection::Bone, None);
        if ui
            .add_enabled(child.is_some(), egui::Button::new("＋ Дочерний сустав"))
            .on_hover_text("Новый сустав-продолжение кости, без весов; Undo отменит")
            .clicked()
            && let Some((translation, _)) = child
        {
            actions.joint_edit = Some(JointEdit::Add {
                parent: node,
                name: child_name(document, node),
                translation,
            });
            ui.close();
        }
    });
    if is_joint(document, node) && delete_button(ui, document, node, actions) {
        ui.close();
    }
    let clips = document.animation_clips();
    let Some(clip) = controls.clip.filter(|&clip| clip < clips.len()) else {
        ui.weak("Выберите клип внизу, чтобы ставить ключи.");
        return;
    };
    if controls.playing {
        ui.weak("Пауза (Пробел) — чтобы ставить ключи.");
        return;
    }
    if !animatable {
        ui.weak("У сустава нет TRS-каналов.");
        return;
    }
    ui.separator();
    accent::scope(ui, Accent::Edit, |ui| {
        for kind in [
            TrackKind::Rotation,
            TrackKind::Translation,
            TrackKind::Scale,
        ] {
            let exists = key_exists(document, clip, node, kind, controls.time);
            let text = format!(
                "{} ключ «{}» · {:.3} с",
                if exists {
                    "Обновить"
                } else {
                    "Поставить"
                },
                kind_name(kind),
                controls.time
            );
            if ui
                .button(text)
                .on_hover_text("Текущая поза сустава в этом канале")
                .clicked()
            {
                select_joint(ui.ctx(), document.id().0, node, true, actions);
                ui.ctx()
                    .data_mut(|d| d.insert_temp(key_kind_id(document), kind));
                actions.animation_key = key_at(document, clip, node, kind, controls.time, false);
                ui.close();
            }
        }
        for kind in [
            TrackKind::Rotation,
            TrackKind::Translation,
            TrackKind::Scale,
        ] {
            if can_remove_key(document, clip, node, kind, controls.time)
                && ui
                    .button(format!("Удалить ключ «{}»", kind_name(kind)))
                    .clicked()
            {
                actions.animation_key = key_at(document, clip, node, kind, controls.time, true);
                ui.close();
            }
        }
    });
}

/// Where a new child joint points: along the parent's bone, or along one of
/// the parent's local axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildDirection {
    Bone,
    X,
    Y,
    Z,
}

/// The new joint's translation in the parent's local space and its length
/// in world units: `direction` from the parent, `length` (default: the
/// parent's own bone, else a tenth of the skeleton's size).
fn default_child(
    document: &AssetDocument,
    parent: NodeId,
    direction: ChildDirection,
    length: Option<f32>,
) -> Option<([f32; 3], f32)> {
    use glam::{Mat4, Vec3};
    let rig = document.rig()?;
    if !rig.skins.iter().any(|skin| skin.joints.contains(&parent)) {
        return None;
    }
    let worlds = document
        .view()
        .node_worlds(None, &ad_document::Cancellation::default())
        .ok()?;
    let world = Mat4::from_cols_array_2d(&worlds.get(parent.0).copied().flatten()?);
    let at = world.w_axis.truncate();
    let up = document
        .nodes()
        .iter()
        .position(|n| n.children.contains(&parent))
        .and_then(|p| worlds[p])
        .map(|m| at - Mat4::from_cols_array_2d(&m).w_axis.truncate())
        .filter(|bone| bone.length() > 1e-6);
    let size = {
        let joints = rig.skins.iter().flat_map(|s| &s.joints);
        let points: Vec<Vec3> = joints
            .filter_map(|j| worlds.get(j.0).copied().flatten())
            .map(|m| Mat4::from_cols_array_2d(&m).w_axis.truncate())
            .collect();
        let (lo, hi) = points.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        (hi - lo).max_element().max(0.)
    };
    let length =
        length
            .or(up.map(Vec3::length))
            .unwrap_or(if size > 0. { size * 0.1 } else { 0.1 });
    let axis = |a: Vec3| world.transform_vector3(a).normalize_or_zero();
    let direction = match direction {
        ChildDirection::Bone => up.map_or(Vec3::Y, Vec3::normalize),
        ChildDirection::X => axis(Vec3::X),
        ChildDirection::Y => axis(Vec3::Y),
        ChildDirection::Z => axis(Vec3::Z),
    };
    if direction == Vec3::ZERO || !length.is_finite() || length <= 0. {
        return None;
    }
    let local = world.inverse().transform_vector3(direction * length);
    local.is_finite().then(|| (local.to_array(), length))
}

/// «<parent>_end», or «<parent>_end2», «_end3»… when taken.
fn child_name(document: &AssetDocument, parent: NodeId) -> String {
    let stem: String = document.nodes()[parent.0].name.chars().take(100).collect();
    let taken = |name: &str| document.nodes().iter().any(|n| n.name == name);
    let mut name = format!("{stem}_end");
    let mut n = 2;
    while taken(&name) {
        name = format!("{stem}_end{n}");
        n += 1;
    }
    name
}

/// The skeleton differs from the file (renamed, added, deleted or moved).
fn rig_changed(document: &AssetDocument) -> bool {
    document.is_restructured()
        || !document.renamed_nodes().is_empty()
        || !document.rested_nodes().is_empty()
}

/// RIG-06: «Сбросить риг к файлу» (red); true when pressed.
fn reset_button(ui: &mut egui::Ui, actions: &mut Actions) -> bool {
    let pressed = accent::scope(ui, Accent::Main, |ui| {
        ui.button("Сбросить риг к файлу")
            .on_hover_text("Имена, положения, суставы, привязки и веса — как в файле. Клипы сохраняют ключи: каналы удалённых суставов возвращаются, каналы добавленных уходят. Undo отменит.")
            .clicked()
    });
    if pressed {
        actions.joint_edit = Some(JointEdit::Reset);
    }
    pressed
}

/// RIG-05: «Подгонка к мешу» — search, the suggestions with their shift
/// and confidence, and applying the chosen ones as one edit.
fn fit_card(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    fit: &mut crate::joint_fit::FitState,
    actions: &mut Actions,
) {
    use accent::pastel;
    use ad_document::FitConfidence;
    roomy(ui);
    accent::card(ui, Accent::Edit, "Подгонка к мешу", |ui| {
        if fit.running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Ищу положения суставов…");
                actions.fit_cancel |= ui.button("Отмена").clicked();
            });
            return;
        }
        let current = fit.is_current(document);
        ui.horizontal_wrapped(|ui| {
            actions.fit_search |= ui
                .button(if current {
                    "Найти заново"
                } else {
                    "Найти положения суставов"
                })
                .on_hover_text("Сустав ставится в центр сгиба: в среднюю точку вершин, которые тянут и он, и его родитель. Модель не меняется, пока вы не примените.")
                .clicked();
            if fit.searched && !current {
                ui.weak("Модель изменилась — найдите заново.");
            }
        });
        if let Some(error) = &fit.error {
            ui.label(egui::RichText::new(error).color(Accent::Main.text()));
        }
        if !current {
            return;
        }
        if fit.items.is_empty() {
            ui.weak("Подсказок нет: веса суставов не смешиваются с весами родителей.");
            return;
        }
        let confident = fit
            .items
            .iter()
            .filter(|i| i.fit.confidence == FitConfidence::High)
            .count();
        let chosen: Vec<(NodeId, [f32; 3])> = fit
            .items
            .iter()
            .filter(|i| i.selected)
            .map(|i| (i.fit.node, i.fit.suggested))
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label("Подсказок:");
            accent::bold(ui, fit.items.len().to_string(), accent::VALUE);
            ui.label("· уверенных:");
            accent::bold(ui, confident.to_string(), pastel::MINT);
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Выбрать уверенные").clicked() {
                for item in &mut fit.items {
                    item.selected =
                        item.fit.confidence == FitConfidence::High && item.fit.shift() > 1e-3;
                }
            }
            if ui.button("Снять выбор").clicked() {
                for item in &mut fit.items {
                    item.selected = false;
                }
            }
            if ui
                .add_enabled(
                    !chosen.is_empty(),
                    egui::Button::new(format!("Применить ({})", chosen.len())),
                )
                .on_hover_text("Выбранные суставы встанут в предложенные точки одной правкой; меш в позе покоя остаётся на месте. Undo отменит.")
                .clicked()
            {
                actions.joint_edit = Some(JointEdit::Fit { targets: chosen });
            }
        });
        egui::ScrollArea::vertical()
            .id_salt(("fit_list", document.id().0))
            .max_height(220.)
            .show(ui, |ui| {
                for item in &mut fit.items {
                    let name = document
                        .nodes()
                        .get(item.fit.node.0)
                        .map_or("", |n| n.name.as_str());
                    let (label, color) = match item.fit.confidence {
                        FitConfidence::High => ("высокая", pastel::MINT),
                        FitConfidence::Medium => ("средняя", pastel::LEMON),
                        FitConfidence::Low => ("низкая", pastel::ROSE),
                    };
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut item.selected, name);
                        accent::bold(ui, format!("{:.1}", item.fit.shift() * 100.), accent::VALUE);
                        ui.label("см ·");
                        ui.label(egui::RichText::new(label).color(color));
                    })
                    .response
                    .on_hover_text(format!(
                        "Вершин в кольце сгиба: {} · длина кости: {:.3} м",
                        item.fit.samples, item.fit.bone
                    ));
                }
            });
    });
}

/// A joint of some skin in the current document (a deleted joint is not).
fn is_joint(document: &AssetDocument, node: NodeId) -> bool {
    document
        .rig()
        .is_some_and(|rig| rig.skins.iter().any(|skin| skin.joints.contains(&node)))
}

/// Nodes under `node` (not counting it).
fn descendants(document: &AssetDocument, node: NodeId) -> usize {
    let nodes = document.nodes();
    let mut seen = vec![false; nodes.len()];
    let mut pending = nodes[node.0].children.clone();
    let mut count = 0;
    while let Some(n) = pending.pop() {
        if n.0 >= nodes.len() || std::mem::replace(&mut seen[n.0], true) {
            continue;
        }
        count += 1;
        pending.extend(nodes[n.0].children.iter().copied());
    }
    count
}

/// RIG-04: «Удалить сустав» (red: it removes); true when pressed.
fn delete_button(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    node: NodeId,
    actions: &mut Actions,
) -> bool {
    let below = descendants(document, node);
    let label = if below == 0 {
        "Удалить сустав".to_owned()
    } else {
        format!("Удалить сустав и {below} ниже")
    };
    let pressed = accent::scope(ui, Accent::Main, |ui| {
        ui.button(label)
            .on_hover_text("Сустав и всё под ним уходят из скелета; их веса переходят ближайшему суставу выше, их каналы — из клипов. Undo отменит.")
            .clicked()
    });
    if pressed {
        actions.joint_edit = Some(JointEdit::Delete { node });
    }
    pressed
}

fn rename_focus_id(document: &AssetDocument) -> egui::Id {
    egui::Id::new(("joint_rename_focus", document.id().0))
}

#[derive(Clone)]
struct JointDraft {
    node: NodeId,
    revision: u64,
    name: String,
    child: String,
    direction: ChildDirection,
    length: f32,
    /// RIG-03: the bone to this joint (world length) and its rest position
    /// in the parent's space.
    bone: f32,
    position: [f32; 3],
}

/// World length of the bone from the parent node to `node` at rest.
fn bone_length(document: &AssetDocument, node: NodeId) -> Option<f32> {
    let worlds = document
        .view()
        .node_worlds(None, &ad_document::Cancellation::default())
        .ok()?;
    let parent = document
        .nodes()
        .iter()
        .position(|n| n.children.contains(&node))?;
    let at = |n: usize| {
        glam::Mat4::from_cols_array_2d(&worlds[n]?)
            .w_axis
            .truncate()
            .into()
    };
    let (a, b): (Option<glam::Vec3>, Option<glam::Vec3>) = (at(parent), at(node.0));
    Some(a?.distance(b?))
}

/// RIG-03: the selected joint's name and a new child joint (pink: editing).
fn joint_card(ui: &mut egui::Ui, document: &AssetDocument, actions: &mut Actions) {
    use crate::viewer_ui::skeleton_ui::joint_id;
    let selected = ui
        .ctx()
        .data(|d| d.get_temp::<NodeId>(joint_id(document.id().0)))
        .filter(|n| is_joint(document, *n));
    roomy(ui);
    accent::card(ui, Accent::Edit, "Сустав", |ui| {
        let Some(node) = selected else {
            ui.weak(
                "Выберите сустав в дереве или во вьюпорте: здесь его имя и новый дочерний сустав.",
            );
            return;
        };
        let id = egui::Id::new(("joint_draft", document.id().0));
        let mut draft = ui
            .ctx()
            .data(|d| d.get_temp::<JointDraft>(id))
            .filter(|d| d.node == node && d.revision == document.revision())
            .unwrap_or_else(|| JointDraft {
                node,
                revision: document.revision(),
                name: document.nodes()[node.0].name.clone(),
                child: child_name(document, node),
                direction: ChildDirection::Bone,
                length: default_child(document, node, ChildDirection::Bone, None)
                    .map_or(0.1, |(_, length)| length),
                bone: bone_length(document, node).unwrap_or(0.),
                position: document
                    .rest_transform(node)
                    .map_or([0.; 3], |rest| rest.translation),
            });
        form(egui::Id::new(("joint_form", document.id().0))).show(ui, |ui| {
            accent::field_label(ui, "Имя");
            ui.horizontal(|ui| {
                let field = accent::values(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .char_limit(120)
                            .desired_width((ui.available_width() - 130.).max(80.)),
                    )
                });
                let focus = rename_focus_id(document);
                if ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(focus))
                    .unwrap_or(false)
                {
                    field.request_focus();
                    ui.ctx().data_mut(|d| d.remove::<bool>(focus));
                }
                let name = draft.name.trim();
                let changed = !name.is_empty() && name != document.nodes()[node.0].name;
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui
                    .add_enabled(changed, egui::Button::new("Переименовать"))
                    .clicked();
                if clicked || (enter && changed) {
                    actions.joint_edit = Some(JointEdit::Rename {
                        node,
                        name: name.to_owned(),
                    });
                }
            });
            ui.end_row();

            if let Some(rest) = document.rest_transform(node)
                && let Some(length) = bone_length(document, node)
            {
                accent::field_label(ui, "Длина кости");
                ui.horizontal(|ui| {
                    accent::values(ui, |ui| {
                        let response = ui
                            .add(
                                egui::DragValue::new(&mut draft.bone)
                                    .range(0.0001..=10_000.)
                                    .speed(0.005)
                                    .max_decimals(4)
                                    .suffix(" м"),
                            )
                            .on_hover_text("От родителя до этого сустава; колёсико — шаг 0.01");
                        accent::wheel_value(ui, &response, &mut draft.bone, 0.01, 0.0001..=10_000.);
                    });
                    let can = length > 1e-6 && (draft.bone - length).abs() > 1e-6;
                    if ui
                        .add_enabled(can, egui::Button::new("Применить"))
                        .on_hover_text("Сустав сдвигается вдоль кости, меш в позе покоя остаётся на месте (перепривязка). Undo отменит.")
                        .clicked()
                    {
                        let k = draft.bone / length;
                        actions.joint_edit = Some(JointEdit::Rest {
                            node,
                            translation: rest.translation.map(|v| v * k),
                        });
                    }
                });
                ui.end_row();
                accent::field_label(ui, "Положение");
                ui.horizontal(|ui| {
                    for (value, (letter, color)) in draft.position.iter_mut().zip(AXES) {
                        ui.label(egui::RichText::new(letter).strong().color(color));
                        accent::values(ui, |ui| {
                            let response = ui.add(
                                egui::DragValue::new(value).speed(0.005).max_decimals(4),
                            );
                            accent::wheel_value(ui, &response, value, 0.01, -1e6..=1e6);
                        });
                    }
                    if ui
                        .add_enabled(
                            draft.position != rest.translation,
                            egui::Button::new("Применить"),
                        )
                        .on_hover_text("Положение сустава в позе покоя, в осях родителя; меш остаётся на месте. Undo отменит.")
                        .clicked()
                    {
                        actions.joint_edit = Some(JointEdit::Rest {
                            node,
                            translation: draft.position,
                        });
                    }
                });
                ui.end_row();
            }
            accent::field_label(ui, "Удалить");
            delete_button(ui, document, node, actions);
            ui.end_row();
            let can_add = default_child(document, node, ChildDirection::Bone, None).is_some();
            if !can_add {
                ui.label("");
                ui.weak("Дочерний сустав — только под суставом скина.");
                ui.end_row();
                return;
            }
            accent::field_label(ui, "Новый дочерний");
            accent::values(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.child)
                        .char_limit(120)
                        .desired_width(f32::INFINITY),
                )
            });
            ui.end_row();
            accent::field_label(ui, "Направление");
            let directions = [
                (ChildDirection::Bone, "Вдоль кости"),
                (ChildDirection::X, "X"),
                (ChildDirection::Y, "Y"),
                (ChildDirection::Z, "Z"),
            ];
            let group = ui
                .horizontal(|ui| {
                    for (choice, label) in directions {
                        ui.selectable_value(&mut draft.direction, choice, label);
                    }
                })
                .response
                .on_hover_text(
                    "Вдоль кости — продолжение кости родителя; X, Y, Z — локальные оси родителя",
                );
            let at = directions
                .iter()
                .position(|(d, _)| *d == draft.direction)
                .unwrap_or(0);
            draft.direction = directions[accent::wheel_choice(ui, &group, at, directions.len())].0;
            ui.end_row();
            accent::field_label(ui, "Длина");
            ui.horizontal(|ui| {
                accent::values(ui, |ui| {
                    let response = ui.add(
                        egui::DragValue::new(&mut draft.length)
                            .range(0.0001..=10_000.)
                            .speed(0.005)
                            .max_decimals(4)
                            .suffix(" м"),
                    );
                    accent::wheel_value(ui, &response, &mut draft.length, 0.01, 0.0001..=10_000.);
                });
                let child = default_child(document, node, draft.direction, Some(draft.length));
                let name = draft.child.trim();
                if ui
                    .add_enabled(
                        child.is_some() && !name.is_empty(),
                        egui::Button::new("＋ Добавить сустав"),
                    )
                    .on_hover_text("Новый сустав без весов, в скине родителя; Undo отменит")
                    .clicked()
                    && let Some((translation, _)) = child
                {
                    actions.joint_edit = Some(JointEdit::Add {
                        parent: node,
                        name: name.to_owned(),
                        translation,
                    });
                }
            });
            ui.end_row();
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, draft));
    });
}

/// The channel the key editor works on.
pub(super) fn key_kind_id(document: &AssetDocument) -> egui::Id {
    egui::Id::new(("animation_node", document.id().0)).with("kind")
}

pub(super) fn kind_name(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Translation => "сдвиг",
        TrackKind::Rotation => "поворот",
        TrackKind::Scale => "масштаб",
    }
}

fn key_exists(
    document: &AssetDocument,
    clip: usize,
    node: NodeId,
    kind: TrackKind,
    time: f32,
) -> bool {
    let times = document.animation_clips()[clip]
        .track(node, kind)
        .map_or(&[][..], |track| track.times());
    times.get(times.partition_point(|&t| t < time)) == Some(&time)
}

/// A key at `time` can be removed unless it is the clip's last one.
pub(super) fn can_remove_key(
    document: &AssetDocument,
    clip: usize,
    node: NodeId,
    kind: TrackKind,
    time: f32,
) -> bool {
    let active = &document.animation_clips()[clip];
    let count = active
        .track(node, kind)
        .map_or(0, |track| track.times().len());
    key_exists(document, clip, node, kind, time) && (count > 1 || active.tracks().len() > 1)
}

/// A key of the channel's current value at `time` (or its removal).
pub(super) fn key_at(
    document: &AssetDocument,
    clip: usize,
    node: NodeId,
    kind: TrackKind,
    time: f32,
    remove: bool,
) -> Option<AnimationKey> {
    let value = document.animation_value(clip, node, kind, time).ok()?;
    Some(AnimationKey {
        clip,
        time,
        channel: ChannelValue { node, kind, value },
        remove,
    })
}

/// Joints of `skin` as a tree (parent → children by the scene hierarchy;
/// helper nodes between joints are skipped). The selection is shared with
/// the skeleton overlay; a joint picked in the viewport opens its branch and
/// scrolls into view.
fn joint_tree(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    skin: &ad_document::Skin,
    report: &ad_document::SkinReport,
    controls: Controls,
    actions: &mut Actions,
) {
    use crate::viewer_ui::skeleton_ui::{joint_id, select_joint};
    let nodes = document.nodes();
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        for child in &node.children {
            if let Some(slot) = parents.get_mut(child.0) {
                *slot = Some(index);
            }
        }
    }
    let mut slot_of = vec![None; nodes.len()];
    for (slot, joint) in skin.joints.iter().enumerate() {
        if joint.0 < nodes.len() {
            slot_of[joint.0] = Some(slot);
        }
    }
    let joint_parent = |node: usize| {
        let mut parent = parents[node];
        let mut steps = 0;
        while let Some(p) = parent
            && slot_of[p].is_none()
            && steps < nodes.len()
        {
            parent = parents[p];
            steps += 1;
        }
        parent.filter(|p| slot_of[*p].is_some())
    };
    let mut children = vec![Vec::new(); skin.joints.len()];
    let mut roots = Vec::new();
    for (slot, joint) in skin.joints.iter().enumerate() {
        match joint_parent(joint.0).and_then(|p| slot_of[p]) {
            Some(parent) if parent != slot => children[parent].push(slot),
            _ => roots.push(slot),
        }
    }
    let document_id = document.id().0;
    let selected = ui
        .ctx()
        .data(|d| d.get_temp::<NodeId>(joint_id(document_id)));
    // A new selection (from the viewport or here) opens its ancestors once.
    let seen_id = joint_id(document_id).with("seen");
    let seen = ui
        .ctx()
        .data(|d| d.get_temp::<Option<NodeId>>(seen_id))
        .flatten();
    let reveal = selected != seen;
    ui.ctx().data_mut(|d| d.insert_temp(seen_id, selected));
    let mut open = vec![false; skin.joints.len()];
    if let Some(slot) = selected.and_then(|n| slot_of.get(n.0).copied().flatten()) {
        let mut at = joint_parent(skin.joints[slot].0).and_then(|p| slot_of[p]);
        let mut steps = 0;
        while let Some(parent) = at
            && steps < skin.joints.len()
        {
            open[parent] = true;
            at = joint_parent(skin.joints[parent].0).and_then(|p| slot_of[p]);
            steps += 1;
        }
    }
    struct Tree<'a> {
        document: &'a AssetDocument,
        skin: &'a ad_document::Skin,
        report: &'a ad_document::SkinReport,
        children: &'a [Vec<usize>],
        open: &'a [bool],
        selected: Option<NodeId>,
        reveal: bool,
        controls: Controls,
    }
    fn row(ui: &mut egui::Ui, tree: &Tree, slot: usize, depth: usize, actions: &mut Actions) {
        let node = tree.skin.joints[slot];
        let unused = tree.report.unused_joints.binary_search(&slot).is_ok();
        let label = format!(
            "{} · {}{}",
            slot,
            tree.document.nodes()[node.0].name,
            if unused { " · без веса" } else { "" }
        );
        let is_selected = tree.selected == Some(node);
        let draw = |ui: &mut egui::Ui, actions: &mut Actions| {
            let response = ui
                .add(egui::Button::selectable(is_selected, label.clone()).truncate())
                .on_hover_text(format!("Узел glTF №{}", node.0));
            if is_selected && tree.reveal {
                response.scroll_to_me(Some(egui::Align::Center));
            }
            if response.clicked() {
                select_joint(
                    ui.ctx(),
                    tree.document.id().0,
                    node,
                    tree.document.can_animate_node(node),
                    actions,
                );
            }
            response.context_menu(|ui| {
                joint_menu(ui, tree.document, node, tree.controls, actions);
            });
        };
        if tree.children[slot].is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(18.);
                draw(ui, actions);
            });
            return;
        }
        let id = ui.make_persistent_id(("joint_branch", tree.document.id().0, slot));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            depth < 2,
        );
        if tree.reveal && tree.open[slot] {
            state.set_open(true);
        }
        state.show_header(ui, |ui| draw(ui, actions)).body(|ui| {
            for &child in &tree.children[slot] {
                row(ui, tree, child, depth + 1, actions);
            }
        });
    }
    let tree = Tree {
        document,
        skin,
        report,
        children: &children,
        open: &open,
        selected,
        reveal,
        controls,
    };
    for &root in &roots {
        row(ui, &tree, root, 0, actions);
    }
}

/// A new clip keyed in the rest pose: on the key editor's joint, else the
/// first joint (or node) that can be keyed.
pub(super) fn new_clip_edit(context: &egui::Context, document: &AssetDocument) -> Option<ClipEdit> {
    let node_id = egui::Id::new(("animation_node", document.id().0));
    let node = context
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
        })?;
    let clips = document.animation_clips();
    let mut number = clips.len() + 1;
    let name = loop {
        let name = format!("Клип {number}");
        if !clips.iter().any(|clip| clip.name == name) {
            break name;
        }
        number += 1;
    };
    Some(ClipEdit::Create { name, node })
}

/// A copy of `active`, named `name` or «<name> (копия)».
pub(super) fn duplicate_edit(active: &AnimationClip, clip: usize, name: &str) -> ClipEdit {
    let name = if name.is_empty() || name == active.name {
        let stem: String = active.name.trim().chars().take(112).collect();
        format!("{stem} (копия)").trim().to_owned()
    } else {
        name.to_owned()
    };
    ClipEdit::Duplicate { clip, name }
}

#[derive(Clone)]
struct ClipDraft {
    clip_id: u64,
    revision: u64,
    name: String,
    duration: f32,
}

/// Name, duration and delete for the chosen clip.
fn clip_properties(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    actions: &mut Actions,
) {
    let active = &document.animation_clips()[clip];
    let id = egui::Id::new(("clip_properties", document.id().0));
    let mut draft = ui
        .ctx()
        .data(|data| data.get_temp::<ClipDraft>(id))
        .filter(|draft| draft.clip_id == active.id() && draft.revision == document.revision())
        .unwrap_or_else(|| ClipDraft {
            clip_id: active.id(),
            revision: document.revision(),
            name: active.name.clone(),
            duration: active.duration,
        });
    form(egui::Id::new(("clip_form", document.id().0))).show(ui, |ui| {
        accent::field_label(ui, "Имя");
        ui.horizontal(|ui| {
            accent::values(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .char_limit(120)
                        .desired_width((ui.available_width() - 120.).max(80.))
                        .hint_text("Название клипа"),
                );
            });
            let name = draft.name.trim();
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
        });
        ui.end_row();

        accent::field_label(ui, "Длительность");
        ui.horizontal(|ui| {
            accent::values(ui, |ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut draft.duration)
                        .range(0. ..=f32::MAX)
                        .speed(0.01)
                        .max_decimals(6)
                        .suffix(" с"),
                );
                accent::wheel_value(ui, &response, &mut draft.duration, 0.1, 0.1..=f32::MAX);
            });
            let can_retime =
                active.duration > 0. && draft.duration > 0. && draft.duration != active.duration;
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
        ui.end_row();

        ui.label("");
        if ui.button("Удалить клип").clicked() {
            actions.clip_edit = Some(ClipEdit::Delete { clip });
        }
        ui.end_row();
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, draft));
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

const AXES: [(&str, egui::Color32); 3] = [
    ("X", egui::Color32::from_rgb(240, 96, 96)),
    ("Y", egui::Color32::from_rgb(112, 214, 112)),
    ("Z", egui::Color32::from_rgb(104, 156, 250)),
];
const KINDS: [(TrackKind, &str); 3] = [
    (TrackKind::Translation, "Сдвиг"),
    (TrackKind::Rotation, "Поворот"),
    (TrackKind::Scale, "Масштаб"),
];

/// The key editor as a form: joint, channel, time, value, key, gizmo axes.
/// The mouse wheel also steps every field and choice.
fn key_editor(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    controls: &mut Controls,
    local_axes: &mut bool,
    gizmo_size: &mut f32,
    actions: &mut Actions,
) {
    let active = &document.animation_clips()[clip];
    let node_id = egui::Id::new(("animation_node", document.id().0));
    let kind_id = key_kind_id(document);
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
    let keyable: Vec<NodeId> = (0..document.nodes().len())
        .map(NodeId)
        .filter(|&node| document.can_animate_node(node))
        .collect();

    form(egui::Id::new(("key_form", document.id().0))).show(ui, |ui| {
        accent::field_label(ui, "Сустав");
        let combo = egui::ComboBox::from_id_salt(node_id)
            .width((ui.available_width() - 4.).max(120.))
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
            })
            .response
            .on_hover_text("Колёсико — соседний сустав");
        if let Some(at) = keyable.iter().position(|&n| n == node) {
            node = keyable[accent::wheel_choice(ui, &combo, at, keyable.len())];
        }
        ui.end_row();

        accent::field_label(ui, "Канал");
        let group = ui
            .horizontal(|ui| {
                for (choice, label) in KINDS {
                    ui.selectable_value(&mut kind, choice, label);
                }
            })
            .response;
        let at = KINDS.iter().position(|(k, _)| *k == kind).unwrap_or(0);
        kind = KINDS[accent::wheel_choice(ui, &group, at, KINDS.len())].0;
        ui.end_row();
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
    form(egui::Id::new(("key_form_time", document.id().0))).show(ui, |ui| {
        accent::field_label(ui, "Время");
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
                && let Some(i) = previous
            {
                controls.time = times[i];
            }
            if ui
                .add_enabled(next.is_some(), egui::Button::new("▶"))
                .on_hover_text("Следующий ключ")
                .clicked()
                && let Some(next) = next
            {
                controls.time = next;
            }
            accent::values(ui, |ui| {
                let response = ui
                    .add(
                        egui::DragValue::new(&mut controls.time)
                            .speed(0.01)
                            .range(0. ..=f32::MAX)
                            .max_decimals(4)
                            .suffix(" с"),
                    )
                    .on_hover_text("Время ключа; колёсико — на кадр (1/30 с). Можно ввести время за пределами клипа.");
                accent::wheel_value(ui, &response, &mut controls.time, 1. / 30., 0. ..=f32::MAX);
            });
            ui.weak(format!("ключей: {}", times.len()));
        });
        ui.end_row();
    });

    let position = KeyPosition {
        revision: document.revision(),
        clip,
        node,
        kind,
        time: controls.time,
    };
    let value = document
        .animation_value(clip, node, kind, position.time)
        .ok();
    form(egui::Id::new(("key_form_value", document.id().0))).show(ui, |ui| {
        if let Some(value) = value {
            key_value_rows(ui, active, position, value, draft_id, times, actions);
        }
        accent::field_label(ui, "Оси gizmo");
        let group = ui
            .horizontal(|ui| {
                ui.selectable_value(local_axes, true, "Локальные");
                ui.selectable_value(local_axes, false, "Мировые");
            })
            .response
            .on_hover_text("Кольца поворачивают выбранный сустав, стрелки двигают его; отпускание — ключ в текущем времени клипа (Undo отменит). Нужны выбранный клип и пауза.");
        if accent::wheel_steps(ui, &group) != 0 {
            *local_axes = !*local_axes;
        }
        ui.end_row();
        accent::field_label(ui, "Размер gizmo");
        accent::values(ui, |ui| {
            let response = ui
                .add(
                    egui::DragValue::new(gizmo_size)
                        .range(0.5..=2.)
                        .speed(0.01)
                        .fixed_decimals(1)
                        .suffix("×"),
                )
                .on_hover_text("Размер колец и стрелок gizmo сустава, 0.5–2×; колёсико — шаг 0.1");
            accent::wheel_value(ui, &response, gizmo_size, 0.1, 0.5..=2.);
        });
        ui.end_row();
    });
}

/// «Значение» (X, Y, Z: coloured letters, white numbers) and «Ключ»
/// (add or update, delete, reset).
fn key_value_rows(
    ui: &mut egui::Ui,
    active: &AnimationClip,
    position: KeyPosition,
    value: [f32; 4],
    draft_id: egui::Id,
    times: &[f32],
    actions: &mut Actions,
) {
    let KeyPosition {
        clip, node, kind, ..
    } = position;
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
    let (step, unit) = if kind == TrackKind::Rotation {
        (1., "°")
    } else {
        (0.01, "")
    };
    let mut changed = false;
    accent::field_label(ui, "Значение");
    ui.horizontal(|ui| {
        for (axis, (letter, color)) in draft.axes.iter_mut().zip(AXES) {
            ui.label(egui::RichText::new(letter).strong().color(color));
            accent::values(ui, |ui| {
                let response = ui
                    .add(
                        egui::DragValue::new(axis)
                            .speed(0.02)
                            .max_decimals(4)
                            .suffix(unit),
                    )
                    .on_hover_text(if unit.is_empty() {
                        "Колёсико — шаг 0.01"
                    } else {
                        "Колёсико — шаг 1°"
                    });
                changed |= response.changed();
                changed |= accent::wheel_value(ui, &response, axis, step, f32::MIN..=f32::MAX);
            });
            ui.add_space(4.);
        }
    });
    ui.end_row();
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
    accent::field_label(ui, "Ключ");
    ui.horizontal(|ui| {
        let exists =
            times.get(times.partition_point(|&time| time < position.time)) == Some(&position.time);
        if ui
            .button(if exists {
                "Обновить ключ"
            } else {
                "＋ Добавить ключ"
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
    ui.end_row();
    ui.ctx().data_mut(|data| data.insert_temp(draft_id, draft));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_joint_continues_the_bone_or_follows_an_axis() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Animation_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let document = asset.document();
        let tip = NodeId(1);
        let worlds = document
            .view()
            .node_worlds(None, &ad_document::Cancellation::default())
            .unwrap();
        let world = |n: NodeId| glam::Mat4::from_cols_array_2d(&worlds[n.0].unwrap());
        let bone = world(tip).w_axis.truncate() - world(NodeId(0)).w_axis.truncate();
        // Along the bone, default length = the parent's bone.
        let (t, length) = default_child(document, tip, ChildDirection::Bone, None).unwrap();
        assert!((length - bone.length()).abs() < 1e-5);
        let end = world(tip).transform_point3(glam::Vec3::from_array(t));
        let expected = world(tip).w_axis.truncate() + bone;
        assert!(end.abs_diff_eq(expected, 1e-4), "{end} vs {expected}");
        // Along the parent's local Y with a chosen length.
        let (t, _) = default_child(document, tip, ChildDirection::Y, Some(0.5)).unwrap();
        let moved = world(tip).transform_vector3(glam::Vec3::from_array(t));
        assert!((moved.length() - 0.5).abs() < 1e-4);
        // Not under a mesh node; names are unique.
        let mesh_node = (0..document.nodes().len())
            .map(NodeId)
            .find(|n| !document.nodes()[n.0].meshes.is_empty())
            .unwrap();
        assert!(default_child(document, mesh_node, ChildDirection::Bone, None).is_none());
        // The bone to Tip is as long as Root→Tip; Root (a scene root) has none.
        let bone = bone_length(document, tip).unwrap();
        assert!(
            (bone - bone.max(0.)).abs() < 1e-6
                && (bone
                    - (world(tip).w_axis.truncate() - world(NodeId(0)).w_axis.truncate()).length())
                .abs()
                    < 1e-5
        );
        assert!(bone_length(document, NodeId(0)).is_none());
        let name = child_name(document, tip);
        assert_eq!(name, format!("{}_end", document.nodes()[tip.0].name));
        assert!(document.nodes().iter().all(|n| n.name != name));
    }
}
