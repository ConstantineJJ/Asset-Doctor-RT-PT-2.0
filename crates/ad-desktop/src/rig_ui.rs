use crate::imports::PreparedAsset;

pub(super) fn show(ui: &mut egui::Ui, asset: Option<&PreparedAsset>) {
    let Some(asset) = asset else {
        ui.label("Откройте GLB со скином.");
        return;
    };
    let document = asset.document();
    let Some(rig) = document.rig() else {
        ui.label("У этой модели нет скина.");
        return;
    };

    ui.weak("Исходная поза · только просмотр");
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
                ui.selectable_value(&mut selected, index, format!("{} · {}", index + 1, skin.name));
            }
        });
    ui.ctx()
        .data_mut(|data| data.insert_temp(selector_id, selected));

    let skin = &rig.skins[selected];
    let report = &rig.reports[selected];
    ui.label(format!("Суставов: {}", skin.joints.len()));
    if let Some(root) = skin.skeleton {
        ui.label(format!(
            "Корень скелета: {}",
            document.nodes()[root.0].name
        ));
    }
    ui.label(format!("Суставов без весов: {}", report.unused_joints.len()));
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
