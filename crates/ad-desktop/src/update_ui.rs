//! ADR-0049 update window and the «Обновления» part of «Настройки».
use super::{Actions, ViewerUi};
use crate::updates::Phase;
use egui::RichText;

fn megabytes(bytes: u64) -> String {
    format!("{:.1} МБ", bytes as f64 / (1024. * 1024.))
}

impl ViewerUi {
    /// «Настройки → Обновления».
    pub(super) fn update_settings(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.label(format!(
            "Установлена версия v{}.",
            env!("CARGO_PKG_VERSION")
        ));
        if ui
            .button("Проверить обновления…")
            .on_hover_text(
                "Спросить GitHub, есть ли новая версия. Устанавливается только после вашего «Да».",
            )
            .clicked()
        {
            actions.update_check = true;
            self.menu = None;
        }
        ui.checkbox(&mut self.update.check_on_start, "Проверять при запуске")
            .on_hover_text("Один тихий запрос к релизам программы на GitHub при каждом запуске.");
    }

    pub(super) fn update_window(&mut self, context: &egui::Context, actions: &mut Actions) {
        if !self.update.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Обновление")
            .id(egui::Id::new("update_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.)
            .show(context, |ui| {
                ui.label(format!("Установлена версия v{}.", env!("CARGO_PKG_VERSION")));
                ui.separator();
                match self.update.phase.clone() {
                    Phase::Idle => {
                        if ui.button("Проверить обновления").clicked() {
                            actions.update_check = true;
                        }
                    }
                    Phase::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Проверяю релизы на GitHub…");
                        });
                    }
                    Phase::UpToDate => {
                        ui.label("Это последняя версия.");
                        if ui.button("Проверить ещё раз").clicked() {
                            actions.update_check = true;
                        }
                    }
                    Phase::Available(release) => {
                        ui.label(RichText::new(format!("Доступна версия {}", release.tag)).strong().size(15.));
                        if !release.name.is_empty() && release.name != release.tag {
                            ui.label(&release.name);
                        }
                        if release.zip_size > 0 {
                            ui.weak(format!("Размер загрузки: {}", megabytes(release.zip_size)));
                        }
                        if !release.notes.trim().is_empty() {
                            ui.add_space(4.);
                            egui::ScrollArea::vertical().max_height(220.).show(ui, |ui| {
                                ui.label(release.notes.trim());
                            });
                        }
                        if !release.page_url.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&release.page_url).weak().small()).selectable(true));
                        }
                        ui.add_space(6.);
                        ui.weak("Программа скачает пакет, проверит SHA-256, закроется, заменит файлы и запустится снова. Если новая версия не запустится, вернётся прежняя.");
                        ui.horizontal(|ui| {
                            let install = ui.add_enabled(self.update.can_install, egui::Button::new("Скачать и установить"));
                            let install = if self.update.can_install {
                                install
                            } else {
                                install.on_disabled_hover_text("Сначала сохраните модель или отмените правки: при обновлении программа закроется.")
                            };
                            if install.clicked() {
                                actions.update_install = true;
                            }
                            if ui.button("Позже").clicked() {
                                self.update.open = false;
                            }
                        });
                    }
                    Phase::Downloading { done, total } => {
                        ui.label("Скачиваю и проверяю обновление…");
                        let progress = if total > 0 { done as f32 / total as f32 } else { 0. };
                        ui.add(egui::ProgressBar::new(progress.clamp(0., 1.)).text(if total > 0 {
                            format!("{} из {}", megabytes(done), megabytes(total))
                        } else {
                            megabytes(done)
                        }));
                        if ui.button("Отменить").clicked() {
                            actions.update_cancel = true;
                        }
                    }
                    Phase::Installing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Программа закрывается для обновления и запустится снова…");
                        });
                    }
                    Phase::Failed(error) => {
                        ui.colored_label(egui::Color32::from_rgb(240, 170, 90), format!("Не получилось: {error}"));
                        if ui.button("Повторить проверку").clicked() {
                            actions.update_check = true;
                        }
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.update.check_on_start, "Проверять при запуске");
            });
        if !open {
            self.update.open = false;
        }
    }
}
