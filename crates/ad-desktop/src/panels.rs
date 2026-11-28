//! Owner layout of 2026-11-21: top bar with drop-down menus (▼) without
//! nested foldable groups, a lamp switch, the right block with tabs
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
/// Height of the light grey tool strip under the top bar.
const STRIP_HEIGHT: f32 = 26.;
/// Colour marks of the Work-Tree (and fixed outline colours): red, orange,
/// yellow, green, cyan, blue, magenta.
pub(crate) const MARK_COLORS: [[u8; 3]; 7] = [
    [235, 64, 64],
    [245, 150, 40],
    [240, 220, 50],
    [70, 200, 90],
    [60, 200, 220],
    [70, 120, 240],
    [210, 80, 220],
];
/// Outline colours: the seven mark colours (the indices kept in view
/// settings stay valid), then bright «acid» ones (owner 2026-11-25).
pub(crate) const OUTLINE_COLORS: [[u8; 3]; 18] = [
    [235, 64, 64],
    [245, 150, 40],
    [240, 220, 50],
    [70, 200, 90],
    [60, 200, 220],
    [70, 120, 240],
    [210, 80, 220],
    [57, 255, 20],   // neon green
    [204, 255, 0],   // acid yellow-green
    [255, 255, 0],   // electric yellow
    [255, 95, 31],   // neon orange
    [255, 7, 58],    // neon red
    [255, 20, 147],  // hot pink
    [255, 0, 255],   // magenta
    [191, 0, 255],   // electric purple
    [0, 255, 255],   // electric cyan
    [0, 150, 255],   // electric blue
    [255, 255, 255], // white
];
/// View menus of the top bar, left to right (ADR-0045 amendment).
const VIEW_MENUS: [(TopMenu, &str, &str); 8] = [
    (TopMenu::View, "Вид", "Вид"),
    (TopMenu::Grid, "Сетка и помощники", "Сетка"),
    (TopMenu::Light, "Освещение", "Освещение"),
    (TopMenu::Lamp, "Лампа", "Лампа"),
    (TopMenu::Exposure, "Экспозиция и сила света", "Экспозиция"),
    (TopMenu::Scene, "Сцена", "Сцена"),
    (TopMenu::Sky, "Окружение", "Окружение"),
    (TopMenu::Model, "Модель", "Модель"),
];

impl ViewerUi {
    pub(super) fn top_bar(
        &mut self,
        root: &mut egui::Ui,
        width: f32,
        loading: bool,
        read_only: bool,
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
                    if width > 1850. {
                        ui.strong("ASSET DOCTOR");
                        ui.separator();
                    }
                    let _ = self.menu_button(ui, TopMenu::Settings, "Настройки");
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
                        .on_hover_text("Открыть GLB-модель · Ctrl+O");
                    #[cfg(test)]
                    {
                        self.open_rect = open.rect;
                    }
                    actions.open |= open.clicked();
                    if self.edit.dirty && width >= 900. {
                        ui.colored_label(Color32::from_rgb(255, 190, 120), "* не сохранено")
                            .on_hover_text("Есть правки, которых нет ни в исходном файле, ни в последней проверенной копии.");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if width >= 1400.
                            && ui
                                .add_enabled(
                                    !loading && !read_only,
                                    egui::Button::new("Очередь ремонтов…"),
                                )
                                .on_hover_text("Выполнить все доступные ремонты по очереди с проверкой после каждого шага.")
                                .clicked()
                        {
                            self.show_queue = true;
                        }
                        if width >= 560. {
                            self.view_menus(ui, width < 2100.);
                        }
                    });
                });
            });
    }

    /// The view menus and the lamp switch, in the right part of the top bar:
    /// their right edge meets the viewport's right edge (last frame) when
    /// there is room, otherwise they follow «Очередь ремонтов…».
    /// `short`: narrow windows use the short names (full name on hover).
    fn view_menus(&mut self, ui: &mut egui::Ui, short: bool) {
        let free = ui.available_rect_before_wrap();
        let edge = self.scene.right() - 8.;
        if self.scene.is_positive()
            && edge < free.right()
            && edge - self.menu_group_width > free.left() + 8.
        {
            ui.add_space(free.right() - edge);
        }
        let start = ui.available_rect_before_wrap().right();
        self.amoeba_switch(ui);
        self.lamp_switch(ui);
        for (menu, label, brief) in VIEW_MENUS.into_iter().rev() {
            if menu == TopMenu::Sky {
                self.skybox_button(ui);
            }
            let response = self.menu_button(ui, menu, if short { brief } else { label });
            if short && brief != label {
                response.on_hover_text(label);
            }
        }
        self.menu_group_width = start - ui.available_rect_before_wrap().right();
    }

    /// «SkyBox» opens or closes the SkyBox window (and lists its folder the
    /// first time).
    fn skybox_button(&mut self, ui: &mut egui::Ui) {
        let response = ui
            .add(egui::Button::selectable(self.show_skybox, "SkyBox").min_size(egui::vec2(0., 28.)))
            .on_hover_text("Выбор панорамы-фона с превью (PNG/JPEG/HDR), поворот и качество.");
        #[cfg(test)]
        {
            self.skybox_button_rect = response.rect;
        }
        self.menu_buttons.push(response.rect);
        if response.clicked() {
            self.show_skybox = !self.show_skybox;
            self.menu = None;
            if self.show_skybox && self.skybox_library.folder.is_none() {
                let folder = self
                    .skybox_folder
                    .clone()
                    .or_else(|| {
                        self.skybox_path
                            .as_ref()
                            .and_then(|p| p.parent().map(Into::into))
                    })
                    .or_else(crate::skybox_library::default_folder);
                if let Some(folder) = folder {
                    self.skybox_library.open(folder);
                }
            }
        }
    }

    /// A top-bar button with a filled triangle that opens its menu.
    fn menu_button(&mut self, ui: &mut egui::Ui, menu: TopMenu, label: &str) -> egui::Response {
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
        response
    }

    /// The amoeba: object outline on/off (Tab). Coloured like the outline
    /// (several colours for the dynamic contrast), grey when off.
    fn amoeba_switch(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(30., 30.), egui::Sense::click());
        #[cfg(test)]
        {
            self.amoeba_rect = rect;
        }
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, 5., ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let c = rect.center();
        let n = 28;
        let points: Vec<egui::Pos2> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let r = 9. + 1.8 * (3. * a).sin() + 1.1 * (5. * a + 1.).cos();
                c + egui::vec2(a.cos(), a.sin()) * r
            })
            .collect();
        let fixed = self
            .outline_color
            .map(|i| OUTLINE_COLORS[i % OUTLINE_COLORS.len()]);
        if self.outline_on {
            let fill = fixed.map_or(
                Color32::from_rgba_unmultiplied(255, 255, 255, 40),
                |[r, g, b]| Color32::from_rgba_unmultiplied(r, g, b, 70),
            );
            painter.add(egui::Shape::convex_polygon(
                points.clone(),
                fill,
                egui::Stroke::NONE,
            ));
        }
        // Dashed outline: every other segment; the dynamic mode cycles colours.
        for i in (0..n).step_by(2) {
            let color = match (self.outline_on, fixed) {
                (false, _) => ui.visuals().weak_text_color(),
                (true, Some([r, g, b])) => Color32::from_rgb(r, g, b),
                (true, None) => {
                    let [r, g, b] = MARK_COLORS[(i / 2) % MARK_COLORS.len()];
                    Color32::from_rgb(r, g, b)
                }
            };
            painter.line_segment(
                [points[i], points[(i + 1) % n]],
                egui::Stroke::new(1.8, color),
            );
        }
        let response = response.on_hover_text(if self.outline_on {
            "Контур объектов включён (Tab): пунктир — под курсором, сплошной — выбранный. Цвет — в «Настройках»."
        } else {
            "Контур объектов выключен (Tab)."
        });
        if response.clicked() {
            self.outline_on = !self.outline_on;
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
        // One area per menu, sized by its own content every frame: no size
        // remembered from another menu and no inner scrolling.
        let _ = height;
        let area = egui::Area::new(egui::Id::new(("top_menu", menu)))
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(self.menu_anchor.left_bottom() + egui::vec2(0., 6.))
            .show(context, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(10.)
                    .show(ui, |ui| {
                        ui.set_width(350.);
                        match menu {
                            TopMenu::Settings => self.settings_menu(ui, actions),
                            TopMenu::View => self.view_menu(ui),
                            TopMenu::Grid => self.grid_menu(ui),
                            TopMenu::Light => self.light_menu(ui),
                            TopMenu::Lamp => self.lamp_menu(ui, actions),
                            TopMenu::Exposure => self.exposure_menu(ui),
                            TopMenu::Scene => self.scene_menu(ui),
                            TopMenu::Sky => self.sky_menu(ui),
                            TopMenu::Model => self.model_menu(ui, asset, actions),
                        }
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

    /// A plain (not foldable) group title inside a menu.
    fn heading(ui: &mut egui::Ui, title: &str) {
        ui.add_space(4.);
        ui.label(RichText::new(title).strong().size(14.5));
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        Self::heading(ui, "Управление");
        ui.weak("Клавиши и кнопки мыши, включая боковые: переназначение, конфликты, сброс.");
        if ui.button("Клавиши и мышь…").clicked() {
            self.show_controls = true;
            self.menu = None;
        }
        ui.separator();
        Self::heading(ui, "Контур объектов");
        self.outline_controls(ui);
        ui.separator();
        Self::heading(ui, "Справка");
        if ui
            .button("Справка…")
            .on_hover_text("Клавиши, мышь и что где находится · H")
            .clicked()
        {
            self.help = true;
            self.menu = None;
        }
        ui.separator();
        Self::heading(ui, "Окно");
        ui.horizontal(|ui| {
            use super::WindowMode as W;
            ui.selectable_value(&mut self.window_mode, W::Normal, "Окно")
                .on_hover_text("Обычное окно");
            ui.selectable_value(&mut self.window_mode, W::Fullscreen, "Полный экран")
                .on_hover_text("Без рамки поверх всего · F11 — переключить, Esc — вернуть окно");
        });
        ui.checkbox(&mut self.frameless, "Окно без рамки и заголовка (F1)")
            .on_hover_text("Убирает рамку и верхнюю полосу с кнопками свернуть/развернуть/закрыть; F1 — вернуть.");
        self.ui_scale_controls(ui);
        ui.separator();
        Self::heading(ui, "Анимация");
        ui.checkbox(
            &mut self.autoplay,
            "Запускать первый клип при открытии модели",
        )
        .on_hover_text("Модель с анимацией сразу проигрывает свой первый клип; Пробел — пауза.");
        ui.separator();
        Self::heading(ui, "Обновления");
        self.update_settings(ui, actions);
        ui.separator();
        let exit = ui
            .button("Выход")
            .on_hover_text("Закрыть программу; о несохранённых правках будет вопрос");
        #[cfg(test)]
        {
            self.exit_rect = exit.rect;
        }
        if exit.clicked() {
            actions.exit = true;
            self.menu = None;
        }
    }

    /// Interface scale 75–200 % (text, buttons, panels; not the 3D view):
    /// a slider in 5 % steps (the mouse wheel too) and «100 %».
    pub(super) fn ui_scale_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut percent = (self.ui_scale * 100.).round();
            let response = ui
                .add(
                    egui::Slider::new(&mut percent, 75.0..=200.0)
                        .step_by(5.)
                        .suffix(" %")
                        .text("Масштаб интерфейса"),
                )
                .on_hover_text("Размер текста, кнопок и панелей · колёсико — шаг 5 %");
            crate::accent::wheel_value(ui, &response, &mut percent, 5., 75.0..=200.0);
            if ui
                .add_enabled(percent != 100., egui::Button::new("100 %"))
                .clicked()
            {
                percent = 100.;
            }
            self.ui_scale = percent / 100.;
        });
    }

    /// Outline switch, width and colour («Настройки» and the right-click
    /// menu): dynamic contrast or one of OUTLINE_COLORS.
    pub(super) fn outline_controls(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.outline_on, "Показывать контур (Tab)");
        let width = ui
            .add(
                egui::Slider::new(&mut self.outline_width, ad_render_vulkan::outline::WIDTHS)
                    .step_by(1.)
                    .suffix(" px")
                    .text("Толщина"),
            )
            .on_hover_text("Толщина линии контура в пикселях экрана: 1–32 · колёсико — шаг 1.");
        crate::accent::wheel_value(
            ui,
            &width,
            &mut self.outline_width,
            1.,
            ad_render_vulkan::outline::WIDTHS,
        );
        ui.radio_value(&mut self.outline_color, None, "Динамический контраст")
            .on_hover_text("Линия инвертирует то, что под ней: видна на любом фоне.");
        ui.label("Цвет:");
        for row in OUTLINE_COLORS.chunks(9).enumerate() {
            ui.horizontal(|ui| {
                for (k, [r, g, b]) in row.1.iter().copied().enumerate() {
                    let i = row.0 * 9 + k;
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::click());
                    ui.painter()
                        .rect_filled(rect.shrink(1.), 3., Color32::from_rgb(r, g, b));
                    if self.outline_color == Some(i) {
                        ui.painter().rect_stroke(
                            rect,
                            3.,
                            egui::Stroke::new(2., ui.visuals().strong_text_color()),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        self.outline_color = Some(i);
                    }
                }
            });
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("shading_mode")
            .selected_text(match self.lighting.mode {
                ShadingMode::BaseColor => "Базовый вид",
                ShadingMode::DirectPbr => "PBR · направленный свет",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.lighting.mode,
                    ShadingMode::BaseColor,
                    "Базовый вид",
                );
                ui.selectable_value(
                    &mut self.lighting.mode,
                    ShadingMode::DirectPbr,
                    "PBR · направленный свет",
                );
            });
        let channel = egui::ComboBox::from_label("Канал")
            .selected_text(self.channel.label())
            .show_ui(ui, |ui| {
                for channel in ad_render_vulkan::lighting::Channel::ALL {
                    ui.selectable_value(&mut self.channel, channel, channel.label());
                }
            })
            .response
            .on_hover_text("Что показывать на модели: обычное освещение или один канал данных (нормали, UV, карты материала). Только вид · колёсико — соседний.");
        let all = ad_render_vulkan::lighting::Channel::ALL;
        let at = all.iter().position(|c| *c == self.channel).unwrap_or(0);
        self.channel = all[crate::accent::wheel_choice(ui, &channel, at, all.len())];
        ui.checkbox(&mut self.textures, "Базовые текстуры")
            .on_hover_text(
                "Включить выборку встроенных PNG/JPEG. Исходные материалы не изменяются.",
            );
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
    }

    fn grid_menu(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled(
            self.wireframe_supported,
            egui::Checkbox::new(&mut self.wireframe, "Топологическая сетка (каркас)"),
        )
        .on_hover_text("Рёбра треугольников поверх модели.")
        .on_disabled_hover_text("Видеокарта не поддерживает отрисовку линиями (fillModeNonSolid).");
        ui.checkbox(&mut self.show_axes, "Оси").on_hover_text("Мировые оси из начала координат: X красная, Y зелёная, Z синяя; отрицательные половины тусклее.");
        ui.checkbox(&mut self.show_bounds, "Габариты").on_hover_text("Ограничивающий бокс модели и её размеры (ширина x высота x глубина, в единицах файла).");
        ui.checkbox(&mut self.skeleton_on, "Скелет").on_hover_text("Кости и суставы скина поверх модели в текущей позе. Клик по суставу выбирает его во вкладке Rigging.");
    }

    fn light_menu(&mut self, ui: &mut egui::Ui) {
        let pbr = self.lighting.mode == ShadingMode::DirectPbr;
        Self::heading(ui, "Тени");
        ui.checkbox(&mut self.shadows, "Тени").on_hover_text(
            "Тень от лампы на модели и на полу. Только вид; при выключенной лампе теней нет.",
        );
        let strength = ui.add_enabled(
            self.shadows,
            egui::Slider::new(&mut self.shadow_strength, 0.0..=1.0).text("Сила тени"),
        );
        crate::accent::wheel_value(ui, &strength, &mut self.shadow_strength, 0.05, 0.0..=1.0);
        ui.separator();
        Self::heading(ui, "Свет окружения");
        if !pbr {
            ui.weak("Действует в режиме PBR (меню «Вид»).");
        }
        ui.add_enabled_ui(pbr, |ui| {
            ui.checkbox(&mut self.environment_light, "Свет окружения").on_hover_text("С показанной панорамой (скайбокс PNG/JPEG или HDR) модель освещается ею целиком: рассеянный свет и отражения с учётом шероховатости (IBL). Без панорамы — приближение цветами выбранного окружения (небо сверху, земля снизу).");
            let range = 0.0..=ad_render_vulkan::sky::EnvironmentLight::MAX_STRENGTH;
            let strength = ui.add_enabled(self.environment_light, egui::Slider::new(&mut self.environment_strength, range.clone()).text("Сила"));
            crate::accent::wheel_value(ui, &strength, &mut self.environment_strength, 0.1, range);
        });
        ui.separator();
        let reset = ui.button("Сбросить освещение").on_hover_text(
            "Лампа, экспозиция, основной свет и заполнение — по умолчанию, режим PBR.",
        );
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

    fn lamp_menu(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.checkbox(&mut self.lighting.enabled, "Лампа включена")
            .on_hover_text("Выключенная лампа не светит; остаются заполнение и свет окружения.");
        let kelvin = ui
            .add(egui::Slider::new(&mut self.lamp_kelvin, 1500.0..=10000.0).text("Температура, K"))
            .on_hover_text("Цветовая температура лампы: меньше — теплее, больше — холоднее · колёсико — шаг 100 K.");
        if kelvin.changed()
            | crate::accent::wheel_value(ui, &kelvin, &mut self.lamp_kelvin, 100., 1500.0..=10000.0)
        {
            self.lighting.color = crate::lamp::kelvin_to_rgb(self.lamp_kelvin);
        }
        ui.horizontal(|ui| {
            ui.label("Цвет лампы");
            ui.color_edit_button_rgb(&mut self.lighting.color);
            ui.checkbox(&mut self.lamp_icon, "Значок лампы").on_hover_text("Значок лампы во вьюпорте: тяните его или его стрелки, чтобы повернуть свет; правая кнопка — меню.");
        });
        let distance = ui.add(
            egui::Slider::new(&mut self.lighting.distance, ad_render_vulkan::lighting::LAMP_DISTANCES)
                .text("Расстояние"),
        )
        .on_hover_text("Расстояние от центра модели в её размерах (радиусах). Ближе — ярче и с более резким перепадом света; тень расходится от лампы. 2,5 — как прежняя лампа.");
        crate::accent::wheel_value(
            ui,
            &distance,
            &mut self.lighting.distance,
            0.1,
            ad_render_vulkan::lighting::LAMP_DISTANCES,
        );
        let (mut azimuth, mut elevation) = crate::lamp::angles(self.lighting.direction);
        let turn = ui.add(egui::Slider::new(&mut azimuth, -180.0..=180.0).text("Азимут"));
        let turned = turn.changed()
            | crate::accent::wheel_value(ui, &turn, &mut azimuth, 5., -180.0..=180.0);
        let heights = -crate::lamp::MAX_ELEVATION..=crate::lamp::MAX_ELEVATION;
        let raise = ui.add(egui::Slider::new(&mut elevation, heights.clone()).text("Высота"));
        let raised =
            raise.changed() | crate::accent::wheel_value(ui, &raise, &mut elevation, 5., heights);
        if turned || raised {
            self.lighting.direction = crate::lamp::from_angles(azimuth, elevation);
        }
        ui.horizontal(|ui| {
            if ui.button("Сбросить положение").clicked() {
                self.lighting.direction = ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION;
            }
            if ui.button("Свет сверху").clicked() {
                self.lighting.direction =
                    crate::lamp::from_angles(azimuth, crate::lamp::MAX_ELEVATION);
            }
            actions.lamp_from_camera |= ui.button("От камеры").clicked();
        });
    }

    fn exposure_menu(&mut self, ui: &mut egui::Ui) {
        let pbr = self.lighting.mode == ShadingMode::DirectPbr;
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
            let l = &mut self.lighting;
            let r = ui.add(egui::Slider::new(&mut l.exposure_ev, -6.0..=6.0).text("Экспозиция · EV"));
            crate::accent::wheel_value(ui, &r, &mut l.exposure_ev, 0.25, -6.0..=6.0);
            let r = ui.add(egui::Slider::new(&mut l.key, 0.0..=8.0).text("Основной свет"));
            crate::accent::wheel_value(ui, &r, &mut l.key, 0.1, 0.0..=8.0);
            let r = ui.add(egui::Slider::new(&mut l.fill, 0.0..=1.0).text("Заполнение"));
            crate::accent::wheel_value(ui, &r, &mut l.fill, 0.05, 0.0..=1.0);
        });
    }

    fn scene_menu(&mut self, ui: &mut egui::Ui) {
        Self::heading(ui, "Пол");
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
    }

    fn sky_menu(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_label("Погода и время суток")
            .selected_text(self.environment.label())
            .show_ui(ui, |ui| {
                for preset in ad_render_vulkan::sky::Environment::ALL {
                    ui.selectable_value(&mut self.environment, preset, preset.label());
                }
            })
            .response
            .on_hover_text(
                "Фон-небо за моделью. Только вид: лампы, экспозиция и подсветка не меняются.",
            );
        if self.skybox_on && self.skybox_name.is_some() {
            ui.weak("Сейчас показан SkyBox; небо видно, когда SkyBox скрыт.");
        }
    }

    /// The SkyBox window: preview, the shown panorama and its settings, and
    /// the panoramas of a folder with their previews.
    pub(super) fn skybox_window(&mut self, context: &egui::Context, actions: &mut Actions) {
        if !self.show_skybox {
            return;
        }
        self.skybox_library.poll(context);
        let mut open = true;
        egui::Window::new("SkyBox")
            .id(egui::Id::new("skybox_window"))
            .open(&mut open)
            .resizable(true)
            .default_size([560., 620.])
            .show(context, |ui| {
                let width = ui.available_width();
                // Large preview: the hovered panorama, else the shown one.
                let shown = self
                    .skybox_path
                    .as_ref()
                    .and_then(|p| self.skybox_library.entries.iter().position(|e| &e.path == p));
                let preview = self.skybox_hover.or(shown);
                let preview_size = egui::vec2(width, (width / 2.).min(280.));
                match preview.and_then(|i| self.skybox_library.entries.get(i)) {
                    Some(crate::skybox_library::Entry {
                        thumb: crate::skybox_library::Thumb::Ready(texture),
                        name,
                        ..
                    }) => {
                        ui.add(egui::Image::new((texture.id(), preview_size)).corner_radius(6.));
                        ui.weak(name);
                    }
                    _ => {
                        let (rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
                        ui.painter().rect_filled(rect, 6., ui.visuals().extreme_bg_color);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Наведите на панораму, чтобы увидеть её здесь",
                            egui::FontId::proportional(14.),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                ui.separator();
                if let Some(name) = self.skybox_name.clone() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.skybox_on, format!("Показать: {name}"))
                            .on_hover_text("Показать панораму вместо неба «Окружения». Со «Светом окружения» она освещает модель.");
                        if ui.button("Убрать").clicked() {
                            actions.skybox_remove = true;
                        }
                    });
                    if self.skybox_on {
                        let yaw = ui.add(
                            egui::Slider::new(&mut self.skybox_yaw, 0.0..=360.0)
                                .text("Поворот")
                                .suffix(" град."),
                        );
                        crate::accent::wheel_value(ui, &yaw, &mut self.skybox_yaw, 5., 0.0..=360.0);
                    }
                } else {
                    ui.weak("SkyBox не выбран: показано небо «Окружения».");
                }
                self.skybox_quality_ui(ui, actions);
                ui.separator();
                ui.horizontal(|ui| {
                    let folder = self
                        .skybox_library
                        .folder
                        .as_ref()
                        .map_or_else(|| "Папка не выбрана".to_string(), |f| f.display().to_string());
                    if ui.button("Папка…").on_hover_text("Выбрать папку с панорамами: укажите любой файл в ней.").clicked() {
                        actions.skybox_folder_choose = true;
                    }
                    if ui.add_enabled(self.skybox_library.folder.is_some(), egui::Button::new("Обновить")).clicked()
                        && let Some(folder) = self.skybox_library.folder.clone()
                    {
                        self.skybox_library.open(folder);
                    }
                    actions.skybox_load |= ui
                        .button("Другой файл…")
                        .on_hover_text("Открыть одну панораму PNG/JPEG/HDR из любого места.")
                        .clicked();
                    ui.add(egui::Label::new(egui::RichText::new(folder).weak()).truncate());
                });
                if let Some(error) = &self.skybox_library.error {
                    ui.colored_label(Color32::from_rgb(255, 190, 120), error);
                }
                let pending = self.skybox_library.pending();
                if pending > 0 {
                    ui.weak(format!("Готовятся превью: {pending}"));
                }
                self.skybox_hover = None;
                let tile = egui::vec2(168., 84.);
                let columns = ((ui.available_width() + 8.) / (tile.x + 8.)).floor().max(1.) as usize;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let entries = self.skybox_library.entries.len();
                    for row in (0..entries).step_by(columns) {
                        ui.horizontal(|ui| {
                            for i in row..(row + columns).min(entries) {
                                let entry = &self.skybox_library.entries[i];
                                let current = self.skybox_path.as_ref() == Some(&entry.path);
                                let response = ui
                                    .vertical(|ui| {
                                        ui.set_width(tile.x);
                                        let (rect, response) = ui.allocate_exact_size(tile, egui::Sense::click());
                                        match &entry.thumb {
                                            crate::skybox_library::Thumb::Ready(texture) => {
                                                egui::Image::new((texture.id(), tile))
                                                    .corner_radius(4.)
                                                    .paint_at(ui, rect);
                                            }
                                            crate::skybox_library::Thumb::Pending => {
                                                ui.painter().rect_filled(rect, 4., ui.visuals().extreme_bg_color);
                                            }
                                            crate::skybox_library::Thumb::Failed(_) => {
                                                ui.painter().rect_filled(rect, 4., Color32::from_rgb(70, 40, 40));
                                            }
                                        }
                                        let stroke = if current {
                                            egui::Stroke::new(2.5, Color32::from_rgb(255, 205, 80))
                                        } else if response.hovered() {
                                            egui::Stroke::new(1.5, ui.visuals().strong_text_color())
                                        } else {
                                            egui::Stroke::NONE
                                        };
                                        ui.painter().rect_stroke(rect, 4., stroke, egui::StrokeKind::Inside);
                                        ui.add(egui::Label::new(egui::RichText::new(&entry.name).small()).truncate());
                                        response
                                    })
                                    .inner;
                                #[cfg(test)]
                                {
                                    self.skybox_tile_rects.push(response.rect);
                                }
                                let hint = match &entry.thumb {
                                    crate::skybox_library::Thumb::Failed(error) => error.clone(),
                                    _ => format!("{} · {:.1} МБ · нажмите, чтобы показать", entry.name, entry.bytes as f64 / 1048576.),
                                };
                                let response = response.on_hover_text(hint);
                                if response.hovered() {
                                    self.skybox_hover = Some(i);
                                }
                                if response.clicked() && !current {
                                    actions.skybox_open = Some(entry.path.clone());
                                }
                            }
                        });
                    }
                    if entries == 0 && self.skybox_library.folder.is_some() {
                        ui.weak("В папке нет панорам PNG/JPEG/HDR.");
                    }
                });
            });
        self.show_skybox &= open;
    }

    /// SkyBox quality: widest level kept on the GPU; changing it prepares
    /// the shown panorama again.
    fn skybox_quality_ui(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        let before = self.skybox_width;
        ui.horizontal(|ui| {
            ui.label("Качество SkyBox");
            for (width, label) in ad_render_vulkan::ibl::WIDTHS.into_iter().zip(["2K", "4K", "8K"]) {
                ui.radio_value(&mut self.skybox_width, width, label);
            }
        })
        .response
        .on_hover_text("Ширина панорамы на видеокарте. 4K — чёткий фон при умеренной памяти (около 90 МБ видеопамяти); 8K — самый чёткий, около 360 МБ видеопамяти и до 0,7 ГБ памяти во время подготовки. Освещение от этого почти не меняется.");
        if self.skybox_width != before
            && let Some(path) = self.skybox_path.clone()
        {
            actions.skybox_reload = Some(path);
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
        if asset.document().is_read_only() {
            ui.label("Модель со скином или анимацией открыта только для просмотра.");
            return;
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.model_gizmo_on, "Гизмо модели").on_hover_text("Стрелки X/Y/Z сдвигают модель, кольца поворачивают её вокруг Y и X, квадрат масштабирует равномерно — всё от нижней точки центра. Каждое отпускание — одна правка: Ctrl+Z отменит, «Сохранить» запишет в файл.");
            if ui.add_enabled(!self.placement.is_identity(), egui::Button::new("Сбросить положение")).clicked() {
                actions.model_reset = true;
            }
        });
        ui.separator();
        Self::heading(ui, "Положение, поворот, масштаб");
        let snapshot = asset.snapshot();
        self.placement_fields_ui(ui, snapshot.bounds, snapshot.source_bounds, actions);
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
                    for (tab, label) in [
                        (Tab::Health, "Здоровье"),
                        (Tab::Info, "Инфо"),
                        (Tab::Rigging, "Rigging"),
                    ] {
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
                if self.tab == Tab::Rigging {
                    ui.add_enabled_ui(self.edit.busy.is_none(), |ui| {
                        super::rig_ui::show(
                            ui,
                            asset,
                            self.playback,
                            &mut self.joint_local_axes,
                            &mut self.joint_gizmo_size,
                            &mut self.skeleton_on,
                            &mut self.root_path,
                            &mut self.fit,
                            actions,
                        );
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.tab == Tab::Health {
                                self.health_tab(ui, asset, loading, actions);
                            } else {
                                self.info_tab(ui, asset);
                            }
                        });
                }
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
        let read_only = asset.is_some_and(|asset| asset.document().is_read_only());
        let can_preview = !read_only && !loading && self.edit.busy.is_none();
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
                    .add_enabled(!read_only && !loading, egui::Button::new("Очередь ремонтов…"))
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
            ui.weak("Статические GLB: PNG/JPEG, цвета вершин, нормали, OPAQUE/MASK. PBR — предварительный режим. Скины и клипы GLB доступны во вкладке Rigging. BLEND и морф-цели пока не поддерживаются.");
            return;
        };
        use crate::accent::{pastel, stat};
        ui.strong(asset.document().name());
        stat(
            ui,
            "Исходный файл:",
            format!("{:.2}", asset.document().source().len() as f64 / 1048576.),
            "МиБ",
            pastel::LEMON,
        );
        stat(
            ui,
            "Узлов в документе:",
            asset.document().nodes().len().to_string(),
            "",
            pastel::LAVENDER,
        );
        let textures = asset.document().texture_stats();
        stat(
            ui,
            "Изображений материалов:",
            textures.images.to_string(),
            "",
            pastel::PEACH,
        );
        stat(
            ui,
            "RGBA с mip-уровнями:",
            format!("{:.2}", textures.mip_bytes as f64 / 1048576.),
            "МиБ",
            pastel::LEMON,
        );
        ui.add_space(12.);
        ui.strong("Отображаемая геометрия");
        stat(
            ui,
            "Вершины:",
            group_digits(asset.snapshot().positions.len()),
            "",
            pastel::SKY,
        );
        stat(
            ui,
            "Треугольники:",
            group_digits(asset.snapshot().indices.len() / 3),
            "",
            pastel::MINT,
        );
        stat(
            ui,
            "Вызовы отрисовки (примитивы с повторами):",
            group_digits(asset.snapshot().draws.len()),
            "",
            pastel::PEACH,
        );
        stat(
            ui,
            "Нормаль грани:",
            group_digits(asset.snapshot().fallback_normal_vertices),
            "вершин",
            pastel::ROSE,
        )
        .on_hover_text("Авторская нормаль отсутствует, нулевая или преобразование вырождено. Источник не исправляется.");
        if asset.snapshot().fallback_tangent_vertices > 0 {
            stat(
                ui,
                "UV-основа нормалей:",
                group_digits(asset.snapshot().fallback_tangent_vertices),
                "вершин",
                pastel::ROSE,
            )
            .on_hover_text("Нет пригодных авторских касательных. Используется основа по производным геометрии/UV; на сложных швах возможно отличие от MikkTSpace.");
        }
        ui.weak("С учётом экземпляров в сцене");
        let b = asset.snapshot().bounds;
        ui.add_space(12.);
        ui.strong("Габариты · метры");
        for (i, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            stat(
                ui,
                axis,
                format!("{:.3}", b.max[i] - b.min[i]),
                "",
                [pastel::ROSE, pastel::MINT, pastel::SKY][i],
            );
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
        let count = self.tree.all_selected().len();
        if count > 1 {
            ui.separator();
            ui.strong(format!("Выбрано объектов: {count}"));
            ui.weak("Shift + клик — добавить, Ctrl + клик — убрать, рамка — выбрать область. Ниже — последний выбранный.");
        }
        if let Some(id) = self.tree.selected
            && let Some(node) = asset.document().nodes().get(id.0)
        {
            ui.separator();
            ui.strong("Выбранный узел");
            if let Some(row) = self.tree.rows.iter().find(|row| row.id == id) {
                ui.label(&row.label);
            }
            stat(
                ui,
                "Дочерние узлы:",
                node.children.len().to_string(),
                "",
                pastel::LAVENDER,
            );
            stat(
                ui,
                "Ссылки на примитивы:",
                node.meshes.len().to_string(),
                "",
                pastel::PEACH,
            );
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
        ui.weak("Металл и шероховатость: параметры × карты. Заполнение приблизительное. Свет окружения от панорамы (IBL) — в меню «Освещение» при показанном скайбоксе.");
        ui.weak("Статические GLB: PNG/JPEG, цвета вершин, нормали, OPAQUE/MASK. PBR — предварительный режим. Скины и клипы GLB доступны во вкладке Rigging. BLEND и морф-цели пока не поддерживаются.");
    }

    /// Right-click menu: the object under the pointer (gizmo, delete, hide,
    /// mark, find in the Work-Tree) and the view submenus. Esc or a press
    /// outside closes it.
    pub(super) fn context_menu_ui(
        &mut self,
        context: &egui::Context,
        asset: Option<&PreparedAsset>,
        actions: &mut Actions,
    ) {
        let Some((at, node)) = self.context_menu else {
            return;
        };
        if context.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.context_menu = None;
            return;
        }
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("viewport_context_menu"))
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(at)
            .show(context, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.);
                    if let (Some(node), Some(asset)) = (node, asset) {
                        let name = asset
                            .document()
                            .nodes()
                            .get(node.0)
                            .map_or(String::new(), |n| n.name.clone());
                        ui.label(
                            RichText::new(if name.is_empty() {
                                "Без имени"
                            } else {
                                &name
                            })
                            .strong(),
                        );
                        let editable = !asset.document().is_read_only();
                        if ui
                            .add_enabled(
                                editable,
                                egui::Button::new("Gizmo: переместить, повернуть, масштаб"),
                            )
                            .clicked()
                        {
                            if !self.tree.is_selected(node) {
                                self.tree.select_only(Some(node));
                            }
                            self.object_gizmo = Some(node);
                            self.model_gizmo_on = true;
                            close = true;
                        }
                        if ui
                            .add_enabled(editable, egui::Button::new("Удалить (X)"))
                            .clicked()
                        {
                            actions.delete_object = Some(node);
                            close = true;
                        }
                        if ui
                            .button("Скрыть")
                            .on_hover_text("Только вид; вернуть — глазом в Work-Tree")
                            .clicked()
                        {
                            self.tree.hidden.insert(node.0);
                            self.tree.rev += 1;
                            close = true;
                        }
                        ui.menu_button("Пометить цветом", |ui| {
                            ui.horizontal(|ui| {
                                for color in MARK_COLORS {
                                    let (r, response) = ui.allocate_exact_size(
                                        egui::vec2(20., 20.),
                                        egui::Sense::click(),
                                    );
                                    ui.painter().rect_filled(
                                        r,
                                        3.,
                                        Color32::from_rgb(color[0], color[1], color[2]),
                                    );
                                    if response.clicked() {
                                        self.tree.tints.insert(node.0, color);
                                        self.tree.rev += 1;
                                        close = true;
                                    }
                                }
                            });
                            if ui.button("Без пометки").clicked() {
                                self.tree.tints.remove(&node.0);
                                self.tree.rev += 1;
                                close = true;
                            }
                        });
                        if ui.button("Показать в Work-Tree").clicked() {
                            self.show_tree = true;
                            close = true;
                        }
                        ui.separator();
                    }
                    if self.object_gizmo.is_some() && ui.button("Gizmo: вся модель").clicked()
                    {
                        // The gizmo follows the selection: clear it.
                        self.tree.select_only(None);
                        self.object_gizmo = None;
                        close = true;
                    }
                    if ui.button("Вписать (F)").clicked() {
                        actions.fit = true;
                        close = true;
                    }
                    ui.menu_button("Контур объектов", |ui| {
                        self.outline_controls(ui)
                    });
                    ui.separator();
                    ui.menu_button("Вид", |ui| self.view_menu(ui));
                    ui.menu_button("Сетка и помощники", |ui| self.grid_menu(ui));
                    ui.menu_button("Освещение", |ui| self.light_menu(ui));
                    ui.menu_button("Лампа", |ui| self.lamp_menu(ui, actions));
                    ui.menu_button("Окружение", |ui| self.sky_menu(ui));
                });
            });
        let rect = area.response.rect;
        let press = context.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if close
            || press.is_some_and(|pos| {
                !rect.contains(pos)
                    && !context.layer_id_at(pos).is_some_and(|layer| {
                        matches!(layer.order, egui::Order::Foreground | egui::Order::Tooltip)
                    })
            })
        {
            self.context_menu = None;
        }
    }

    /// A bug in the bottom-right corner opens the program log; amber when
    /// new warnings came since the log was last open.
    pub(super) fn bug_button(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(26., 22.), egui::Sense::click());
        #[cfg(test)]
        {
            self.bug_rect = rect;
        }
        let (_, warnings) = crate::log::counts();
        let fresh = warnings > self.log_seen_warnings;
        let color = if fresh {
            Color32::from_rgb(255, 190, 90)
        } else if response.hovered() || self.show_log {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let painter = ui.painter();
        let c = rect.center() + egui::vec2(0., 1.);
        let stroke = egui::Stroke::new(1.3, color);
        for dy in [-3.5f32, 0., 3.5] {
            painter.line_segment(
                [c + egui::vec2(-3., dy), c + egui::vec2(-8., dy - 2.)],
                stroke,
            );
            painter.line_segment(
                [c + egui::vec2(3., dy), c + egui::vec2(8., dy - 2.)],
                stroke,
            );
        }
        painter.add(egui::Shape::ellipse_filled(
            c + egui::vec2(0., 1.),
            egui::vec2(4.5, 6.),
            color,
        ));
        painter.circle_filled(c + egui::vec2(0., -6.), 2.8, color);
        painter.line_segment(
            [c + egui::vec2(-1.5, -8.), c + egui::vec2(-4., -10.5)],
            stroke,
        );
        painter.line_segment(
            [c + egui::vec2(1.5, -8.), c + egui::vec2(4., -10.5)],
            stroke,
        );
        let response = response.on_hover_text(if fresh {
            "Журнал работы программы: есть новые предупреждения"
        } else {
            "Журнал работы программы"
        });
        if response.clicked() {
            self.show_log = !self.show_log;
        }
    }

    /// The program log: every diagnostic line and status message, with a
    /// filter, copy, clear and writing to a file.
    pub(super) fn log_window(&mut self, context: &egui::Context) {
        if !self.show_log {
            return;
        }
        let (lines, total, warnings) = crate::log::snapshot();
        self.log_seen_warnings = warnings;
        let mut open = true;
        egui::Window::new("Log")
            .id(egui::Id::new("program_log"))
            .open(&mut open)
            .resizable(true)
            .default_size([760., 420.])
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Строк: {total} · предупреждений: {warnings}"));
                    ui.separator();
                    ui.add(egui::TextEdit::singleline(&mut self.log_filter).hint_text("Фильтр").desired_width(180.));
                    ui.checkbox(&mut self.log_only_warnings, "Только предупреждения");
                });
                ui.horizontal(|ui| {
                    let before = self.log_to_file;
                    ui.checkbox(&mut self.log_to_file, "Писать в файл")
                        .on_hover_text("Каждый запуск — новый файл в %LOCALAPPDATA%\\AssetDoctorRTPT\\logs. Включение запоминается.");
                    if self.log_to_file != before {
                        let target = self.log_to_file.then(crate::log::default_folder).flatten();
                        if let Err(error) = crate::log::write_to(target) {
                            self.log_to_file = false;
                            self.set_status(format!("Журнал не пишется в файл: {error}"), true);
                        }
                    }
                    if let Some(path) = crate::log::file() {
                        ui.add(egui::Label::new(egui::RichText::new(path.display().to_string()).weak()).truncate());
                    }
                });
                let filter = self.log_filter.to_lowercase();
                let shown: Vec<&crate::log::Line> = lines
                    .iter()
                    .filter(|l| !self.log_only_warnings || l.warning)
                    .filter(|l| filter.is_empty() || l.text.to_lowercase().contains(&filter))
                    .collect();
                ui.horizontal(|ui| {
                    if ui.button("Копировать").on_hover_text("Показанные строки — в буфер обмена").clicked() {
                        let text: Vec<String> = shown.iter().map(|l| format!("{:10.3} {}", l.at, l.text)).collect();
                        ui.ctx().copy_text(text.join("\n"));
                    }
                    if ui.button("Очистить").on_hover_text("Убрать строки из окна (файл не меняется)").clicked() {
                        crate::log::clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, 16., shown.len(), |ui, range| {
                        for line in &shown[range] {
                            let text = egui::RichText::new(format!("{:9.3}  {}", line.at, line.text)).monospace().size(12.);
                            ui.label(if line.warning { text.color(Color32::from_rgb(255, 190, 120)) } else { text });
                        }
                    });
            });
        self.show_log &= open;
    }

    /// Light grey strip under the top bar (room for future tools) with the
    /// black «Work-Tree» button that opens the left block. It is part of the
    /// background, like the panels (owner 2026-11-28): drawn in the root
    /// layer without taking room, so windows and menus always cover it; the
    /// camera ignores the pointer over it (`camera_at`).
    pub(super) fn work_tree_strip(&mut self, root: &mut egui::Ui, width: f32) {
        let strip = egui::Rect::from_min_size(
            egui::pos2(0., 48.),
            egui::vec2((width * 0.405).max(160.), STRIP_HEIGHT),
        );
        self.strip_rect = strip;
        root.painter()
            .rect_filled(strip, 0., Color32::from_rgb(196, 198, 204));
        let mut child = root.new_child(
            egui::UiBuilder::new()
                .id_salt("work_tree_strip")
                .max_rect(strip.shrink2(egui::vec2(6., 2.))),
        );
        {
            let ui = &mut child;
            {
                {
                    ui.horizontal_centered(|ui| {
                        let text = RichText::new("      Work-Tree").color(Color32::from_rgb(235, 235, 240)).size(13.);
                        let fill = if self.show_tree {
                            Color32::from_rgb(110, 36, 44)
                        } else {
                            Color32::from_rgb(18, 18, 22)
                        };
                        let response = ui.add(egui::Button::new(text).fill(fill).min_size(egui::vec2(110., STRIP_HEIGHT - 6.)));
                        triangle(
                            ui.painter(),
                            egui::pos2(response.rect.left() + 13., response.rect.center().y),
                            4.5,
                            if self.show_tree { DOWN } else { RIGHT },
                            Color32::from_rgb(235, 235, 240),
                        );
                        #[cfg(test)]
                        {
                            self.tree_rect = response.rect;
                        }
                        if response
                            .on_hover_text("Work-Tree: структура сцены — узлы, порядок, вложенность; скрыть объект или пометить его цветом (только вид)")
                            .clicked()
                        {
                            self.show_tree = !self.show_tree;
                        }
                    });
                }
            }
        }
    }

    /// The left block (mirror of the right one): the scene tree with hide
    /// and colour-mark switches per node. View only; children follow.
    pub(super) fn work_tree_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("work_tree")
            .default_size(390.)
            .min_size(330.)
            .max_size(570.)
            .show(root, |ui| {
                ui.add_space(STRIP_HEIGHT + 4.);
                ui.horizontal(|ui| {
                    ui.strong("Work-Tree");
                    ui.weak(format!("узлов: {}", self.tree.rows.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.tree.hidden.is_empty() || !self.tree.tints.is_empty(), egui::Button::new("Сбросить").small())
                            .on_hover_text("Показать всё и снять цветные пометки")
                            .clicked()
                        {
                            self.tree.hidden.clear();
                            self.tree.tints.clear();
                            self.tree.rev += 1;
                        }
                    });
                });
                ui.weak("Глаз — скрыть или показать, квадрат — цветная пометка. Вложенные узлы следуют за родителем. Только вид: в файл не попадает.");
                ui.separator();
                if self.tree.rows.is_empty() {
                    ui.label("Модель не открыта");
                }
                let rows = self.tree.rows.len();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 24., rows, |ui, range| {
                    for index in range {
                        let (id, depth, label) = {
                            let row = &self.tree.rows[index];
                            (row.id, row.depth, row.label.clone())
                        };
                        ui.horizontal(|ui| {
                            ui.set_min_height(24.);
                            ui.add_space((depth.min(12) * 12) as f32);
                            let hidden = self.tree.hidden.contains(&id.0);
                            let (eye, response) = ui.allocate_exact_size(egui::vec2(20., 20.), egui::Sense::click());
                            paint_eye(ui.painter(), eye.center(), !hidden, if response.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().text_color() });
                            if response.on_hover_text(if hidden { "Показать" } else { "Скрыть" }).clicked() {
                                if hidden {
                                    self.tree.hidden.remove(&id.0);
                                } else {
                                    self.tree.hidden.insert(id.0);
                                }
                                self.tree.rev += 1;
                            }
                            let tint = self.tree.tints.get(&id.0).copied();
                            let (square, response) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::click());
                            match tint {
                                Some([r, g, b]) => ui.painter().rect_filled(square.shrink(2.), 3., Color32::from_rgb(r, g, b)),
                                None => ui.painter().rect_stroke(square.shrink(3.), 3., egui::Stroke::new(1., ui.visuals().weak_text_color()), egui::StrokeKind::Inside),
                            };
                            let response = response.on_hover_text("Цветная пометка");
                            egui::Popup::menu(&response).show(|ui| {
                                ui.horizontal(|ui| {
                                    for color in MARK_COLORS {
                                        let (r, response) = ui.allocate_exact_size(egui::vec2(20., 20.), egui::Sense::click());
                                        ui.painter().rect_filled(r, 3., Color32::from_rgb(color[0], color[1], color[2]));
                                        if response.clicked() {
                                            self.tree.tints.insert(id.0, color);
                                            self.tree.rev += 1;
                                        }
                                    }
                                    if ui.small_button("Без пометки").clicked() {
                                        self.tree.tints.remove(&id.0);
                                        self.tree.rev += 1;
                                    }
                                });
                            });
                            let name = if label.is_empty() { "Без имени".to_string() } else { label };
                            let text = if hidden { RichText::new(&name).weak() } else { RichText::new(&name) };
                            let response = ui.add_sized(
                                [ui.available_width(), 22.],
                                egui::Button::selectable(self.tree.is_selected(id), text).truncate(),
                            );
                            if response.on_hover_text(&name).clicked() {
                                // Shift adds, Ctrl removes, a plain click selects only this node.
                                let mods = ui.input(|i| i.modifiers);
                                if mods.ctrl {
                                    self.tree.select_remove(id);
                                } else if mods.shift {
                                    self.tree.select_add(id);
                                } else {
                                    self.tree.select_only(Some(id));
                                }
                            }
                        });
                    }
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
        let corner = egui::Area::new(egui::Id::new("undo_redo"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(scene.right_bottom() - egui::vec2(10., 10.))
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    #[cfg(test)]
                    self.view_rects.clear();
                    for view in crate::camera::View::ALL {
                        let response = ui
                            .add(
                                egui::Button::new(egui::RichText::new(view.label()).size(13.))
                                    .min_size(egui::vec2(0., 32.)),
                            )
                            .on_hover_text(view.hint());
                        #[cfg(test)]
                        self.view_rects.push((view, response.rect));
                        if response.clicked() {
                            actions.camera_view = Some(view);
                        }
                    }
                    ui.add_space(8.);
                    let undo = history_button(ui, idle && self.edit.can_undo, false).on_hover_text(
                        "Undo: отменить последнюю правку · Ctrl+Z. Ревизия всё равно увеличится.",
                    );
                    let redo = history_button(ui, idle && self.edit.can_redo, true)
                        .on_hover_text("Redo: повторить отменённую правку · Ctrl+Y");
                    #[cfg(test)]
                    {
                        self.undo_rect = undo.rect;
                        self.redo_rect = redo.rect;
                    }
                    actions.undo |= undo.clicked();
                    actions.redo |= redo.clicked();
                });
            });
        self.corner_rect = corner.response.rect;
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

/// An icon button with a half-circle arrow: left for Undo, right (mirrored)
/// for Redo.
fn history_button(ui: &mut egui::Ui, enabled: bool, redo: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(egui::vec2(40., 32.)),
    );
    let color = if enabled {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let painter = ui.painter();
    let icon = response.rect.center() + egui::vec2(0., 2.);
    let flip = if redo { -1. } else { 1. };
    let r = 7.;
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
    response
}

/// An eye: open (shown) or crossed out (hidden).
fn paint_eye(painter: &egui::Painter, c: egui::Pos2, open: bool, color: Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    let upper: Vec<egui::Pos2> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.;
            c + egui::vec2(-7. + 14. * t, -4.5 * (std::f32::consts::PI * t).sin())
        })
        .collect();
    let lower: Vec<egui::Pos2> = upper
        .iter()
        .map(|p| egui::pos2(p.x, 2. * c.y - p.y))
        .collect();
    painter.add(egui::Shape::line(upper, stroke));
    painter.add(egui::Shape::line(lower, stroke));
    if open {
        painter.circle_filled(c, 2.2, color);
    } else {
        painter.line_segment([c + egui::vec2(-6., 5.), c + egui::vec2(6., -5.)], stroke);
    }
}

/// 1234567 → «1 234 567» (thin spaces between thousands).
fn group_digits(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod digit_tests {
    #[test]
    fn thousands_are_grouped() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(4903), "4\u{2009}903");
        assert_eq!(super::group_digits(1234567), "1\u{2009}234\u{2009}567");
    }
}
