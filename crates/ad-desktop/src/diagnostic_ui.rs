use crate::imports::PreparedAsset;
use egui::Color32;

pub fn show(
    context: &egui::Context,
    open: &mut bool,
    asset: Option<&PreparedAsset>,
    viewport_height: f32,
) {
    egui::Window::new("Диагностика геометрии").open(open).default_size([620.,420.]).show(context,|ui| {
                ui.label("Исходные примитивы, включая неактивные; каждый проверен один раз. Экземпляры и преобразования просмотра не учитываются.");
                ui.weak("Проверки: площадь, дубли, вершины, нормали, рёбра и связность по индексам. Границы и группы могут быть намеренными. Ремонт не выполняется.");
                ui.separator();
                match asset.map(|a|(&a.document,&a.diagnostics)) {
                    None=>{ui.label("Откройте модель для анализа.");}
                    Some((document,Ok(report))) if report.matches(document)=>{
                        ui.strong(format!("{} · {} примитивов · {} треугольников",document.name(),report.primitives,report.triangles));
                        if !report.is_complete() {ui.colored_label(Color32::from_rgb(255,190,120),"Анализ частичный. Сохранённые замечания доступны; часть проверок не завершена.");}
                        ui.collapsing("Объём проверок",|ui|{
                            for c in &report.coverage {
                                let reason=match c.reason {None=>"завершено",Some(ad_analysis::LimitReason::Findings)=>"лимит результатов",Some(ad_analysis::LimitReason::Scratch)=>"лимит памяти",Some(ad_analysis::LimitReason::Allocation)=>"недостаточно памяти"};
                                let detail=format!("Пропущенные примитивы: {:?}. Исключено граней с повторными индексами: {}. Первое ограничение: {}.",c.omitted_samples().iter().map(|m|m.0).collect::<Vec<_>>(),c.excluded_triangles,reason);
                                ui.label(format!("G{:03}: примитивы {}/{} · {}",c.code as usize+1,c.completed_primitives,report.primitives,reason)).on_hover_text(detail);
                            }
                        });
                        if report.findings.is_empty() {ui.label(empty_result_text(report));}
                        egui::ScrollArea::vertical().max_height((ui.available_height()-55.).clamp(60.,(viewport_height-300.).clamp(60.,400.))).show_rows(ui,76.,report.findings.len(),|ui,range|{
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
                                    };
                                    let severity=if finding.severity==ad_analysis::Severity::Information {"Сведения"} else {"Замечание"};
                                    let title=format!("{code} · {severity} · {label}");
                                    ui.add(egui::Label::new(&title).truncate()).on_hover_text(title);
                                    let metadata=format!("Примитив #{} · количество {}",finding.mesh.0,finding.count);
                                    ui.add(egui::Label::new(&metadata).truncate()).on_hover_text(metadata);
                                    let samples=match &finding.samples {
                                        ad_analysis::Samples::Indices {element,indices}=>format!("{}: {indices:?} (с нуля, до 16 примеров)",match element {ad_analysis::Element::Vertex=>"Вершины",ad_analysis::Element::Triangle=>"Треугольники"}),
                                        ad_analysis::Samples::Edges(pairs)=>format!("Рёбра: {} (пары вершин, с нуля)",pairs.iter().map(|p|format!("{}–{}",p[0],p[1])).collect::<Vec<_>>().join(", ")),
                                        ad_analysis::Samples::Components(groups)=>format!("Группы: {}",groups.iter().map(|g|format!("v{} (вершин {}, граней {})",g.representative_vertex,g.vertices,g.triangles)).collect::<Vec<_>>().join("; ")),
                                    };
                                    ui.add(egui::Label::new(&samples).truncate()).on_hover_text(samples);
                                }); });
                            }
                        });
                    }
                    Some((_,Err(error)))=>{ui.colored_label(Color32::from_rgb(255,190,120),format!("Анализ не завершён: {}. Модель доступна для просмотра.",error.message));}
                    Some((_,Ok(_)))=>{ui.label("Результат анализа устарел и скрыт.");}
                }
                ui.separator();ui.weak("Рёбра и связность — по индексам внутри примитива, без склеивания позиций. Повторные индексы исключены. Общая вершина объединяет группы; это не число физических деталей. Ориентация, UV, материалы и скелет пока не проверяются.");
            });
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
