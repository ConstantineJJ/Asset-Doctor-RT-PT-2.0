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
    /// Start a read-only repair preview for this source primitive.
    pub preview: Option<(ad_document::MeshId, ad_heal::RepairKind)>,
    pub apply: bool,
    pub discard_preview: bool,
    pub undo: bool,
    pub redo: bool,
    pub cancel_edit: bool,
    pub save_copy: bool,
    /// «Сохранить» over the working file (ADR-0035).
    pub save: bool,
    /// HEAL-02 queue (ADR-0038).
    pub queue_start: bool,
    pub queue_stop: bool,
    /// HEAL-03 history window (ADR-0039).
    pub history_open: bool,
    pub history_clear: bool,
}
/// Repair history as shown by the UI; filled by the application on demand.
#[derive(Default)]
pub struct HistoryUi {
    pub open: bool,
    /// (written by this run, formatted record), newest first.
    pub lines: Vec<(bool, String)>,
    pub corrupt: usize,
    pub error: Option<String>,
    pub path: String,
    pub only_session: bool,
    pub confirm_clear: bool,
}
/// One history record as a single readable line.
pub fn history_line(r: &crate::history::Record) -> String {
    let mesh = r
        .mesh
        .map_or(String::new(), |m| format!(" · примитив #{m}"));
    let what = match r.action.as_str() {
        "save" => "Сохранить".to_owned(),
        "save-copy" => "Сохранить копию".to_owned(),
        "undo" => "Отмена правки".to_owned(),
        "redo" => "Возврат правки".to_owned(),
        "queue" => format!("Очередь · {}", r.rule),
        _ => format!("Ремонт {}", r.rule),
    };
    let mut line = format!(
        "{} · {} · {what}{mesh}",
        crate::history::format_utc(r.time),
        r.model
    );
    if r.mesh.is_some() {
        line.push_str(&format!(
            " · треугольники {} -> {} · вершины {} -> {}",
            r.triangles.0, r.triangles.1, r.vertices.0, r.vertices.1
        ));
        if r.normals > 0 {
            line.push_str(&format!(" · нормалей {}", r.normals));
        }
        if let Some(left) = r.left {
            line.push_str(&format!(" · осталось {} {left}", r.rule));
        }
        line.push_str(if r.complete {
            " · анализ полный"
        } else {
            " · анализ неполный"
        });
    }
    line.push_str(&format!(" · ревизия {}", r.revision));
    line
}
/// Repair-queue state shown by the UI (ADR-0038).
#[derive(Clone, Default)]
pub struct QueueUi {
    pub running: bool,
    pub done: Vec<String>,
    pub skipped: Vec<String>,
    pub current: Option<String>,
    pub stop: Option<String>,
}
/// Facts of a prepared, not yet applied repair (ADR-0031 Preview).
#[derive(Clone)]
pub struct PreviewInfo {
    pub kind: ad_heal::RepairKind,
    pub mesh: ad_document::MeshId,
    /// Triangles before/after the edit.
    pub before: usize,
    pub after: usize,
    pub vertices_before: usize,
    pub vertices_after: usize,
    /// BIN bytes a saved copy appends after the original data.
    pub appended_bytes: usize,
    /// G006 boundary edges of the primitive before/after a vertex merge.
    pub boundary_edges: Option<(usize, usize)>,
    /// Normals a G004/G005 repair rewrites.
    pub changed_normals: usize,
    /// Source node references, including inactive nodes; not visible copies.
    pub references: usize,
    pub complete: bool,
    /// Findings of the repaired rule left on this primitive after the edit.
    pub target_left: Option<usize>,
    pub geometry_bytes: usize,
}
/// Edit state shown by the UI; owned and refreshed by the application.
#[derive(Default)]
pub struct EditUi {
    pub busy: Option<String>,
    pub preview: Option<PreviewInfo>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A model is open; Save Copy also works for an unchanged model.
    pub can_save: bool,
    /// A working file exists that «Сохранить» may replace.
    pub can_overwrite: bool,
    pub queue: Option<QueueUi>,
    /// Revision/origin differs from the imported file or last verified copy.
    pub dirty: bool,
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
    show_diagnostics: bool,
    pub lighting: ViewLighting,
    help: bool,
    pub tree: SceneTree,
    scene: egui::Rect,
    pub status: String,
    /// Refusal or failure: drawn in the warning colour until the next status.
    pub status_warning: bool,
    pub history: HistoryUi,
    pub error: Option<String>,
    pub edit: EditUi,
    #[cfg(test)]
    open_rect: egui::Rect,
    #[cfg(test)]
    apply_rect: egui::Rect,
    #[cfg(test)]
    discard_rect: egui::Rect,
    #[cfg(test)]
    undo_rect: egui::Rect,
    #[cfg(test)]
    redo_rect: egui::Rect,
    #[cfg(test)]
    preview_rect: egui::Rect,
    #[cfg(test)]
    preview_rects: Vec<(ad_heal::RepairKind, egui::Rect)>,
    #[cfg(test)]
    save_rect: egui::Rect,
    #[cfg(test)]
    overwrite_rect: egui::Rect,
    show_queue: bool,
    #[cfg(test)]
    queue_rect: egui::Rect,
    #[cfg(test)]
    queue_start_rect: egui::Rect,
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
    #[cfg(test)]
    diagnostics_rect: egui::Rect,
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
            show_diagnostics: false,
            lighting: ViewLighting::default(),
            help: false,
            tree: SceneTree::default(),
            scene: egui::Rect::NOTHING,
            status: "Готов к открытию GLB".into(),
            status_warning: false,
            history: HistoryUi::default(),
            error: None,
            edit: EditUi::default(),
            #[cfg(test)]
            open_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            apply_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            discard_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            undo_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            redo_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            preview_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            preview_rects: Vec::new(),
            #[cfg(test)]
            save_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            overwrite_rect: egui::Rect::NOTHING,
            show_queue: false,
            #[cfg(test)]
            queue_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            queue_start_rect: egui::Rect::NOTHING,
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
            #[cfg(test)]
            diagnostics_rect: egui::Rect::NOTHING,
        }
    }
}
impl ViewerUi {
    /// Replaces the status line; `warning` marks a refusal or failure.
    pub fn set_status(&mut self, text: impl Into<String>, warning: bool) {
        self.status = text.into();
        self.status_warning = warning;
    }
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
            let height=root.max_rect().height();
            egui::Panel::top("tools").exact_size(48.).resizable(false).show(root,|ui| {
                ui.horizontal_centered(|ui| {
                    if width>=260. {ui.label(RichText::new("✚").size(23.).color(Color32::from_rgb(240,68,78)));}
                    if width>850. {ui.strong("ASSET DOCTOR");ui.separator();}
                    let open=ui.button("Открыть…").on_hover_text("Открыть статическую GLB-модель · Ctrl+O");
                    #[cfg(test)]
                    {self.open_rect=open.rect;}
                    actions.open|=open.clicked();
                    actions.fit|=ui.add_enabled(asset.is_some(),egui::Button::new("Вписать")).on_hover_text("Показать модель целиком · F").clicked();
                    if width>=900. {
                        let idle=!loading && self.edit.busy.is_none();
                        let undo=ui.add_enabled(idle && self.edit.can_undo,egui::Button::new("Отменить правку")).on_hover_text("Вернуть индексы до последней правки · Ctrl+Z. Ревизия всё равно увеличится.");
                        let redo=ui.add_enabled(idle && self.edit.can_redo,egui::Button::new("Вернуть правку")).on_hover_text("Повторить отменённую правку · Ctrl+Y");
                        #[cfg(test)]
                        {self.undo_rect=undo.rect;self.redo_rect=redo.rect;}
                        actions.undo|=undo.clicked();
                        actions.redo|=redo.clicked();
                        let overwrite=ui.add_enabled(idle && self.edit.can_overwrite && self.edit.dirty,egui::Button::new("Сохранить")).on_hover_text("Перезаписать рабочий файл проверенной версией · Ctrl+S. Первый раз спрашивает подтверждение; если файл изменён другой программой, перезаписи не будет.");
                        #[cfg(test)]
                        {self.overwrite_rect=overwrite.rect;}
                        actions.save|=overwrite.clicked();
                        let save=ui.add_enabled(idle && self.edit.can_save,egui::Button::new("Сохранить копию…")).on_hover_text("Проверенная копия GLB в новый файл · Ctrl+Shift+S. Существующие файлы не перезаписываются; после сохранения можно сделать копию рабочим файлом.");
                        #[cfg(test)]
                        {self.save_rect=save.rect;}
                        actions.save_copy|=save.clicked();
                        let queue=ui.add_enabled(!loading,egui::Button::new("Очередь ремонтов…")).on_hover_text("Выполнить все доступные ремонты по очереди с проверкой после каждого шага.");
                        #[cfg(test)]
                        {self.queue_rect=queue.rect;}
                        if queue.clicked() {self.show_queue=true;}
                        if self.edit.dirty {ui.colored_label(Color32::from_rgb(255,190,120),"* не сохранено").on_hover_text("Есть правки, которых нет ни в исходном файле, ни в последней проверенной копии.");}
                    }
                    if width>=680. {
                        ui.add_enabled_ui(width>=780.,|ui|{ui.toggle_value(&mut self.show_tree,"Дерево");});
                        ui.toggle_value(&mut self.show_summary,"Сводка");
                    }
                    if width>=330. {self.help|=ui.button("Справка").clicked();}
                    if width>=680. {
                        let _diagnostics=ui.toggle_value(&mut self.show_diagnostics,"Диагностика");
                        #[cfg(test)]
                        {self.diagnostics_rect=_diagnostics.rect;}
                    }
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
                    else if let Some(busy)=&self.edit.busy {
                        ui.spinner();
                        if width>=430. {ui.label(busy);}
                        actions.cancel_edit|=ui.button("Отменить подготовку").on_hover_text("Модель не изменится · Esc").clicked();
                    }
                    else if self.status_warning {ui.add(egui::Label::new(RichText::new(&self.status).color(Color32::from_rgb(255,190,120))).truncate()).on_hover_text(&self.status);}
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
                            ui.strong(asset.document().name());
                            ui.label(format!("Исходный файл: {:.2} МиБ",asset.document().source().len() as f64/1048576.));
                            ui.label(format!("Узлов в документе: {}",asset.document().nodes().len()));
                            let textures=asset.document().texture_stats();
                            ui.label(format!("Изображений материалов: {}",textures.images));
                            ui.label(format!("RGBA с mip-уровнями: {:.2} МиБ",textures.mip_bytes as f64/1048576.));
                            ui.add_space(12.);ui.strong("Отображаемая геометрия");
                            ui.label(format!("Вершины: {}",asset.snapshot().positions.len()));
                            ui.label(format!("Треугольники: {}",asset.snapshot().indices.len()/3));
                            ui.label(format!("Примитивы с повторами: {}",asset.snapshot().draws.len()));
                            ui.label(format!("Нормаль грани: {} вершин",asset.snapshot().fallback_normal_vertices)).on_hover_text("Авторская нормаль отсутствует, нулевая или преобразование вырождено. Источник не исправляется.");
                            if asset.snapshot().fallback_tangent_vertices>0 {
                                ui.label(format!("UV-основа нормалей: {} вершин",asset.snapshot().fallback_tangent_vertices)).on_hover_text("Нет пригодных авторских касательных. Используется основа по производным геометрии/UV; на сложных швах возможно отличие от MikkTSpace.");
                            }
                            ui.weak("С учётом экземпляров в сцене");
                            let b=asset.snapshot().bounds;
                            ui.add_space(12.);ui.strong("Габариты · метры");
                            for (i,axis) in ["X","Y","Z"].into_iter().enumerate() {ui.label(format!("{axis}   {:.3}",b.max[i]-b.min[i]));}
                            if let Some(id)=self.tree.selected && let Some(node)=asset.document().nodes().get(id.0) {
                                ui.separator();ui.strong("Выбранный узел");
                                if let Some(row)=self.tree.rows.iter().find(|row|row.id==id) {ui.label(&row.label);}
                                ui.label(format!("Дочерние узлы: {}",node.children.len()));
                                ui.label(format!("Ссылки на примитивы: {}",node.meshes.len()));
                                ui.collapsing("Исходная локальная матрица",|ui| {
                                    for r in 0..4 {ui.monospace(format!("{:.3}  {:.3}  {:.3}  {:.3}",node.local[0][r],node.local[1][r],node.local[2][r],node.local[3][r]));}
                                });
                            }
                            ui.collapsing("Ограничения просмотра",|ui| {
                                for note in asset.document().notes() {ui.weak(note);}
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
            let (preview,_preview_rects)=crate::diagnostic_ui::show(&context,&mut self.show_diagnostics,asset,height,!loading && self.edit.busy.is_none());
            #[cfg(test)]
            {self.preview_rect=_preview_rects.first().map_or(egui::Rect::NOTHING,|r|r.1);self.preview_rects=_preview_rects;}
            actions.preview=preview;
            if let Some(info)=self.edit.preview.clone() {
                let ready=!loading && self.edit.busy.is_none();
                let code=crate::edits::repair_label(info.kind);
                egui::Window::new(format!("Ремонт {code} · предпросмотр")).id(egui::Id::new("repair_preview")).collapsible(false).resizable(false).default_width(420.).show(&context,|ui| {
                    ui.strong(format!("Исходный примитив #{}",info.mesh.0));
                    ui.label(format!("Ссылок из узлов: {} (включая неактивные; это не число видимых копий).",info.references));
                    let vertices=matches!(info.kind,ad_heal::RepairKind::UnusedVertices|ad_heal::RepairKind::DuplicateVertices);
                    ui.label(if vertices {"Правка меняет общий примитив: все узлы, которые его используют, получат одинаковые вершины и индексы."} else {"Правка меняет общий примитив: все узлы, которые его используют, получат одинаковые индексы."});
                    ui.separator();
                    let normals=matches!(info.kind,ad_heal::RepairKind::MissingNormals|ad_heal::RepairKind::ZeroNormals);
                    if normals {
                        ui.label(format!("Нормали: вычисляется {} из {} вершин. Позиции, треугольники и остальные данные не меняются.",info.changed_normals,info.vertices_before));
                    } else if vertices {
                        ui.label(format!("Вершины: было {}, станет {} (удаляется {}). Треугольники не меняются: {}.",info.vertices_before,info.vertices_after,info.vertices_before-info.vertices_after,info.before));
                        if let Some((before,after))=info.boundary_edges {ui.label(format!("Граничные рёбра (G006): было {before}, станет {after}."));}
                    } else {
                        ui.label(format!("Треугольники: было {}, станет {} (удаляется {}).",info.before,info.after,info.before-info.after));
                    }
                    ui.weak(match info.kind {
                        ad_heal::RepairKind::DuplicateTriangles=>"Удаляются только точные повторы с тем же обходом; первый экземпляр, порядок и все атрибуты сохраняются.",
                        ad_heal::RepairKind::DegenerateTriangles=>"Удаляются только треугольники точно нулевой площади (повторяющийся индекс или три вершины на одной прямой); остальные треугольники, их порядок, вершины и атрибуты сохраняются. Освободившиеся вершины остаются (G003).",
                        ad_heal::RepairKind::UnusedVertices=>"Удаляются только вершины, на которые не ссылается ни один треугольник этого примитива. Порядок и все атрибуты оставшихся вершин сохраняются, индексы перенумеровываются. Другие примитивы с общим буфером вершин продолжают использовать прежние данные.",
                        ad_heal::RepairKind::MissingNormals=>"Нормали вычисляются по граням с учётом их площади, по индексам вершин: разрезанные швы остаются острыми. В копии GLB появится новый атрибут NORMAL.",
                        ad_heal::RepairKind::ZeroNormals=>"Заменяются только нулевые нормали; авторские нормали сохраняются без изменений. Новые значения вычисляются по граням с учётом их площади.",
                        ad_heal::RepairKind::DuplicateVertices=>"Объединяются только вершины, у которых совпадают все данные: позиция, нормаль, касательная, цвет и все UV. Треугольники переходят на первую такую вершину; швы с разными UV или нормалями остаются. Ремонт отклоняется, если топология примитива ухудшится.",
                    });
                    match (info.complete,info.target_left) {
                        (true,Some(0))=>ui.label(format!("Проверка результата: полный анализ, {code} в этом примитиве не останется.")),
                        (true,Some(n))=>ui.colored_label(Color32::from_rgb(255,190,120),format!("Проверка результата: полный анализ, {code} останется: {n}.")),
                        _=>ui.colored_label(Color32::from_rgb(255,190,120),format!("Проверка результата неполная: отсутствие {code} не доказано.")),
                    };
                    ui.label("Сохранение: результат совместим с проверенным экспортом копии GLB.");
                    ui.weak(format!("Копия GLB станет больше примерно на {:.1} КиБ: новые данные пишутся в конец файла, исходные данные остаются.",info.appended_bytes as f64/1024.));
                    ui.weak(format!("Новая сцена на GPU: {:.1} МиБ геометрии; текстуры загружаются заново. Новые G006 после удаления треугольников — не обязательно новые дыры.",info.geometry_bytes as f64/1048576.));
                    ui.separator();
                    ui.horizontal(|ui| {
                        let apply=ui.add_enabled(ready,egui::Button::new("Применить")).on_hover_text("Опубликовать правку целиком: индексы, история, анализ и сцена. Отменяется через «Отменить правку».");
                        let discard=ui.button("Отмена").on_hover_text("Закрыть предпросмотр; модель не изменится.");
                        #[cfg(test)]
                        {self.apply_rect=apply.rect;self.discard_rect=discard.rect;}
                        actions.apply|=apply.clicked();
                        actions.discard_preview|=discard.clicked();
                    });
                });
            }
            if self.show_queue {
                let mut open=true;
                let ready=!loading && self.edit.busy.is_none();
                egui::Window::new("Очередь ремонтов").id(egui::Id::new("repair_queue")).open(&mut open).collapsible(false).resizable(false).default_width(460.).show(&context,|ui| {
                    match &self.edit.queue {
                        Some(q) if q.running => {
                            ui.label(format!("Выполнено: {} · пропущено: {}",q.done.len(),q.skipped.len()));
                            if let Some(current)=&q.current {ui.label(format!("Сейчас: {current}"));}
                            actions.queue_stop|=ui.button("Остановить").on_hover_text("Текущий шаг отменяется; уже применённые ремонты остаются и отменяются через «Отменить правку».").clicked();
                        }
                        _ => {
                            if let Some(q)=&self.edit.queue {
                                ui.strong(format!("Итог: выполнено {}, пропущено {}",q.done.len(),q.skipped.len()));
                                for step in &q.done {ui.label(format!("выполнено: {step}"));}
                                for step in &q.skipped {ui.colored_label(Color32::from_rgb(255,190,120),format!("пропущено: {step}"));}
                                if let Some(reason)=&q.stop {ui.colored_label(Color32::from_rgb(255,190,120),format!("Остановлена: {reason}"));}
                                ui.separator();
                            }
                            let planned=asset.and_then(|a|a.diagnostics().as_ref().ok()).map(crate::queue::candidates).unwrap_or_default();
                            if planned.is_empty() {
                                ui.label("Ремонтируемых замечаний нет.");
                            } else {
                                ui.label("Первый проход (после каждого шага анализ повторяется и список пересчитывается):");
                                for (mesh,kind) in planned.iter().take(12) {ui.label(crate::queue::step_name(*mesh,*kind));}
                                if planned.len()>12 {ui.weak(format!("… и ещё {}",planned.len()-12));}
                            }
                            ui.weak("Каждый шаг — обычный проверенный ремонт и отдельная запись истории (Undo по шагам). Отказы пропускаются с причиной; регрессия или сбой останавливают очередь. Файл не сохраняется.");
                            let start=ui.add_enabled(ready && !planned.is_empty(),egui::Button::new("Запустить очередь"));
                            #[cfg(test)]
                            {self.queue_start_rect=start.rect;}
                            actions.queue_start|=start.clicked();
                        }
                    }
                    ui.separator();
                    actions.history_open|=ui.button("История ремонта…").on_hover_text("Прошлые ремонты и сохранения из всех запусков программы. Это не проверка текущей модели.").clicked();
                });
                self.show_queue&=open;
            }
            if self.history.open {
                let mut open=true;
                egui::Window::new("История ремонта").id(egui::Id::new("repair_history")).open(&mut open).resizable(true).default_size([620.,360.]).show(&context,|ui| {
                    ui.colored_label(Color32::from_rgb(255,190,120),"Записи описывают прошлые ремонты и проверки. Состояние текущей модели — только в окне «Диагностика».");
                    if !self.history.path.is_empty() {ui.weak(format!("Файл: {}",self.history.path));}
                    if let Some(error)=&self.history.error {ui.colored_label(Color32::from_rgb(255,173,173),error);}
                    if self.history.corrupt>0 {ui.colored_label(Color32::from_rgb(255,190,120),format!("Пропущено повреждённых записей: {}",self.history.corrupt));}
                    ui.checkbox(&mut self.history.only_session,"Только эта сессия");
                    egui::ScrollArea::vertical().max_height(240.).show(ui,|ui|{
                        let mut shown=0;
                        for (session,line) in &self.history.lines {
                            if self.history.only_session && !session {continue;}
                            ui.label(line);
                            shown+=1;
                        }
                        if shown==0 {ui.weak("Записей нет.");}
                    });
                    ui.separator();
                    if self.history.confirm_clear {
                        ui.horizontal(|ui| {
                            ui.label("Удалить всю историю ремонта?");
                            if ui.button("Да, очистить").clicked() {actions.history_clear=true;self.history.confirm_clear=false;}
                            if ui.button("Нет").clicked() {self.history.confirm_clear=false;}
                        });
                    } else if ui.add_enabled(!self.history.lines.is_empty(),egui::Button::new("Очистить историю…")).clicked() {
                        self.history.confirm_clear=true;
                    }
                });
                self.history.open&=open;
            }
            egui::Window::new("Справка").open(&mut self.help).resizable(true).show(&context,|ui| {
                ui.label("Открыть: Ctrl+O или перетащить GLB в окно.");
                ui.label("ЛКМ — вращение; ПКМ / средняя — перемещение; колесо — масштаб.");
                ui.label("F — вписать модель; Esc — отменить загрузку или подготовку правки; H — справка.");
                ui.label("Ремонт G001 и G002: «Диагностика», затем «Предпросмотр ремонта…» в строке замечания, затем «Применить». Ctrl+Z — отменить правку, Ctrl+Y — вернуть.");
                ui.label("«Сохранить копию…» (Ctrl+Shift+S) пишет проверенный GLB только в новый файл и заново открывает его для сверки. При закрытии или открытии другой модели с несохранёнными правками программа спросит, что делать.");
                ui.separator();ui.label("Выбор узла показывает сведения. Подсветка узлов появится позднее.");
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
        let diagnostics = ad_analysis::analyze(
            &document,
            ad_analysis::Limits::default(),
            &ad_document::Cancellation::default(),
        );
        let stamp = document.stamp();
        let asset = PreparedAsset {
            session: std::sync::Arc::new(
                ad_commands::Session::new(
                    document,
                    ad_commands::HistoryLimits::default(),
                    ad_document::ImportLimits::default(),
                )
                .unwrap(),
            ),
            generation: crate::imports::Generation {
                snapshot,
                diagnostics,
                stamp,
            },
        };
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
        assert!(std::sync::Arc::ptr_eq(asset.document().source(), &source));
        assert_eq!(asset.snapshot().fallback_normal_vertices, 6);
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
    fn diagnostics_button_opens_the_read_only_report_window() {
        let mut ui = ViewerUi::default();
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let pos = ui.diagnostics_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.show_diagnostics);
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
    /// Clicks the centre of a rect across two frames; returns release actions.
    fn click(
        ui: &mut ViewerUi,
        asset: Option<&PreparedAsset>,
        rect: egui::Rect,
        size: (f32, f32),
    ) -> Actions {
        let mut last = Actions::default();
        for pressed in [true, false] {
            let mut input = raw(size.0, size.1);
            input.events = vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, asset, false);
            out.drop_without_applying_deltas();
            last = actions;
        }
        last
    }
    fn preview_info() -> PreviewInfo {
        PreviewInfo {
            kind: ad_heal::RepairKind::DuplicateTriangles,
            mesh: ad_document::MeshId(0),
            before: 992,
            after: 960,
            vertices_before: 600,
            vertices_after: 600,
            appended_bytes: 11_520,
            boundary_edges: None,
            changed_normals: 0,
            references: 1,
            complete: true,
            target_left: Some(0),
            geometry_bytes: 261_504,
        }
    }
    #[test]
    fn apply_and_discard_are_offered_only_for_a_ready_preview() {
        let mut ui = ViewerUi::default();
        ui.edit.preview = Some(preview_info());
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
            out.drop_without_applying_deltas();
        }
        let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.apply_rect.is_positive() && ui.discard_rect.is_positive());
        let actions = {
            let rect = ui.apply_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(actions.apply && !actions.discard_preview);
        let actions = {
            let rect = ui.discard_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(actions.discard_preview && !actions.apply);
        // While another edit runs, the stale-prone Apply is disabled.
        ui.edit.busy = Some("Подготовка".into());
        let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        let actions = {
            let rect = ui.apply_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(!actions.apply);
    }
    #[test]
    fn undo_and_redo_buttons_follow_history_state() {
        let mut ui = ViewerUi::default();
        ui.edit.can_undo = true;
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        let actions = {
            let rect = ui.undo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(actions.undo && !actions.redo);
        let actions = {
            let rect = ui.redo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.redo, "redo disabled without a redo entry");
        ui.edit.can_redo = true;
        ui.edit.busy = Some("Отмена правки".into());
        let actions = {
            let rect = ui.redo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(
            !actions.redo && !actions.undo,
            "history disabled while busy"
        );
    }
    #[test]
    fn g002_finding_offers_a_read_only_repair_preview() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Duplicate_Heal.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            show_diagnostics: true,
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        assert!(ui.preview_rect.is_positive());
        let actions = {
            let rect = ui.preview_rect;
            click(&mut ui, Some(&asset), rect, (1400., 1000.))
        };
        assert_eq!(
            actions.preview,
            Some((
                ad_document::MeshId(0),
                ad_heal::RepairKind::DuplicateTriangles
            ))
        );
        assert_eq!(
            asset.document().revision(),
            0,
            "preview request changes nothing"
        );
    }
    #[test]
    fn refusal_status_is_marked_as_warning_until_the_next_status() {
        let mut ui = ViewerUi::default();
        assert!(!ui.status_warning);
        ui.set_status("Правка не подготовлена, модель прежняя: отказ", true);
        assert!(ui.status_warning);
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        ui.set_status("Правка применена; ревизия 1", false);
        assert!(!ui.status_warning);
        assert_eq!(ui.status, "Правка применена; ревизия 1");
    }
    #[test]
    fn save_copy_is_offered_when_idle_and_dirty_state_is_visible() {
        let mut ui = ViewerUi::default();
        ui.edit.can_save = true;
        ui.edit.dirty = true;
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.save_rect.is_positive());
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(actions.save_copy);
        ui.edit.busy = Some("Сохранение копии".into());
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.save_copy, "no second save or edit while busy");
        ui.edit.busy = None;
        ui.edit.can_save = false;
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.save_copy);
    }
    #[test]
    fn history_lines_read_as_past_records_with_counts() {
        let record = crate::history::Record {
            time: 0,
            session: 1,
            model: "m.glb".into(),
            source_bytes: 10,
            action: "remove".into(),
            rule: "G012".into(),
            mesh: Some(0),
            triangles: (3, 3),
            vertices: (9, 7),
            normals: 0,
            left: Some(0),
            complete: true,
            revision: 1,
        };
        assert_eq!(
            history_line(&record),
            "1970-01-01 00:00 UTC · m.glb · Ремонт G012 · примитив #0 · треугольники 3 -> 3 · вершины 9 -> 7 · осталось G012 0 · анализ полный · ревизия 1"
        );
        let save = crate::history::Record {
            action: "save".into(),
            rule: "-".into(),
            mesh: None,
            ..record
        };
        assert!(history_line(&save).contains("Сохранить · ревизия 1"));
    }
    #[test]
    fn repair_queue_window_lists_planned_steps_and_starts() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi::default();
        ui.edit.can_save = true;
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1300., 900.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let rect = ui.queue_rect;
        assert!(rect.is_positive());
        click(&mut ui, Some(&asset), rect, (1300., 900.));
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1300., 900.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.show_queue && ui.queue_start_rect.is_positive());
        let rect = ui.queue_start_rect;
        assert!(click(&mut ui, Some(&asset), rect, (1300., 900.)).queue_start);
        // While running, the window offers Stop instead of Start.
        ui.edit.queue = Some(QueueUi {
            running: true,
            ..Default::default()
        });
        ui.edit.busy = Some("Расчёт".into());
        let actions = click(&mut ui, Some(&asset), rect, (1300., 900.));
        assert!(!actions.queue_start);
    }
    #[test]
    fn save_over_working_file_needs_a_working_path_and_unsaved_edits() {
        let mut ui = ViewerUi::default();
        ui.edit.can_save = true;
        ui.edit.can_overwrite = true;
        ui.edit.dirty = true;
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.overwrite_rect.is_positive());
        let rect = ui.overwrite_rect;
        let actions = click(&mut ui, None, rect, (1200., 760.));
        assert!(actions.save && !actions.save_copy);
        for (overwrite, dirty) in [(false, true), (true, false)] {
            ui.edit.can_overwrite = overwrite;
            ui.edit.dirty = dirty;
            let actions = click(&mut ui, None, rect, (1200., 760.));
            assert!(!actions.save);
        }
    }
    /// Every non-ASCII character inside UI string literals must exist in the
    /// bundled egui fonts; otherwise it renders as an empty box (found in the
    /// manual G002 check: "992 → 960").
    #[test]
    fn ui_string_literals_use_only_glyphs_present_in_bundled_fonts() {
        let mut ui = ViewerUi::default();
        let (out, _, _) = ui.layout(raw(800., 600.), None, false);
        out.drop_without_applying_deltas();
        let sources = [
            include_str!("viewer_ui.rs"),
            include_str!("diagnostic_ui.rs"),
            include_str!("main.rs"),
        ];
        let mut missing = std::collections::BTreeSet::new();
        for source in sources {
            for line in source.lines() {
                let code = line.split("//").next().unwrap_or("");
                for (i, literal) in code.split('"').enumerate() {
                    if i % 2 == 0 {
                        continue;
                    }
                    for c in literal.chars().filter(|c| !c.is_ascii()) {
                        let present = ui
                            .context
                            .fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.), c));
                        if !present {
                            missing.insert(c);
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs missing from bundled fonts: {missing:?}"
        );
    }
    #[test]
    fn g001_finding_offers_a_degenerate_repair_preview() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            show_diagnostics: true,
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let actions = {
            let rect = ui.preview_rect;
            click(&mut ui, Some(&asset), rect, (1400., 1000.))
        };
        assert_eq!(
            actions.preview,
            Some((
                ad_document::MeshId(0),
                ad_heal::RepairKind::DegenerateTriangles
            )),
            "the first repairable row is G001"
        );
    }
    #[test]
    fn g003_finding_offers_an_unused_vertex_preview_and_its_window_applies() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            show_diagnostics: true,
            ..Default::default()
        };
        for _ in 0..4 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let rect = ui
            .preview_rects
            .iter()
            .find(|(kind, _)| *kind == ad_heal::RepairKind::UnusedVertices)
            .expect("a visible G003 row with a preview button")
            .1;
        let actions = click(&mut ui, Some(&asset), rect, (1400., 1000.));
        assert_eq!(
            actions.preview,
            Some((ad_document::MeshId(0), ad_heal::RepairKind::UnusedVertices))
        );
        assert_eq!(asset.document().revision(), 0);
        // The G003 preview window offers Apply like the triangle repairs.
        let mut ui = ViewerUi::default();
        ui.edit.preview = Some(PreviewInfo {
            kind: ad_heal::RepairKind::UnusedVertices,
            before: 5,
            after: 5,
            vertices_before: 5,
            vertices_after: 4,
            appended_bytes: 64,
            references: 2,
            ..preview_info()
        });
        for _ in 0..4 {
            let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.apply_rect.is_positive());
        let rect = ui.apply_rect;
        assert!(click(&mut ui, None, rect, (1200., 900.)).apply);
    }
}
