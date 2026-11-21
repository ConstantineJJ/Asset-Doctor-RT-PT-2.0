//! «Здоровье» tab: analysis as swipeable finding cards and the repair list.
use crate::imports::PreparedAsset;
use egui::Color32;

/// A requested repair preview: source primitive and repair kind.
pub type PreviewRequest = (ad_document::MeshId, ad_heal::RepairKind);
/// Every visible preview button with its repair kind (used by UI tests).
pub type PreviewButtons = Vec<(ad_heal::RepairKind, egui::Rect)>;

/// Readable text of one finding.
pub struct Card {
    pub code: &'static str,
    pub label: &'static str,
    pub severity: &'static str,
    pub metadata: String,
    pub samples: String,
    pub repair: Option<(ad_heal::RepairKind, &'static str)>,
}
pub fn card(finding: &ad_analysis::Finding) -> Card {
    let (code, label) = match finding.code {
        ad_analysis::Code::ZeroArea => ("G001", "Нулевая площадь треугольника"),
        ad_analysis::Code::DuplicateTriangle => ("G002", "Повторные треугольники · тот же обход"),
        ad_analysis::Code::UnusedVertex => ("G003", "Неиспользуемые вершины"),
        ad_analysis::Code::MissingNormals => (
            "G004",
            "Нормали отсутствуют · просмотр использует нормаль грани",
        ),
        ad_analysis::Code::ZeroNormal => ("G005", "Нулевые нормали"),
        ad_analysis::Code::BoundaryEdge => ("G006", "Граничные рёбра · могут быть намеренными"),
        ad_analysis::Code::HighIncidenceEdge => ("G007", "Рёбра с более чем двумя гранями"),
        ad_analysis::Code::DisconnectedComponents => (
            "G008",
            "Группы связности по индексам · могут быть намеренными",
        ),
        ad_analysis::Code::MissingUv0 => ("G009", "Исходный UV0 отсутствует"),
        ad_analysis::Code::ZeroUvArea => ("G010", "Нулевая UV-площадь"),
        ad_analysis::Code::OutsideUnitUv => ("G011", "UV вне 0…1 · могут быть намеренными"),
        ad_analysis::Code::DuplicateVertex => {
            ("G012", "Точные дубли вершин · все атрибуты совпадают")
        }
    };
    let severity = if finding.severity == ad_analysis::Severity::Information {
        "Сведения"
    } else {
        "Замечание"
    };
    let repair = match finding.code {
        ad_analysis::Code::DuplicateTriangle => Some((
            ad_heal::RepairKind::DuplicateTriangles,
            "Полный расчёт удаления всех повторов этого исходного примитива. Модель не изменяется до «Применить».",
        )),
        ad_analysis::Code::ZeroArea => Some((
            ad_heal::RepairKind::DegenerateTriangles,
            "Полный расчёт удаления всех треугольников нулевой площади этого исходного примитива. Модель не изменяется до «Применить».",
        )),
        ad_analysis::Code::MissingNormals => Some((
            ad_heal::RepairKind::MissingNormals,
            "Полный расчёт нормалей для всех вершин этого исходного примитива по его граням. Модель не изменяется до «Применить».",
        )),
        ad_analysis::Code::ZeroNormal => Some((
            ad_heal::RepairKind::ZeroNormals,
            "Полный расчёт нулевых нормалей этого исходного примитива; авторские нормали сохраняются. Модель не изменяется до «Применить».",
        )),
        ad_analysis::Code::DuplicateVertex => Some((
            ad_heal::RepairKind::DuplicateVertices,
            "Полный расчёт объединения всех точных дублей вершин этого исходного примитива с проверкой топологии. Модель не изменяется до «Применить».",
        )),
        ad_analysis::Code::UnusedVertex => Some((
            ad_heal::RepairKind::UnusedVertices,
            "Полный расчёт удаления всех вершин, которые не использует ни один треугольник этого исходного примитива. Модель не изменяется до «Применить».",
        )),
        _ => None,
    };
    let samples = match &finding.samples {
        ad_analysis::Samples::Indices { element, indices } => format!(
            "{}: {indices:?} (с нуля, до 16 примеров)",
            match element {
                ad_analysis::Element::Vertex => "Вершины",
                ad_analysis::Element::Triangle => "Треугольники",
            }
        ),
        ad_analysis::Samples::Edges(pairs) => format!(
            "Рёбра: {} (пары вершин, с нуля)",
            pairs
                .iter()
                .map(|p| format!("{}–{}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ad_analysis::Samples::Components(groups) => format!(
            "Группы: {}",
            groups
                .iter()
                .map(|g| format!(
                    "v{} (вершин {}, граней {})",
                    g.representative_vertex, g.vertices, g.triangles
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        ad_analysis::Samples::MissingUv { set } => {
            format!("Нет исходного TEXCOORD_{set}; канал материала не заменяет UV0")
        }
        ad_analysis::Samples::Uv {
            set,
            element,
            indices,
        } => format!(
            "TEXCOORD_{set} · {}: {indices:?} (с нуля)",
            match element {
                ad_analysis::Element::Vertex => "вершины",
                ad_analysis::Element::Triangle => "треугольники",
            }
        ),
    };
    Card {
        code,
        label,
        severity,
        metadata: format!(
            "Примитив #{} · количество {}",
            finding.mesh.0, finding.count
        ),
        samples,
        repair,
    }
}

/// State of the card carousel.
pub struct Carousel {
    pub index: usize,
    /// Horizontal drag of the current swipe, points.
    pub swipe: f32,
    #[cfg(test)]
    pub prev_rect: egui::Rect,
    #[cfg(test)]
    pub next_rect: egui::Rect,
    #[cfg(test)]
    pub card_rect: egui::Rect,
}
impl Default for Carousel {
    fn default() -> Self {
        Self {
            index: 0,
            swipe: 0.,
            #[cfg(test)]
            prev_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            next_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            card_rect: egui::Rect::NOTHING,
        }
    }
}

/// Small filled triangle (◀ ▶ ▼) painted into `rect`; `dir` points the tip.
pub fn triangle(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    dir: egui::Vec2,
    color: Color32,
) {
    let d = dir.normalized();
    let n = egui::vec2(-d.y, d.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + d * size,
            center - d * size * 0.7 + n * size,
            center - d * size * 0.7 - n * size,
        ],
        color,
        egui::Stroke::NONE,
    ));
}
fn arrow_button(ui: &mut egui::Ui, enabled: bool, dir: egui::Vec2, hint: &str) -> egui::Response {
    let response = ui
        .add_enabled(
            enabled,
            egui::Button::new("").min_size(egui::vec2(34., 28.)),
        )
        .on_hover_text(hint);
    let color = if enabled {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    triangle(ui.painter(), response.rect.center(), 6., dir, color);
    response
}

/// «Анализ»: summary, coverage and one finding card at a time. Cards change
/// with the ◀ ▶ buttons, a horizontal swipe on the card, or the Left/Right
/// keys while the pointer is over the panel. Returns a requested preview.
pub fn analysis(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    carousel: &mut Carousel,
    can_preview: bool,
    buttons: &mut PreviewButtons,
) -> Option<PreviewRequest> {
    let mut preview = None;
    ui.weak("Исходные примитивы, включая неактивные; каждый проверен один раз. Экземпляры и преобразования просмотра не учитываются. Ремонт не выполняется.");
    ui.separator();
    match asset.map(|a| (a.document(), a.diagnostics())) {
        None => {
            ui.label("Откройте модель для анализа.");
        }
        Some((document, Ok(report))) if report.matches(document) => {
            ui.strong(format!(
                "{} · {} примитивов · {} треугольников",
                document.name(),
                report.primitives,
                report.triangles
            ));
            if !report.is_complete() {
                ui.colored_label(
                    Color32::from_rgb(255, 190, 120),
                    "Анализ частичный. Сохранённые замечания доступны; часть проверок не завершена.",
                );
            }
            ui.collapsing("Объём проверок", |ui| {
                for c in &report.coverage {
                    let reason = match c.reason {
                        None => "завершено",
                        Some(ad_analysis::LimitReason::Findings) => "лимит результатов",
                        Some(ad_analysis::LimitReason::Scratch) => "лимит памяти",
                        Some(ad_analysis::LimitReason::Allocation) => "недостаточно памяти",
                    };
                    let label = coverage_text(c, report.primitives);
                    let detail = format!(
                        "{label}. Пропущенные примитивы: {:?}. Исключено граней с повторными индексами: {}. Первое ограничение: {}.",
                        c.omitted_samples().iter().map(|m| m.0).collect::<Vec<_>>(),
                        c.excluded_triangles,
                        reason
                    );
                    ui.add(egui::Label::new(label).truncate()).on_hover_text(detail);
                }
            });
            let count = report.findings.len();
            if count == 0 {
                ui.label(empty_result_text(report));
                return None;
            }
            carousel.index = carousel.index.min(count - 1);
            // The whole visible panel, not only what is laid out so far.
            let hovered = ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| ui.clip_rect().contains(p));
            ui.add_space(6.);
            ui.horizontal(|ui| {
                let prev = arrow_button(
                    ui,
                    carousel.index > 0,
                    egui::vec2(-1., 0.),
                    "Предыдущая карточка · стрелка влево или свайп вправо",
                );
                #[cfg(test)]
                {
                    carousel.prev_rect = prev.rect;
                }
                if prev.clicked() {
                    carousel.index -= 1;
                }
                ui.label(format!("Карточка {} из {}", carousel.index + 1, count));
                let next = arrow_button(
                    ui,
                    carousel.index + 1 < count,
                    egui::vec2(1., 0.),
                    "Следующая карточка · стрелка вправо или свайп влево",
                );
                #[cfg(test)]
                {
                    carousel.next_rect = next.rect;
                }
                if next.clicked() {
                    carousel.index += 1;
                }
            });
            if hovered && !ui.ctx().egui_wants_keyboard_input() {
                let (left, right) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowLeft),
                        i.key_pressed(egui::Key::ArrowRight),
                    )
                });
                if left && carousel.index > 0 {
                    carousel.index -= 1;
                }
                if right && carousel.index + 1 < count {
                    carousel.index += 1;
                }
            }
            let finding = &report.findings[carousel.index];
            let text = card(finding);
            let accent = if finding.severity == ad_analysis::Severity::Information {
                Color32::from_rgb(110, 160, 230)
            } else {
                Color32::from_rgb(240, 170, 90)
            };
            let frame = egui::Frame::group(ui.style())
                .fill(ui.visuals().faint_bg_color)
                .stroke(egui::Stroke::new(1.5, accent))
                .corner_radius(8.)
                .inner_margin(12.);
            let shown = frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Text selection would take the swipe drag.
                ui.style_mut().interaction.selectable_labels = false;
                ui.horizontal(|ui| {
                    ui.colored_label(accent, egui::RichText::new(text.code).strong().size(18.));
                    ui.label(text.severity);
                });
                ui.strong(text.label);
                ui.add_space(4.);
                ui.label(&text.metadata);
                ui.add(egui::Label::new(egui::RichText::new(&text.samples).weak()).wrap());
                if let Some((kind, hint)) = text.repair {
                    ui.add_space(6.);
                    let button = ui
                        .add_enabled(can_preview, egui::Button::new("Предпросмотр ремонта…"))
                        .on_hover_text(hint);
                    buttons.push((kind, button.rect));
                    if button.clicked() {
                        preview = Some((finding.mesh, kind));
                    }
                } else {
                    ui.add_space(6.);
                    ui.weak("Автоматического ремонта для этого правила нет.");
                }
            });
            #[cfg(test)]
            {
                carousel.card_rect = shown.response.rect;
            }
            // Swipe: a horizontal drag across the card turns it.
            let swipe = ui.interact(
                shown.response.rect,
                ui.id().with("card_swipe"),
                egui::Sense::drag(),
            );
            if swipe.dragged() {
                carousel.swipe += swipe.drag_delta().x;
            }
            if swipe.drag_stopped() {
                if carousel.swipe <= -60. && carousel.index + 1 < count {
                    carousel.index += 1;
                } else if carousel.swipe >= 60. && carousel.index > 0 {
                    carousel.index -= 1;
                }
                carousel.swipe = 0.;
            }
            // Position dots (at most 15, the current one bright).
            let dots = count.min(15);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.), egui::Sense::hover());
            let first = carousel.index.saturating_sub(dots / 2).min(count - dots);
            for i in 0..dots {
                let x = rect.center().x + (i as f32 - (dots as f32 - 1.) / 2.) * 12.;
                let current = first + i == carousel.index;
                ui.painter().circle_filled(
                    egui::pos2(x, rect.center().y),
                    if current { 4. } else { 2.5 },
                    if current {
                        accent
                    } else {
                        ui.visuals().weak_text_color()
                    },
                );
            }
        }
        Some((_, Err(error))) => {
            ui.colored_label(
                Color32::from_rgb(255, 190, 120),
                format!(
                    "Анализ не завершён: {}. Модель доступна для просмотра.",
                    error.message
                ),
            );
        }
        Some((_, Ok(_))) => {
            ui.label("Результат анализа устарел и скрыт.");
        }
    }
    ui.add_space(8.);
    ui.weak("Связность — по индексам, без склеивания позиций и подсчёта физических деталей. UV: наличие UV0, точная площадь, диапазон всех каналов. Перекрытия UV, ориентация, материалы и скелет не проверяются.");
    preview
}

/// «Ремонт»: every finding that has a repair, with its preview button.
pub fn repairs(
    ui: &mut egui::Ui,
    asset: Option<&PreparedAsset>,
    can_preview: bool,
    buttons: &mut PreviewButtons,
) -> Option<PreviewRequest> {
    let mut preview = None;
    match asset.map(|a| (a.document(), a.diagnostics())) {
        Some((document, Ok(report))) if report.matches(document) => {
            let mut any = false;
            for (i, finding) in report.findings.iter().enumerate() {
                let text = card(finding);
                let Some((kind, hint)) = text.repair else {
                    continue;
                };
                any = true;
                ui.push_id(i, |ui| {
                    egui::Frame::group(ui.style())
                        .corner_radius(6.)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{} · {}", text.code, text.label))
                                        .strong(),
                                )
                                .truncate(),
                            );
                            ui.horizontal(|ui| {
                                ui.label(&text.metadata);
                                let button = ui
                                    .add_enabled(
                                        can_preview,
                                        egui::Button::new("Предпросмотр…").small(),
                                    )
                                    .on_hover_text(hint);
                                buttons.push((kind, button.rect));
                                if button.clicked() {
                                    preview = Some((finding.mesh, kind));
                                }
                            });
                        });
                });
            }
            if !any {
                ui.label("Ремонтируемых замечаний нет.");
            }
        }
        None => {
            ui.label("Откройте модель, чтобы увидеть доступные ремонты.");
        }
        _ => {
            ui.label("Список ремонтов появится после завершённого анализа.");
        }
    }
    preview
}

fn coverage_text(c: &ad_analysis::Coverage, total: usize) -> String {
    format!(
        "G{:03}: проверено {}/{} · неприменимо {} · пропущено {}",
        c.code as usize + 1,
        c.completed_primitives,
        total,
        c.not_applicable_primitives,
        c.omitted_primitives
    )
}

fn empty_result_text(report: &ad_analysis::Report) -> &'static str {
    if report.is_complete() {
        "По выполненным проверкам замечаний нет. Полная пригодность модели не оценивалась."
    } else {
        "Сохранённых замечаний нет. Часть проверок не завершена; это не оценка пригодности модели."
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn coverage_labels_distinguish_checked_not_applicable_and_omitted_primitives() {
        let doc = ad_formats::import_glb(
            std::sync::Arc::from(
                include_bytes!("../../../tests/fixtures/UV_Checker.glb").as_slice(),
            ),
            "uv".into(),
            ad_document::DocumentId(1),
            Default::default(),
            &Default::default(),
        )
        .unwrap();
        let full = ad_analysis::analyze(&doc, Default::default(), &Default::default()).unwrap();
        let text = super::coverage_text(&full.coverage[ad_analysis::Code::ZeroUvArea as usize], 3);
        assert_eq!(text, "G010: проверено 2/3 · неприменимо 1 · пропущено 0");
        let partial = ad_analysis::analyze(
            &doc,
            ad_analysis::Limits {
                findings: 0,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
        let text =
            super::coverage_text(&partial.coverage[ad_analysis::Code::ZeroUvArea as usize], 3);
        assert_eq!(text, "G010: проверено 1/3 · неприменимо 1 · пропущено 1");
    }
    #[test]
    fn missing_findings_after_result_limit_are_never_presented_as_healthy() {
        let doc = ad_formats::import_glb(
            std::sync::Arc::from(
                &include_bytes!("../../../tests/fixtures/Geometry_Findings.glb")[..],
            ),
            "test".into(),
            ad_document::DocumentId(1),
            ad_document::ImportLimits::default(),
            &ad_document::Cancellation::default(),
        )
        .unwrap();
        let report = ad_analysis::analyze(
            &doc,
            ad_analysis::Limits {
                findings: 0,
                ..Default::default()
            },
            &ad_document::Cancellation::default(),
        )
        .unwrap();
        assert!(report.findings.is_empty());
        assert!(!report.is_complete());
        assert!(super::empty_result_text(&report).contains("не завершена"));
    }
}
