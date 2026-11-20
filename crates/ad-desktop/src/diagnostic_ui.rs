use crate::imports::PreparedAsset;
use egui::Color32;

/// A requested repair preview: source primitive and repair kind.
pub type PreviewRequest = (ad_document::MeshId, ad_heal::RepairKind);
/// Every visible preview button with its repair kind (used by UI tests).
pub type PreviewButtons = Vec<(ad_heal::RepairKind, egui::Rect)>;

/// Returns a requested repair preview and the visible preview buttons.
pub fn show(
    context: &egui::Context,
    open: &mut bool,
    asset: Option<&PreparedAsset>,
    viewport_height: f32,
    can_preview: bool,
) -> (Option<PreviewRequest>, PreviewButtons) {
    let mut preview = None;
    let mut preview_rects = Vec::new();
    egui::Window::new("Диагностика геометрии и UV").open(open).default_size([620.,420.]).show(context,|ui| {
                ui.label("Исходные примитивы, включая неактивные; каждый проверен один раз. Экземпляры и преобразования просмотра не учитываются.");
                ui.weak("Геометрия и все исходные UV-каналы. Границы, группы и UV вне 0…1 могут быть намеренными. Ремонт не выполняется.");
                ui.separator();
                match asset.map(|a|(a.document(),a.diagnostics())) {
                    None=>{ui.label("Откройте модель для анализа.");}
                    Some((document,Ok(report))) if report.matches(document)=>{
                        ui.strong(format!("{} · {} примитивов · {} треугольников",document.name(),report.primitives,report.triangles));
                        if !report.is_complete() {ui.colored_label(Color32::from_rgb(255,190,120),"Анализ частичный. Сохранённые замечания доступны; часть проверок не завершена.");}
                        ui.collapsing("Объём проверок",|ui|{
                            egui::ScrollArea::vertical().id_salt("diagnostic_coverage").auto_shrink([false,true]).max_height((viewport_height-380.).clamp(54.,144.)).show(ui,|ui|{
                            for c in &report.coverage {
                                let reason=match c.reason {None=>"завершено",Some(ad_analysis::LimitReason::Findings)=>"лимит результатов",Some(ad_analysis::LimitReason::Scratch)=>"лимит памяти",Some(ad_analysis::LimitReason::Allocation)=>"недостаточно памяти"};
                                let label=coverage_text(c,report.primitives);
                                let detail=format!("{label}. Пропущенные примитивы: {:?}. Исключено граней с повторными индексами: {}. Первое ограничение: {}.",c.omitted_samples().iter().map(|m|m.0).collect::<Vec<_>>(),c.excluded_triangles,reason);
                                ui.add(egui::Label::new(label).truncate()).on_hover_text(detail);
                            }
                            });
                        });
                        if report.findings.is_empty() {ui.label(empty_result_text(report));}
                        egui::ScrollArea::vertical().id_salt("diagnostic_findings").max_height((ui.available_height()-55.).clamp(60.,(viewport_height-300.).clamp(60.,400.))).show_rows(ui,76.,report.findings.len(),|ui,range|{
                            for i in range {
                                let finding=&report.findings[i];
                                ui.push_id(i,|ui|{ ui.allocate_ui(egui::vec2(ui.available_width(),76.),|ui|{
                                    ui.set_min_height(76.);
                                    let (code,label)=match finding.code {
                                        ad_analysis::Code::ZeroArea=>("G001","Нулевая площадь треугольника"),
                                        ad_analysis::Code::DuplicateTriangle=>("G002","Повторные треугольники · тот же обход"),
                                        ad_analysis::Code::UnusedVertex=>("G003","Неиспользуемые вершины"),
                                        ad_analysis::Code::MissingNormals=>("G004","Нормали отсутствуют · просмотр использует нормаль грани"),
                                        ad_analysis::Code::ZeroNormal=>("G005","Нулевые нормали"),
                                        ad_analysis::Code::BoundaryEdge=>("G006","Граничные рёбра · могут быть намеренными"),
                                        ad_analysis::Code::HighIncidenceEdge=>("G007","Рёбра с более чем двумя гранями"),
                                        ad_analysis::Code::DisconnectedComponents=>("G008","Группы связности по индексам · могут быть намеренными"),
                                        ad_analysis::Code::MissingUv0=>("G009","Исходный UV0 отсутствует"),
                                        ad_analysis::Code::ZeroUvArea=>("G010","Нулевая UV-площадь"),
                                        ad_analysis::Code::OutsideUnitUv=>("G011","UV вне 0…1 · могут быть намеренными"),
                                        ad_analysis::Code::DuplicateVertex=>("G012","Точные дубли вершин · все атрибуты совпадают"),
                                    };
                                    let severity=if finding.severity==ad_analysis::Severity::Information {"Сведения"} else {"Замечание"};
                                    let title=format!("{code} · {severity} · {label}");
                                    ui.add(egui::Label::new(&title).truncate()).on_hover_text(title);
                                    let metadata=format!("Примитив #{} · количество {}",finding.mesh.0,finding.count);
                                    let repair=match finding.code {
                                        ad_analysis::Code::DuplicateTriangle=>Some((ad_heal::RepairKind::DuplicateTriangles,"Полный расчёт удаления всех повторов этого исходного примитива. Модель не изменяется до «Применить».")),
                                        ad_analysis::Code::ZeroArea=>Some((ad_heal::RepairKind::DegenerateTriangles,"Полный расчёт удаления всех треугольников нулевой площади этого исходного примитива. Модель не изменяется до «Применить».")),
                                        ad_analysis::Code::MissingNormals=>Some((ad_heal::RepairKind::MissingNormals,"Полный расчёт нормалей для всех вершин этого исходного примитива по его граням. Модель не изменяется до «Применить».")),
                                        ad_analysis::Code::ZeroNormal=>Some((ad_heal::RepairKind::ZeroNormals,"Полный расчёт нулевых нормалей этого исходного примитива; авторские нормали сохраняются. Модель не изменяется до «Применить».")),
                                        ad_analysis::Code::DuplicateVertex=>Some((ad_heal::RepairKind::DuplicateVertices,"Полный расчёт объединения всех точных дублей вершин этого исходного примитива с проверкой топологии. Модель не изменяется до «Применить».")),
                                        ad_analysis::Code::UnusedVertex=>Some((ad_heal::RepairKind::UnusedVertices,"Полный расчёт удаления всех вершин, которые не использует ни один треугольник этого исходного примитива. Модель не изменяется до «Применить».")),
                                        _=>None,
                                    };
                                    if let Some((kind,hint))=repair {
                                        ui.horizontal(|ui| {
                                            ui.label(&metadata);
                                            let button=ui.add_enabled(can_preview,egui::Button::new("Предпросмотр ремонта…").small()).on_hover_text(hint);
                                            preview_rects.push((kind,button.rect));
                                            if button.clicked() {preview=Some((finding.mesh,kind));}
                                        });
                                    } else {
                                        ui.add(egui::Label::new(&metadata).truncate()).on_hover_text(metadata);
                                    }
                                    let samples=match &finding.samples {
                                        ad_analysis::Samples::Indices {element,indices}=>format!("{}: {indices:?} (с нуля, до 16 примеров)",match element {ad_analysis::Element::Vertex=>"Вершины",ad_analysis::Element::Triangle=>"Треугольники"}),
                                        ad_analysis::Samples::Edges(pairs)=>format!("Рёбра: {} (пары вершин, с нуля)",pairs.iter().map(|p|format!("{}–{}",p[0],p[1])).collect::<Vec<_>>().join(", ")),
                                        ad_analysis::Samples::Components(groups)=>format!("Группы: {}",groups.iter().map(|g|format!("v{} (вершин {}, граней {})",g.representative_vertex,g.vertices,g.triangles)).collect::<Vec<_>>().join("; ")),
                                        ad_analysis::Samples::MissingUv {set}=>format!("Нет исходного TEXCOORD_{set}; канал материала не заменяет UV0"),
                                        ad_analysis::Samples::Uv {set,element,indices}=>format!("TEXCOORD_{set} · {}: {indices:?} (с нуля)",match element {ad_analysis::Element::Vertex=>"вершины",ad_analysis::Element::Triangle=>"треугольники"}),
                                    };
                                    ui.add(egui::Label::new(&samples).truncate()).on_hover_text(samples);
                                }); });
                            }
                        });
                    }
                    Some((_,Err(error)))=>{ui.colored_label(Color32::from_rgb(255,190,120),format!("Анализ не завершён: {}. Модель доступна для просмотра.",error.message));}
                    Some((_,Ok(_)))=>{ui.label("Результат анализа устарел и скрыт.");}
                }
                ui.separator();ui.weak("Связность — по индексам, без склеивания позиций и подсчёта физических деталей. UV: наличие UV0, точная площадь, диапазон всех каналов. Перекрытия UV, ориентация, материалы и скелет не проверяются.");
            });
    (preview, preview_rects)
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
