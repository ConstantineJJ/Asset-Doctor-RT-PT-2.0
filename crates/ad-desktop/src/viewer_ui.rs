use crate::{imports::PreparedAsset, ui_model::SceneTree};
use ad_render_vulkan::{
    SurfaceSize,
    lighting::{ShadingMode, ViewLighting},
    ui::{UiFrame, ViewportRect},
};
use egui::{Color32, RichText};
use winit::{event::WindowEvent, window::Window};

#[derive(Default)]
pub struct Actions {
    pub open: bool,
    pub fit: bool,
    pub cancel: bool,
}
pub struct ViewerUi {
    pub context: egui::Context,
    input: Option<egui_winit::State>,
    pub show_tree: bool,
    pub show_summary: bool,
    pub textures: bool,
    pub mr_maps: bool,
    pub normal_maps: bool,
    pub ao_maps: bool,
    pub emission: bool,
    pub lighting: ViewLighting,
    help: bool,
    pub tree: SceneTree,
    scene: egui::Rect,
    pub status: String,
    pub error: Option<String>,
    #[cfg(test)]
    open_rect: egui::Rect,
    #[cfg(test)]
    reset_lighting_rect: egui::Rect,
    #[cfg(test)]
    mr_maps_rect: egui::Rect,
    #[cfg(test)]
    normal_maps_rect: egui::Rect,
    #[cfg(test)]
    ao_maps_rect: egui::Rect,
    #[cfg(test)]
    emission_rect: egui::Rect,
}
impl Default for ViewerUi {
    fn default() -> Self {
        let context = egui::Context::default();
        context.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(26, 29, 35);
        visuals.window_fill = Color32::from_rgb(30, 33, 40);
        visuals.selection.bg_fill = Color32::from_rgb(100, 42, 49);
        visuals.selection.stroke = egui::Stroke::new(1., Color32::from_rgb(255, 205, 208));
        context.set_visuals(visuals);
        Self {
            context,
            input: None,
            show_tree: false,
            show_summary: true,
            textures: true,
            mr_maps: true,
            normal_maps: true,
            ao_maps: true,
            emission: true,
            lighting: ViewLighting::default(),
            help: false,
            tree: SceneTree::default(),
            scene: egui::Rect::NOTHING,
            status: "Готов к открытию GLB".into(),
            error: None,
            #[cfg(test)]
            open_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            reset_lighting_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            mr_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            normal_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            ao_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            emission_rect: egui::Rect::NOTHING,
        }
    }
}
impl ViewerUi {
    pub fn attach(&mut self, window: &Window) {
        self.input = Some(egui_winit::State::new(
            self.context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(4096),
        ));
    }
    pub fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.input
            .as_mut()
            .is_some_and(|input| input.on_window_event(window, event).consumed)
    }
    pub fn camera_at(&self, x: f64, y: f64) -> bool {
        let scale = self.context.pixels_per_point() as f64;
        self.scene
            .contains(egui::pos2((x / scale) as f32, (y / scale) as f32))
    }
    pub fn help(&mut self) {
        self.help = true;
    }
    pub fn dismiss_help(&mut self) {
        self.help = false;
    }
    pub fn camera_height(&self) -> f32 {
        self.scene.height() * self.context.pixels_per_point()
    }
    pub fn frame(
        &mut self,
        window: &Window,
        asset: Option<&PreparedAsset>,
        loading: bool,
    ) -> (UiFrame, Actions) {
        let raw = self
            .input
            .as_mut()
            .expect("UI attached to window")
            .take_egui_input(window);
        let (output, scene, actions) = self.layout(raw, asset, loading);
        self.input
            .as_mut()
            .expect("UI attached to window")
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = output.pixels_per_point;
        let mut primitives = self.context.tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        let clip = egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::pos2(
                size.width as f32 / pixels_per_point,
                size.height as f32 / pixels_per_point,
            ),
        );
        for p in &mut primitives {
            p.clip_rect = p.clip_rect.intersect(clip);
        }
        primitives.retain(|p| p.clip_rect.is_positive());
        (
            UiFrame {
                primitives,
                textures: output.textures_delta,
                pixels_per_point,
                scene: ViewportRect::from_logical(
                    [scene.min.x, scene.min.y],
                    [scene.max.x, scene.max.y],
                    pixels_per_point,
                    &SurfaceSize {
                        width: size.width,
                        height: size.height,
                    },
                ),
            },
            actions,
        )
    }
    fn layout(
        &mut self,
        raw: egui::RawInput,
        asset: Option<&PreparedAsset>,
        loading: bool,
    ) -> (egui::FullOutput, egui::Rect, Actions) {
        let context = self.context.clone();
        let mut actions = Actions::default();
        let output=context.run_ui(raw,|root| {
            let width=root.max_rect().width();
            egui::Panel::top("tools").exact_size(48.).resizable(false).show(root,|ui| {
                ui.horizontal_centered(|ui| {
                    if width>=260. {ui.label(RichText::new("✚").size(23.).color(Color32::from_rgb(240,68,78)));}
                    if width>850. {ui.strong("ASSET DOCTOR");ui.separator();}
                    let open=ui.button("Открыть…").on_hover_text("Открыть статическую GLB-модель · Ctrl+O");
                    #[cfg(test)]
                    {self.open_rect=open.rect;}
                    actions.open|=open.clicked();
                    actions.fit|=ui.add_enabled(asset.is_some(),egui::Button::new("Вписать")).on_hover_text("Показать модель целиком · F").clicked();
                    if width>=680. {
                        ui.add_enabled_ui(width>=780.,|ui|{ui.toggle_value(&mut self.show_tree,"Дерево");});
                        ui.toggle_value(&mut self.show_summary,"Сводка");
                    }
                    if width>=330. {self.help|=ui.button("Справка").clicked();}
                });
            });
            egui::Panel::bottom("status").exact_size(if self.error.is_some(){64.}else{32.}).resizable(false).show(root,|ui| {
                if let Some(error)=&self.error {
                    ui.add(egui::Label::new(RichText::new(error).color(Color32::from_rgb(255,173,173))).truncate()).on_hover_text(error);
                }
                ui.horizontal(|ui| {
                    if loading {
                        ui.spinner();
                        if width>=430. {ui.label("Загрузка GLB…");}
                        actions.cancel|=ui.button(if width>=430. {"Отменить · Esc"} else {"Отменить"}).on_hover_text("Отменить загрузку · Esc").clicked();
                    }
                    else {ui.label(&self.status);}
                    if ui.available_width()>220. {ui.separator();ui.weak("Vulkan · геометрия и базовые цвета");}
                });
            });
            if self.show_tree && width>=780. {
                egui::Panel::left("tree").default_size(230.).min_size(180.).max_size(360.).show(root,|ui| {
                    ui.heading("Сцена");ui.weak("Узлы активной сцены");ui.separator();
                    if self.tree.rows.is_empty() {ui.label("Модель не открыта");}
                    egui::ScrollArea::vertical().show_rows(ui,24.,self.tree.rows.len(),|ui,range| {
                        for index in range {
                            let row=&self.tree.rows[index];
                            ui.horizontal(|ui| {
                                ui.add_space((row.depth.min(12)*12) as f32);
                                let label=if row.label.is_empty(){"Без имени"}else{&row.label};
                                ui.set_min_height(24.);
                                let response=ui.add_sized([ui.available_width(),24.],egui::Button::selectable(self.tree.selected==Some(row.id),label).truncate());
                                if response.on_hover_text(label).clicked() {self.tree.selected=Some(row.id);}
                            });
                        }
                    });
                });
            }
            if self.show_summary && width>=680. {
                egui::Panel::right("summary").default_size(260.).min_size(220.).max_size(380.).show(root,|ui| {
                    egui::ScrollArea::vertical().show(ui,|ui| {
                        ui.heading("Сводка");ui.separator();
                        ui.checkbox(&mut self.textures,"Базовые текстуры").on_hover_text("Включить выборку встроенных PNG/JPEG. Исходные материалы не изменяются.");
                        egui::ComboBox::from_id_salt("shading_mode").selected_text(match self.lighting.mode {ShadingMode::BaseColor=>"Базовый вид", ShadingMode::DirectPbr=>"PBR · направленный свет"}).show_ui(ui,|ui| {
                            ui.selectable_value(&mut self.lighting.mode,ShadingMode::BaseColor,"Базовый вид");
                            ui.selectable_value(&mut self.lighting.mode,ShadingMode::DirectPbr,"PBR · направленный свет");
                        });
                        if self.lighting.mode==ShadingMode::DirectPbr {
                            let _maps=ui.checkbox(&mut self.mr_maps,"Карты металла/шероховатости").on_hover_text("Включить карты параметров материала. Не зависит от переключателя базовых текстур; исходник не изменяется.");
                            #[cfg(test)]
                            {self.mr_maps_rect=_maps.rect;}
                            let _normal=ui.checkbox(&mut self.normal_maps,"Карты нормалей").on_hover_text("Детали поверхности из линейной RGB-карты. Источник и геометрия не изменяются.");
                            #[cfg(test)]
                            {self.normal_maps_rect=_normal.rect;}
                            let _ao=ui.checkbox(&mut self.ao_maps,"Окклюзия · AO").on_hover_text("Затемнение заполняющего света из карты материала. Направленный свет не меняется.");
                            #[cfg(test)]
                            {self.ao_maps_rect=_ao.rect;}
                            let _emission=ui.checkbox(&mut self.emission,"Свечение").on_hover_text("Собственный цвет поверхности из материала и карты. Без ореола и освещения соседних объектов.");
                            #[cfg(test)]
                            {self.emission_rect=_emission.rect;}
                            ui.add(egui::Slider::new(&mut self.lighting.exposure_ev,-6.0..=6.0).text("Экспозиция · EV"));
                            ui.add(egui::Slider::new(&mut self.lighting.key,0.0..=8.0).text("Основной свет"));
                            ui.add(egui::Slider::new(&mut self.lighting.fill,0.0..=1.0).text("Заполнение"));
                            let reset=ui.button("Сбросить освещение");
                            #[cfg(test)]
                            {self.reset_lighting_rect=reset.rect;}
                            if reset.clicked() {self.lighting=ViewLighting{mode:ShadingMode::DirectPbr,..Default::default()};}
                            ui.weak("Металл и шероховатость: параметры × карты. Заполнение приблизительное; без HDR-окружения и теней.");
                        }
                        if let Some(asset)=asset {
                            ui.strong(asset.document.name());
                            ui.label(format!("Исходный файл: {:.2} МиБ",asset.document.source().len() as f64/1048576.));
                            ui.label(format!("Узлов в документе: {}",asset.document.nodes().len()));
                            let textures=asset.document.texture_stats();
                            ui.label(format!("Изображений материалов: {}",textures.images));
                            ui.label(format!("RGBA с mip-уровнями: {:.2} МиБ",textures.mip_bytes as f64/1048576.));
                            ui.add_space(12.);ui.strong("Отображаемая геометрия");
                            ui.label(format!("Вершины: {}",asset.snapshot.positions.len()));
                            ui.label(format!("Треугольники: {}",asset.snapshot.indices.len()/3));
                            ui.label(format!("Примитивы с повторами: {}",asset.snapshot.draws.len()));
                            ui.label(format!("Нормаль грани: {} вершин",asset.snapshot.fallback_normal_vertices)).on_hover_text("Авторская нормаль отсутствует, нулевая или преобразование вырождено. Источник не исправляется.");
                            if asset.snapshot.fallback_tangent_vertices>0 {
                                ui.label(format!("UV-основа нормалей: {} вершин",asset.snapshot.fallback_tangent_vertices)).on_hover_text("Нет пригодных авторских касательных. Используется основа по производным геометрии/UV; на сложных швах возможно отличие от MikkTSpace.");
                            }
                            ui.weak("С учётом экземпляров в сцене");
                            let b=asset.snapshot.bounds;
                            ui.add_space(12.);ui.strong("Габариты · метры");
                            for (i,axis) in ["X","Y","Z"].into_iter().enumerate() {ui.label(format!("{axis}   {:.3}",b.max[i]-b.min[i]));}
                            if let Some(id)=self.tree.selected && let Some(node)=asset.document.nodes().get(id.0) {
                                ui.separator();ui.strong("Выбранный узел");
                                if let Some(row)=self.tree.rows.iter().find(|row|row.id==id) {ui.label(&row.label);}
                                ui.label(format!("Дочерние узлы: {}",node.children.len()));
                                ui.label(format!("Ссылки на примитивы: {}",node.meshes.len()));
                                ui.collapsing("Исходная локальная матрица",|ui| {
                                    for r in 0..4 {ui.monospace(format!("{:.3}  {:.3}  {:.3}  {:.3}",node.local[0][r],node.local[1][r],node.local[2][r],node.local[3][r]));}
                                });
                            }
                            ui.collapsing("Ограничения просмотра",|ui| {
                                for note in asset.document.notes() {ui.weak(note);}
                            });
                        } else {ui.label("Откройте модель, чтобы увидеть её состав и размеры.");}
                        ui.add_space(16.);ui.separator();ui.weak("Статические GLB: PNG/JPEG, цвета вершин, нормали, OPAQUE/MASK. PBR — предварительный режим. BLEND, скелеты и анимации пока не поддерживаются.");
                    });
                });
            }
            // Leave the central root area unclaimed by egui. A CentralPanel would
            // mark it as UI-owned and consume the viewport's mouse input.
            {
                self.scene=root.available_rect_before_wrap();
                if asset.is_none() {
                    let center=self.scene.center();
                    root.painter().text(center-egui::vec2(0.,16.),egui::Align2::CENTER_CENTER,"Откройте модель",egui::FontId::proportional(24.),Color32::from_rgb(220,224,232));
                    root.painter().text(center+egui::vec2(0.,22.),egui::Align2::CENTER_CENTER,"Ctrl+O или перетащите GLB в окно",egui::FontId::proportional(14.),Color32::from_rgb(154,164,181));
                }
            }
            egui::Window::new("Справка").open(&mut self.help).resizable(true).show(&context,|ui| {
                ui.label("Открыть: Ctrl+O или перетащить GLB в окно.");
                ui.label("ЛКМ — вращение; ПКМ / средняя — перемещение; колесо — масштаб.");
                ui.label("F — вписать модель; Esc — отменить загрузку; H — справка.");
                ui.separator();ui.label("Выбор узла показывает сведения. Подсветка, редактирование и сохранение появятся позднее.");
                ui.label("В Сводке: базовый вид или PBR с авторскими нормалями, металлом и шероховатостью. Экспозиция и свет меняют только просмотр.");
                ui.label("Цвет, металл/шероховатость, нормали, окклюзия и свечение: встроенные PNG/JPEG до 8 бит/канал с независимыми переключателями. AO влияет только на заполняющий свет. Нормали используют авторские касательные либо приближённую UV-основу без MikkTSpace. Свечение не освещает соседние объекты; ореол, HDR-окружение и тени пока не отображаются. Обе стороны граней видимы.");
                ui.collapsing("Лицензии встроенных шрифтов",|ui| {
                    egui::ScrollArea::vertical().max_height(280.).show(ui,|ui| {
                        ui.label(include_str!("../../../assets/font-licenses.txt"));
                    });
                });
            });
        });
        (output, self.scene, actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(width: f32, height: f32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        }
    }
    #[test]
    fn panels_leave_viewport_and_tree_starts_hidden_at_compact_and_normal_sizes() {
        let mut ui = ViewerUi::default();
        assert!(!ui.show_tree);
        for (w, h) in [(1200., 760.), (320., 240.)] {
            let (out, rect, _) = ui.layout(raw(w, h), None, false);
            assert!(rect.width() > 100. && rect.height() > 100.);
            assert!(rect.top() >= 48. && rect.bottom() <= h - 32.);
            if w > 680. {
                assert!(rect.right() < w - 200.);
            }
            out.drop_without_applying_deltas();
        }
    }
    #[test]
    fn empty_viewport_leaves_pointer_for_camera_but_toolbar_captures_it() {
        let mut ui = ViewerUi::default();
        let (out, rect, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        for (position, captured) in [(rect.center(), false), (egui::pos2(90., 24.), true)] {
            let mut input = raw(1200., 760.);
            input.events.push(egui::Event::PointerMoved(position));
            let (out, _, _) = ui.layout(input, None, false);
            assert_eq!(ui.context.egui_wants_pointer_input(), captured);
            out.drop_without_applying_deltas();
        }
    }
    #[test]
    fn camera_hit_test_tracks_egui_zoom_as_well_as_native_dpi() {
        let mut ui = ViewerUi::default();
        ui.context.set_zoom_factor(3.);
        let (out, rect, _) = ui.layout(raw(1200., 760.), None, false);
        let scale = out.pixels_per_point as f64;
        out.drop_without_applying_deltas();
        assert!(ui.camera_at(
            rect.center().x as f64 * scale,
            rect.center().y as f64 * scale
        ));
        assert!(!ui.camera_at(24. * scale, 24. * scale));
    }
    #[test]
    fn lighting_reset_keeps_pbr_mode_and_does_not_change_texture_view_or_asset() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                exposure_ev: 2.,
                key: 7.,
                fill: 0.8,
            },
            textures: false,
            mr_maps: false,
            normal_maps: false,
            ao_maps: false,
            emission: false,
            ..Default::default()
        };
        let source = std::sync::Arc::<[u8]>::from(
            include_bytes!("../../../tests/fixtures/Pbr_Factors.glb").as_slice(),
        );
        let document = ad_formats::import_glb(
            source.clone(),
            "factors".into(),
            ad_document::DocumentId(1),
            ad_document::ImportLimits::default(),
            &ad_document::Cancellation::default(),
        )
        .unwrap();
        let snapshot = std::sync::Arc::new(
            document
                .snapshot(
                    ad_document::ImportLimits::default(),
                    &ad_document::Cancellation::default(),
                )
                .unwrap(),
        );
        let asset = PreparedAsset { document, snapshot };
        let (out, rect, _) = ui.layout(raw(1200., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        assert!(rect.width() > 100.);
        let position = ui.reset_lighting_rect.center();
        assert!(ui.reset_lighting_rect.is_positive());
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert_eq!(
            ui.lighting,
            ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            }
        );
        assert!(!ui.textures);
        assert!(!ui.mr_maps);
        assert!(!ui.normal_maps);
        assert!(!ui.ao_maps);
        assert!(!ui.emission);
        assert!(std::sync::Arc::ptr_eq(asset.document.source(), &source));
        assert_eq!(asset.snapshot.fallback_normal_vertices, 6);
    }
    #[test]
    fn mr_switch_is_independent_of_color_texture_sampling() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            ..Default::default()
        };
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.mr_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.mr_maps && ui.textures);
    }
    #[test]
    fn normal_switch_leaves_mr_and_color_sampling_independent() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            ..Default::default()
        };
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.normal_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn ao_switch_leaves_other_material_channels_enabled() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            ..Default::default()
        };
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.ao_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.ao_maps && ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn emission_switch_leaves_other_material_channels_enabled() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            ..Default::default()
        };
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.emission_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.emission && ui.ao_maps && ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn open_button_emits_one_action_on_release() {
        let mut ui = ViewerUi::default();
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.open_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 760.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
            assert_eq!(actions.open, !pressed);
            assert!(!actions.fit && !actions.cancel);
        }
    }
}
