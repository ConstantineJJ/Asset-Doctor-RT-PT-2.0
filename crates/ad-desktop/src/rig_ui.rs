use crate::{
    accent::{self, Accent, pastel},
    imports::PreparedAsset,
    rig_playback::Controls,
    viewer_ui::Actions,
};
use ad_document::{
    AnimationClip, AnimationKey, AssetDocument, ChannelValue, ClipEdit, NodeId, TrackKind,
};

pub(super) fn show(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    mut controls: Controls,
    local_axes: &mut bool,
    skeleton_on: &mut bool,
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
                    animation_section(ui, document, &mut controls, local_axes, actions);
                });
            ui.separator();
            rig_section(ui, document, controls, actions);
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

/// Creating (yellow), the chosen clip (blue) and its keys (pink).
fn animation_section(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: &mut Controls,
    local_axes: &mut bool,
    actions: &mut Actions,
) {
    let clips = document.animation_clips();
    let clip = controls.clip.filter(|&clip| clip < clips.len());
    accent::scope(ui, Accent::New, |ui| {
        accent::heading(ui, Accent::New, "Новая анимация");
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
                .add_enabled(clip.is_some(), egui::Button::new("Дублировать клип"))
                .on_hover_text("Новый клип — копия выбранного")
                .clicked()
                && let Some(clip) = clip
            {
                actions.clip_edit = Some(duplicate_edit(&clips[clip], clip, ""));
            }
        });
    });
    let Some(clip) = clip else {
        ui.add_space(4.);
        ui.weak(if clips.is_empty() {
            "В модели нет клипов. Создайте клип, чтобы ставить ключи."
        } else {
            "Выберите клип в панели внизу вьюпорта, чтобы править его и ставить ключи."
        });
        return;
    };
    ui.add_space(4.);
    ui.add_enabled_ui(!controls.playing, |ui| {
        accent::scope(ui, Accent::Existing, |ui| {
            clip_properties(ui, document, clip, actions);
        });
        ui.add_space(2.);
        accent::scope(ui, Accent::Edit, |ui| {
            accent::heading(ui, Accent::Edit, "Ключи и поза");
            key_editor(ui, document, clip, controls, actions);
            ui.horizontal(|ui| {
                ui.label("Gizmo сустава:");
                ui.selectable_value(local_axes, true, "Локальные оси");
                ui.selectable_value(local_axes, false, "Мировые оси");
            })
            .response
            .on_hover_text("Кольца поворачивают выбранный сустав, стрелки двигают его; отпускание — ключ в текущем времени клипа (Undo отменит). Нужны выбранный клип и пауза.");
        });
    });
    if controls.playing {
        ui.weak("Идёт воспроизведение. Пауза (Пробел) — чтобы править ключи.");
    }
}

/// The skin report and the joint tree.
fn rig_section(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    controls: Controls,
    actions: &mut Actions,
) {
    let Some(rig) = document.rig() else {
        ui.label("У этой модели нет скина.");
        return;
    };
    ui.label(egui::RichText::new("Скин и веса").strong());
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

fn clip_properties(
    ui: &mut egui::Ui,
    document: &AssetDocument,
    clip: usize,
    actions: &mut Actions,
) {
    let active = &document.animation_clips()[clip];
    let id = egui::Id::new(("clip_properties", document.id().0));
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("Клип «{}»: имя, длительность", active.name))
            .strong()
            .color(Accent::Existing.text()),
    )
    .id_salt(id)
    .show(ui, |ui| {
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
