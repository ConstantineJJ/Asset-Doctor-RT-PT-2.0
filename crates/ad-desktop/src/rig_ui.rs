use crate::{imports::PreparedAsset, rig_playback::Controls, viewer_ui::Actions};

pub(super) fn show(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    controls: Controls,
    actions: &mut Actions,
) {
    let Some(asset) = asset else {
        ui.label("Откройте GLB со скином или анимацией.");
        return;
    };
    let document = asset.document();
    let clips = document.animation_clips();
    if !clips.is_empty() {
        actions.playback = clip_controls(ui, clips, controls);
        ui.separator();
    }
    let Some(rig) = document.rig() else {
        ui.label("У этой модели нет скина.");
        return;
    };

    ui.weak("Скин · только просмотр");
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
                ui.add(
                    egui::Label::new(format!(
                        "{} · {}{}",
                        slot,
                        document.nodes()[node.0].name,
                        status
                    ))
                    .truncate(),
                )
                .on_hover_text(format!("Узел glTF №{}", node.0));
            }
        });
}

fn clip_controls(
    ui: &mut egui::Ui,
    clips: &[ad_document::AnimationClip],
    mut controls: Controls,
) -> Option<Controls> {
    let before = controls;
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
            ui.checkbox(&mut controls.looping, "Повтор");
        });
        if ui
            .add(
                egui::Slider::new(&mut controls.time, 0. ..=duration)
                    .fixed_decimals(2)
                    .suffix(" с"),
            )
            .changed()
        {
            controls.playing = false;
        }
    }
    (controls != before).then_some(controls)
}
