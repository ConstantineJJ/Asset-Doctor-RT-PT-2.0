//! Owner layout of 2026-11-21: top bar with drop-down menus (▼) and
//! collapsible sections, a lamp switch, the right block with tabs
//! («Здоровье», «Инфо», «Rigging» later), the scene tree under its button in
//! the viewport's top-left corner and Undo/Redo in its bottom-right corner.
use super::{Actions, HealthPage, Tab, TopMenu, ViewerUi};
use crate::diagnostic_ui::triangle;
use crate::imports::PreparedAsset;
use ad_render_vulkan::lighting::{ShadingMode, ViewLighting};
use egui::{Color32, RichText};

const DOWN: egui::Vec2 = egui::vec2(0., 1.);
const UP: egui::Vec2 = egui::vec2(0., -1.);
const RIGHT: egui::Vec2 = egui::vec2(1., 0.);
const LAMP_ON: Color32 = Color32::from_rgb(255, 210, 60);

impl ViewerUi {
    pub(super) fn top_bar(
        &mut self,
        root: &mut egui::Ui,
        width: f32,
        loading: bool,
        actions: &mut Actions,
    ) {
        self.menu_buttons.clear();
        #[cfg(test)]
        {
            self.menu_rects.clear();
            self.tab_rects.clear();
            self.page_rects.clear();
            self.lamp_handle_rects.clear();
            self.model_gizmo_rects.clear();
        }
        egui::Panel::top("tools")
            .exact_size(48.)
            .resizable(false)
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    if width >= 260. {
                        ui.label(
                            RichText::new("✚")
                                .size(23.)
                                .color(Color32::from_rgb(240, 68, 78)),
                        );
                    }
                    if width > 1450. {
                        ui.strong("ASSET DOCTOR");
                        ui.separator();
                    }
                    self.menu_button(ui, TopMenu::Settings, "Настройки");
                    if width >= 560. {
                        ui.separator();
                        self.menu_button(ui, TopMenu::View, "Вид");
                        self.menu_button(ui, TopMenu::Light, "Освещение");
                        self.menu_button(ui, TopMenu::Scene, "Сцена");
                        self.menu_button(ui, TopMenu::Model, "Модель");
                        self.lamp_switch(ui);
                    }
                    ui.separator();
                    let idle = !loading && self.edit.busy.is_none();
                    if width >= 900. {
                        let overwrite = ui
                            .add_enabled(
                                idle && self.edit.can_overwrite && self.edit.dirty,
                                egui::Button::new("Сохранить"),
                            )
                            .on_hover_text("Перезаписать рабочий файл проверенной версией · Ctrl+S. Первый раз спрашивает подтверждение; если файл изменён другой программой, перезаписи не будет.");
                        #[cfg(test)]
                        {
                            self.overwrite_rect = overwrite.rect;
                        }
                        actions.save |= overwrite.clicked();
                        let save = ui
                            .add_enabled(idle && self.edit.can_save, egui::Button::new("Сохранить копию…"))
                            .on_hover_text("Проверенная копия GLB в новый файл · Ctrl+Shift+S. Существующие файлы не перезаписываются; после сохранения можно сделать копию рабочим файлом.");
                        #[cfg(test)]
                        {
                            self.save_rect = save.rect;
                        }
                        actions.save_copy |= save.clicked();
                    }
                    let open = ui
                        .button("Открыть…")
                        .on_hover_text("Открыть статическую GLB-модель · Ctrl+O");
                    #[cfg(test)]
                    {
                        self.open_rect = open.rect;
                    }
                    actions.open |= open.clicked();
                    if self.edit.dirty && width >= 900. {
                        ui.colored_label(Color32::from_rgb(255, 190, 120), "* не сохранено")
                            .on_hover_text("Есть правки, которых нет ни в исходном файле, ни в последней проверенной копии.");
                    }
                    if width >= 330. {
                        self.help |= ui.button("Справка").clicked();
                    }
                    if width >= 1250. {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!loading, egui::Button::new("Очередь ремонтов…"))
                                .on_hover_text("Выполнить все доступные ремонты по очереди с проверкой после каждого шага.")
                                .clicked()
                            {
                                self.show_queue = true;
                            }
                        });
                    }
                });
            });
    }

    /// A top-bar button with a filled triangle that opens its menu.
    fn menu_button(&mut self, ui: &mut egui::Ui, menu: TopMenu, label: &str) {
        let open = self.menu == Some(menu);
        let response = ui.add(
            egui::Button::selectable(open, format!("{label}      ")).min_size(egui::vec2(0., 28.)),
        );
        let color = ui.visuals().strong_text_color();
        triangle(
            ui.painter(),
            egui::pos2(response.rect.right() - 12., response.rect.center().y),
            4.5,
            if open { UP } else { DOWN },
            color,
        );
        #[cfg(test)]
        {
            self.menu_rects.push((menu, response.rect));
        }
        self.menu_buttons.push(response.rect);
        if response.clicked() {
            self.menu = if open { None } else { Some(menu) };
        }
        if self.menu == Some(menu) {
            self.menu_anchor = response.rect;
        }
    }

    /// Yellow lamp: the lamp shines; grey and black: it is off. Click toggles.
    fn lamp_switch(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(30., 30.), egui::Sense::click());
        #[cfg(test)]
        {
            self.lamp_switch_rect = rect;
        }
        let on = self.lighting.enabled;
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, 5., ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let c = rect.center() - egui::vec2(0., 3.);
        if on {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
                if a.sin() > 0.5 {
                    continue; // no rays over the base
                }
                let v = egui::vec2(a.cos(), a.sin());
                painter.line_segment([c + v * 9., c + v * 12.], egui::Stroke::new(1.5, LAMP_ON));
            }
            painter.circle_filled(c, 7., LAMP_ON);
            painter.circle_stroke(
                c,
                7.,
                egui::Stroke::new(1., Color32::from_rgb(150, 110, 20)),
            );
        } else {
            painter.circle_filled(c, 7., Color32::from_rgb(24, 24, 26));
            painter.circle_stroke(
                c,
                7.,
                egui::Stroke::new(1.5, Color32::from_rgb(225, 225, 225)),
            );
        }
        let base = egui::Rect::from_min_max(c + egui::vec2(-3.5, 6.), c + egui::vec2(3.5, 11.));
        painter.rect_filled(base, 1., Color32::from_rgb(200, 200, 205));
        painter.line_segment(
            [base.left_center(), base.right_center()],
            egui::Stroke::new(1., Color32::from_rgb(90, 90, 95)),
        );
        let response = response.on_hover_text(if on {
            "Лампа включена. Нажмите, чтобы выключить (останутся заполнение и свет окружения)."
        } else {
            "Лампа выключена. Нажмите, чтобы включить."
        });
        if response.clicked() {
            self.lighting.enabled = !on;
        }
    }

    /// The open menu under its button. A press outside it (and outside its
    /// own pop-ups such as colour pickers) or Esc closes it.
    pub(super) fn menu_popup(
        &mut self,
        context: &egui::Context,
        height: f32,
        asset: Option<&PreparedAsset>,
        actions: &mut Actions,
    ) {
        let Some(menu) = self.menu else {
            return;
        };
        if context.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.menu = None;
            return;
        }
        let area = egui::Area::new(egui::Id::new("top_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.menu_anchor.left_bottom() + egui::vec2(0., 6.))
            .show(context, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(10.)
                    .show(ui, |ui| {
                        ui.set_width(350.);
                        egui::ScrollArea::vertical()
                            .max_height((height - 130.).max(160.))
                            .show(ui, |ui| match menu {
                                TopMenu::Settings => self.settings_menu(ui),
                                TopMenu::View => self.view_menu(ui),
                                TopMenu::Light => self.light_menu(ui, actions),
                                TopMenu::Scene => self.scene_menu(ui, actions),
                                TopMenu::Model => self.model_menu(ui, asset, actions),
                            });
                    });
            });
        let rect = area.response.rect;
        let press = context.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if let Some(pos) = press
            && !rect.contains(pos)
            && !self.menu_buttons.iter().any(|r| r.contains(pos))
            && !context.layer_id_at(pos).is_some_and(|layer| {
                matches!(layer.order, egui::Order::Foreground | egui::Order::Tooltip)
            })
        {
            self.menu = None;
        }
    }

    /// Collapsible section header with a filled triangle (▶ closed, ▼ open).
    fn section(&mut self, ui: &mut egui::Ui, key: &'static str, title: &str) -> bool {
        let open = self.sections.contains(key);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.), egui::Sense::click());
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, 4., ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = ui.visuals().strong_text_color();
        triangle(
            painter,
            rect.left_center() + egui::vec2(10., 0.),
            5.,
            if open { DOWN } else { RIGHT },
            color,
        );
        painter.text(
            rect.left_center() + egui::vec2(22., 0.),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(14.5),
            color,
        );
        if response.clicked() {
            if open {
                self.sections.remove(key);
            } else {
                self.sections.insert(key);
            }
        }
        self.sections.contains(key)
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        if self.section(ui, "controls", "Управление") {
            ui.indent("controls", |ui| {
                ui.weak(
                    "Клавиши и кнопки мыши, включая боковые: переназначение, конфликты, сброс.",
                );
                if ui.button("Клавиши и мышь…").clicked() {
                    self.show_controls = true;
                    self.menu = None;
                }
            });
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        if self.section(ui, "shading", "Шейдинг") {
            ui.indent("shading", |ui| {
                egui::ComboBox::from_id_salt("shading_mode")
                    .selected_text(match self.lighting.mode {
                        ShadingMode::BaseColor => "Базовый вид",
                        ShadingMode::DirectPbr => "PBR · направленный свет",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.lighting.mode, ShadingMode::BaseColor, "Базовый вид");
                        ui.selectable_value(&mut self.lighting.mode, ShadingMode::DirectPbr, "PBR · направленный свет");
                    });
                egui::ComboBox::from_label("Канал")
                    .selected_text(self.channel.label())
                    .show_ui(ui, |ui| {
                        for channel in ad_render_vulkan::lighting::Channel::ALL {
                            ui.selectable_value(&mut self.channel, channel, channel.label());
                        }
                    })
                    .response
                    .on_hover_text("Что показывать на модели: обычное освещение или один канал данных (нормали, UV, карты материала). Только вид.");
                ui.checkbox(&mut self.textures, "Базовые текстуры")
                    .on_hover_text("Включить выборку встроенных PNG/JPEG. Исходные материалы не изменяются.");
                let pbr = self.lighting.mode == ShadingMode::DirectPbr;
                if !pbr {
                    ui.weak("Карты материала действуют в режиме PBR.");
                }
                ui.add_enabled_ui(pbr, |ui| {
                    let _maps = ui.checkbox(&mut self.mr_maps, "Карты металла/шероховатости").on_hover_text("Включить карты параметров материала. Не зависит от переключателя базовых текстур; исходник не изменяется.");
                    #[cfg(test)]
                    {
                        self.mr_maps_rect = _maps.rect;
                    }
                    let _normal = ui.checkbox(&mut self.normal_maps, "Карты нормалей").on_hover_text("Детали поверхности из линейной RGB-карты. Источник и геометрия не изменяются.");
                    #[cfg(test)]
                    {
                        self.normal_maps_rect = _normal.rect;
                    }
                    let _ao = ui.checkbox(&mut self.ao_maps, "Окклюзия · AO").on_hover_text("Затемнение заполняющего света из карты материала. Направленный свет не меняется.");
                    #[cfg(test)]
                    {
                        self.ao_maps_rect = _ao.rect;
                    }
                    let _emission = ui.checkbox(&mut self.emission, "Свечение").on_hover_text("Собственный цвет поверхности из материала и карты. Без ореола и освещения соседних объектов.");
                    #[cfg(test)]
                    {
                        self.emission_rect = _emission.rect;
                    }
                });
            });
        }
        if self.section(ui, "grid", "Сетка и помощники") {
            ui.indent("grid", |ui| {
                ui.add_enabled(self.wireframe_supported, egui::Checkbox::new(&mut self.wireframe, "Топологическая сетка (каркас)"))
                    .on_hover_text("Рёбра треугольников поверх модели.")
                    .on_disabled_hover_text("Видеокарта не поддерживает отрисовку линиями (fillModeNonSolid).");
                ui.checkbox(&mut self.show_axes, "Оси").on_hover_text("Мировые оси из начала координат: X красная, Y зелёная, Z синяя; отрицательные половины тусклее.");
                ui.checkbox(&mut self.show_bounds, "Габариты").on_hover_text("Ограничивающий бокс модели и её размеры (ширина x высота x глубина, в единицах файла).");
            });
        }
    }

    fn light_menu(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        let pbr = self.lighting.mode == ShadingMode::DirectPbr;
        if self.section(ui, "lamp", "Лампа") {
            ui.indent("lamp", |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.lighting.enabled, "Лампа включена").on_hover_text("Выключенная лампа не светит; остаются заполнение и свет окружения.");
                    if ui.add(egui::Slider::new(&mut self.lamp_kelvin, 1500.0..=10000.0).text("K")).on_hover_text("Цветовая температура лампы: меньше — теплее, больше — холоднее.").changed() {
                        self.lighting.color = crate::lamp::kelvin_to_rgb(self.lamp_kelvin);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Цвет лампы");
                    ui.color_edit_button_rgb(&mut self.lighting.color);
                    ui.checkbox(&mut self.lamp_icon, "Значок лампы").on_hover_text("Значок лампы во вьюпорте: тяните его или его стрелки, чтобы повернуть свет; правая кнопка — меню.");
                });
                let (mut azimuth, mut elevation) = crate::lamp::angles(self.lighting.direction);
                let turned = ui.add(egui::Slider::new(&mut azimuth, -180.0..=180.0).text("Азимут")).changed();
                let raised = ui.add(egui::Slider::new(&mut elevation, -crate::lamp::MAX_ELEVATION..=crate::lamp::MAX_ELEVATION).text("Высота")).changed();
                if turned || raised {
                    self.lighting.direction = crate::lamp::from_angles(azimuth, elevation);
                }
                ui.horizontal(|ui| {
                    if ui.button("Сбросить положение").clicked() {
                        self.lighting.direction = ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION;
                    }
                    if ui.button("Свет сверху").clicked() {
                        self.lighting.direction = crate::lamp::from_angles(azimuth, crate::lamp::MAX_ELEVATION);
                    }
                    actions.lamp_from_camera |= ui.button("От камеры").clicked();
                });
            });
        }
        if self.section(ui, "shadows", "Тени") {
            ui.indent("shadows", |ui| {
                ui.checkbox(&mut self.shadows, "Тени").on_hover_text("Тень от лампы на модели и на полу. Только вид; при выключенной лампе теней нет.");
                ui.add_enabled(self.shadows, egui::Slider::new(&mut self.shadow_strength, 0.0..=1.0).text("Сила тени"));
            });
        }
        if self.section(ui, "exposure", "Экспозиция и сила света") {
            ui.indent("exposure", |ui| {
                if !pbr {
                    ui.weak("Действует в режиме PBR (меню «Вид»).");
                }
                ui.add_enabled_ui(pbr, |ui| {
                    egui::ComboBox::from_label("Пресет света").selected_text("Выбрать").show_ui(ui, |ui| {
                        for preset in crate::lamp::LightPreset::ALL {
                            if ui.selectable_label(false, preset.label()).clicked() {
                                preset.apply(&mut self.lighting);
                            }
                        }
                    }).response.on_hover_text("Задаёт лампу, основной свет, заполнение и экспозицию одним выбором. Окружение и режим не меняются.");
                    ui.add(egui::Slider::new(&mut self.lighting.exposure_ev, -6.0..=6.0).text("Экспозиция · EV"));
                    ui.add(egui::Slider::new(&mut self.lighting.key, 0.0..=8.0).text("Основной свет"));
                    ui.add(egui::Slider::new(&mut self.lighting.fill, 0.0..=1.0).text("Заполнение"));
                });
            });
        }
        if self.section(ui, "ambient", "Свет окружения") {
            ui.indent("ambient", |ui| {
                if !pbr {
                    ui.weak("Действует в режиме PBR (меню «Вид»).");
                }
                ui.add_enabled_ui(pbr, |ui| {
                    ui.checkbox(&mut self.environment_light, "Свет окружения").on_hover_text("Модель дополнительно освещается цветами выбранного окружения (небо сверху, земля снизу). Это приближение, не HDR/IBL.");
                    ui.add_enabled(self.environment_light, egui::Slider::new(&mut self.environment_strength, 0.0..=ad_render_vulkan::sky::EnvironmentLight::MAX_STRENGTH).text("Сила"));
                });
            });
        }
        ui.separator();
        let reset = ui.button("Сбросить освещение");
        #[cfg(test)]
        {
            self.reset_lighting_rect = reset.rect;
        }
        if reset.clicked() {
            self.lighting = ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            };
        }
    }

    fn scene_menu(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        if self.section(ui, "floor", "Пол") {
            ui.indent("floor", |ui| {
                for theme in ad_render_vulkan::grid::FloorTheme::ALL {
                    if ui.radio(self.floor == Some(theme), theme.label()).clicked() {
                        self.floor = Some(theme);
                        self.last_floor = theme;
                    }
                }
                if ui.radio(self.floor.is_none(), "Без поверхности").clicked() {
                    self.floor = None;
                }
                ui.weak("Пол на уровне нижней точки модели в исходном положении. Только вид. G — скрыть или вернуть.");
            });
        }
        if self.section(ui, "sky", "Небо и скайбокс") {
            ui.indent("sky", |ui| {
                egui::ComboBox::from_label("Окружение").selected_text(self.environment.label()).show_ui(ui, |ui| {
                    for preset in ad_render_vulkan::sky::Environment::ALL {
                        ui.selectable_value(&mut self.environment, preset, preset.label());
                    }
                }).response.on_hover_text("Фон-небо за моделью. Только вид: лампы, экспозиция и подсветка не меняются.");
                ui.horizontal(|ui| {
                    if ui.button("Скайбокс...").on_hover_text("Выбрать свою панораму PNG/JPEG 2:1 (равнопромежуточную). Файл только читается; в модель и экспорт не попадает.").clicked() {
                        actions.skybox_load = true;
                    }
                    if let Some(name) = self.skybox_name.clone() {
                        ui.checkbox(&mut self.skybox_on, name).on_hover_text("Показать панораму вместо фона-неба. Свет окружения берёт её цвета.");
                        if ui.button("Убрать").clicked() {
                            actions.skybox_remove = true;
                        }
                    }
                });
                if self.skybox_name.is_some() && self.skybox_on {
                    ui.add(egui::Slider::new(&mut self.skybox_yaw, 0.0..=360.0).text("Поворот").suffix(" град."));
                }
            });
        }
    }

    fn model_menu(
        &mut self,
        ui: &mut egui::Ui,
        asset: Option<&PreparedAsset>,
        actions: &mut Actions,
    ) {
        let Some(asset) = asset else {
            ui.label("Откройте модель.");
            return;
        };
        if self.section(ui, "gizmo", "Гизмо") {
            ui.indent("gizmo", |ui| {
                ui.checkbox(&mut self.model_gizmo_on, "Гизмо модели").on_hover_text("Стрелки X/Y/Z сдвигают модель, кольца поворачивают её вокруг Y и X, квадрат масштабирует равномерно — всё от нижней точки центра. Каждое отпускание — одна правка: Ctrl+Z отменит, «Сохранить» запишет в файл.");
                if ui.add_enabled(!self.placement.is_identity(), egui::Button::new("Сбросить положение")).clicked() {
                    actions.model_reset = true;
                }
            });
        }
        if self.section(ui, "place", "Положение, поворот, масштаб") {
            ui.indent("place", |ui| {
                let snapshot = asset.snapshot();
                self.placement_fields_ui(ui, snapshot.bounds, snapshot.source_bounds, actions);
            });
        }
    }

    pub(super) fn right_panel(
        &mut self,
        root: &mut egui::Ui,
        asset: Option<&PreparedAsset>,
        loading: bool,
        actions: &mut Actions,
    ) {
        egui::Panel::right("side")
            .default_size(390.)
            .min_size(330.)
            .max_size(570.)
            .show(root, |ui| {
                ui.add_space(4.);
                ui.horizontal(|ui| {
                    for (tab, label) in [(Tab::Health, "Здоровье"), (Tab::Info, "Инфо")]
                    {
                        let response = ui.add(
                            egui::Button::selectable(
                                self.tab == tab,
                                RichText::new(label).size(15.),
                            )
                            .min_size(egui::vec2(0., 30.)),
                        );
                        #[cfg(test)]
                        {
                            self.tab_rects.push((tab, response.rect));
                        }
                        if response.clicked() {
                            self.tab = tab;
                        }
                    }
                    ui.add_enabled(
                        false,
                        egui::Button::selectable(false, RichText::new("Rigging").size(15.))
                            .min_size(egui::vec2(0., 30.)),
                    )
                    .on_disabled_hover_text(
                        "Риггинг (скелет и скиннинг) — этап P7, пока не реализован.",
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Скрыть")
                            .on_hover_text("Скрыть правую панель")
                            .clicked()
                        {
                            self.show_summary = false;
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Health => self.health_tab(ui, asset, loading, actions),
                        Tab::Info => self.info_tab(ui, asset),
                    });
            });
    }

    fn health_tab(
        &mut self,
        ui: &mut egui::Ui,
        asset: Option<&PreparedAsset>,
        loading: bool,
        actions: &mut Actions,
    ) {
        ui.horizontal(|ui| {
            for (page, label) in [
                (HealthPage::Analysis, "Анализ и диагностика"),
                (HealthPage::Repair, "Ремонт"),
            ] {
                let response = ui.selectable_label(self.health_page == page, label);
                #[cfg(test)]
                {
                    self.page_rects.push((page, response.rect));
                }
                if response.clicked() {
                    self.health_page = page;
                }
            }
        });
        ui.separator();
        let can_preview = !loading && self.edit.busy.is_none();
        let mut buttons = Vec::new();
        let preview = match self.health_page {
            HealthPage::Analysis => crate::diagnostic_ui::analysis(
                ui,
                asset,
                &mut self.carousel,
                can_preview,
                &mut buttons,
            ),
            HealthPage::Repair => {
                let preview = crate::diagnostic_ui::repairs(ui, asset, can_preview, &mut buttons);
                ui.separator();
                let queue = ui
                    .add_enabled(!loading, egui::Button::new("Очередь ремонтов…"))
                    .on_hover_text("Выполнить все доступные ремонты по очереди с проверкой после каждого шага.");
                #[cfg(test)]
                {
                    self.queue_rect = queue.rect;
                }
                if queue.clicked() {
                    self.show_queue = true;
                }
                actions.history_open |= ui
                    .button("История ремонта…")
                    .on_hover_text("Прошлые ремонты и сохранения из всех запусков программы. Это не проверка текущей модели.")
                    .clicked();
                ui.weak("Каждый ремонт сначала показывает предпросмотр; модель меняется только после «Применить». Undo/Redo — в правом нижнем углу вьюпорта.");
                preview
            }
        };
        if preview.is_some() {
            actions.preview = preview;
        }
        #[cfg(test)]
        {
            self.preview_rect = buttons.first().map_or(egui::Rect::NOTHING, |r| r.1);
            self.preview_rects = buttons;
        }
    }

    fn info_tab(&mut self, ui: &mut egui::Ui, asset: Option<&PreparedAsset>) {
        let Some(asset) = asset else {
            ui.label("Откройте модель, чтобы увидеть её состав и размеры.");
            ui.add_space(16.);
            ui.weak("Статические GLB: PNG/JPEG, цвета вершин, нормали, OPAQUE/MASK. PBR — предварительный режим. BLEND, скелеты и анимации пока не поддерживаются.");
            return;
        };
        ui.strong(asset.document().name());
        ui.label(format!(
            "Исходный файл: {:.2} МиБ",
            asset.document().source().len() as f64 / 1048576.
        ));
        ui.label(format!(
            "Узлов в документе: {}",
            asset.document().nodes().len()
        ));
        let textures = asset.document().texture_stats();
        ui.label(format!("Изображений материалов: {}", textures.images));
        ui.label(format!(
            "RGBA с mip-уровнями: {:.2} МиБ",
            textures.mip_bytes as f64 / 1048576.
        ));
        ui.add_space(12.);
        ui.strong("Отображаемая геометрия");
        ui.label(format!("Вершины: {}", asset.snapshot().positions.len()));
        ui.label(format!(
            "Треугольники: {}",
            asset.snapshot().indices.len() / 3
        ));
        ui.label(format!(
            "Примитивы с повторами: {}",
            asset.snapshot().draws.len()
        ));
        ui.label(format!("Нормаль грани: {} вершин", asset.snapshot().fallback_normal_vertices)).on_hover_text("Авторская нормаль отсутствует, нулевая или преобразование вырождено. Источник не исправляется.");
        if asset.snapshot().fallback_tangent_vertices > 0 {
            ui.label(format!("UV-основа нормалей: {} вершин", asset.snapshot().fallback_tangent_vertices)).on_hover_text("Нет пригодных авторских касательных. Используется основа по производным геометрии/UV; на сложных швах возможно отличие от MikkTSpace.");
        }
        ui.weak("С учётом экземпляров в сцене");
        let b = asset.snapshot().bounds;
        ui.add_space(12.);
        ui.strong("Габариты · метры");
        for (i, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.label(format!("{axis}   {:.3}", b.max[i] - b.min[i]));
        }
        if !self.placement.is_identity() {
            let f = crate::model_gizmo::Fields::of(self.placement, asset.snapshot().source_bounds);
            ui.add_space(12.);
            ui.strong("Положение модели");
            ui.label(format!(
                "Сдвиг: {:.3}, {:.3}, {:.3}",
                f.offset[0], f.offset[1], f.offset[2]
            ));
            ui.label(format!(
                "Поворот: Y {:.1} град., X {:.1} град.",
                f.yaw, f.pitch
            ));
            ui.label(format!("Масштаб: x{:.4}", f.scale));
        }
        if let Some(id) = self.tree.selected
            && let Some(node) = asset.document().nodes().get(id.0)
        {
            ui.separator();
            ui.strong("Выбранный узел");
            if let Some(row) = self.tree.rows.iter().find(|row| row.id == id) {
                ui.label(&row.label);
            }
            ui.label(format!("Дочерние узлы: {}", node.children.len()));
            ui.label(format!("Ссылки на примитивы: {}", node.meshes.len()));
            ui.collapsing("Исходная локальная матрица", |ui| {
                for r in 0..4 {
                    ui.monospace(format!(
                        "{:.3}  {:.3}  {:.3}  {:.3}",
                        node.local[0][r], node.local[1][r], node.local[2][r], node.local[3][r]
                    ));
                }
            });
        }
        ui.collapsing("Ограничения просмотра", |ui| {
            for note in asset.document().notes() {
                ui.weak(note);
            }
        });
        ui.add_space(16.);
        ui.separator();
        ui.weak("Металл и шероховатость: параметры × карты. Заполнение приблизительное; без HDR-окружения.");
        ui.weak("Статические GLB: PNG/JPEG, цвета вершин, нормали, OPAQUE/MASK. PBR — предварительный режим. BLEND, скелеты и анимации пока не поддерживаются.");
    }

    /// «Дерево» in the viewport's top-left corner; the tree unfolds below it.
    pub(super) fn tree_overlay(&mut self, context: &egui::Context) {
        let scene = self.scene;
        if scene.width() < 160. || scene.height() < 120. {
            return;
        }
        egui::Area::new(egui::Id::new("tree_button"))
            .order(egui::Order::Foreground)
            .fixed_pos(scene.min + egui::vec2(10., 10.))
            .show(context, |ui| {
                let response = ui.add(
                    egui::Button::selectable(self.show_tree, "      Дерево")
                        .min_size(egui::vec2(100., 28.)),
                );
                triangle(
                    ui.painter(),
                    egui::pos2(response.rect.left() + 14., response.rect.center().y),
                    5.,
                    if self.show_tree { DOWN } else { RIGHT },
                    ui.visuals().strong_text_color(),
                );
                #[cfg(test)]
                {
                    self.tree_rect = response.rect;
                }
                if response
                    .on_hover_text("Структура сцены: узлы активной сцены")
                    .clicked()
                {
                    self.show_tree = !self.show_tree;
                }
            });
        if !self.show_tree {
            return;
        }
        let rows_height = (scene.height() - 110.).clamp(80., 560.);
        egui::Area::new(egui::Id::new("tree_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(scene.min + egui::vec2(10., 44.))
            .show(context, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(8.)
                    .show(ui, |ui| {
                        ui.set_width(280.);
                        ui.strong("Сцена");
                        ui.weak("Узлы активной сцены");
                        ui.separator();
                        if self.tree.rows.is_empty() {
                            ui.label("Модель не открыта");
                        }
                        egui::ScrollArea::vertical()
                            .max_height(rows_height)
                            .show_rows(ui, 24., self.tree.rows.len(), |ui, range| {
                                for index in range {
                                    let row = &self.tree.rows[index];
                                    ui.horizontal(|ui| {
                                        ui.add_space((row.depth.min(12) * 12) as f32);
                                        let label = if row.label.is_empty() {
                                            "Без имени"
                                        } else {
                                            &row.label
                                        };
                                        let response = ui.add_sized(
                                            [ui.available_width(), 24.],
                                            egui::Button::selectable(
                                                self.tree.selected == Some(row.id),
                                                label,
                                            )
                                            .truncate(),
                                        );
                                        if response.on_hover_text(label).clicked() {
                                            self.tree.selected = Some(row.id);
                                        }
                                    });
                                }
                            });
                    });
            });
    }

    /// Undo / Redo with half-circle arrows in the viewport's bottom-right
    /// corner; a «Панель» button brings back a hidden right block.
    pub(super) fn corner_overlay(
        &mut self,
        context: &egui::Context,
        width: f32,
        loading: bool,
        actions: &mut Actions,
    ) {
        let scene = self.scene;
        if scene.width() < 200. || scene.height() < 80. {
            return;
        }
        let idle = !loading && self.edit.busy.is_none();
        egui::Area::new(egui::Id::new("undo_redo"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(scene.right_bottom() - egui::vec2(10., 10.))
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    let undo = history_button(ui, idle && self.edit.can_undo, "Undo", false)
                        .on_hover_text(
                            "Отменить последнюю правку · Ctrl+Z. Ревизия всё равно увеличится.",
                        );
                    let redo = history_button(ui, idle && self.edit.can_redo, "Redo", true)
                        .on_hover_text("Повторить отменённую правку · Ctrl+Y");
                    #[cfg(test)]
                    {
                        self.undo_rect = undo.rect;
                        self.redo_rect = redo.rect;
                    }
                    actions.undo |= undo.clicked();
                    actions.redo |= redo.clicked();
                });
            });
        if !self.show_summary && width >= 680. {
            egui::Area::new(egui::Id::new("show_side"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(scene.right_top() + egui::vec2(-10., 10.))
                .show(context, |ui| {
                    if ui
                        .button("Панель")
                        .on_hover_text("Показать правую панель")
                        .clicked()
                    {
                        self.show_summary = true;
                    }
                });
        }
    }
}

/// A button with a half-circle arrow: left for Undo, right (mirrored) for Redo.
fn history_button(ui: &mut egui::Ui, enabled: bool, label: &str, redo: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(egui::vec2(88., 32.)),
    );
    let color = if enabled {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let painter = ui.painter();
    let rect = response.rect;
    let (icon, text, align) = if redo {
        (
            egui::pos2(rect.right() - 18., rect.center().y + 2.),
            rect.left_center() + egui::vec2(12., 0.),
            egui::Align2::LEFT_CENTER,
        )
    } else {
        (
            egui::pos2(rect.left() + 18., rect.center().y + 2.),
            rect.right_center() - egui::vec2(12., 0.),
            egui::Align2::RIGHT_CENTER,
        )
    };
    let flip = if redo { -1. } else { 1. };
    let r = 6.5;
    // Upper half circle from the far side to the near side, ending in a
    // head that points down: ↶ for Undo, ↷ for Redo.
    let points: Vec<egui::Pos2> = (0..=14)
        .map(|i| {
            let a = std::f32::consts::PI * i as f32 / 14.;
            icon + egui::vec2(a.cos() * r * flip, -a.sin() * r)
        })
        .collect();
    let end = *points.last().expect("arc points");
    painter.add(egui::Shape::line(points, egui::Stroke::new(2., color)));
    triangle(painter, end + egui::vec2(0., 2.), 4.5, DOWN, color);
    painter.text(text, align, label, egui::FontId::proportional(14.5), color);
    response
}
