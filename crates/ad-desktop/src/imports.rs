use crate::worker::{Request, Worker};
use ad_commands::{HistoryLimits, Session};
use ad_document::{
    AssetDocument, AssetError, AssetResult, DocumentId, DocumentStamp, ErrorCode, GeometrySnapshot,
    ImportLimits,
};
use std::{io::Read, path::PathBuf, sync::Arc};

/// Derived data published together for one document revision (ADR-0031):
/// replaced only as a whole, never field by field.
pub struct Generation {
    pub snapshot: Arc<GeometrySnapshot>,
    pub diagnostics: AssetResult<ad_analysis::Report>,
    pub stamp: DocumentStamp,
}
/// The current model: one Session (document + history) and its generation.
/// Workers may hold read-only clones of the Session; only the UI thread
/// mutates it, through exclusive access at publication.
pub struct PreparedAsset {
    pub session: Arc<Session>,
    pub generation: Generation,
}
impl PreparedAsset {
    pub fn document(&self) -> &AssetDocument {
        self.session.document()
    }
    pub fn snapshot(&self) -> &Arc<GeometrySnapshot> {
        &self.generation.snapshot
    }
    pub fn diagnostics(&self) -> &AssetResult<ad_analysis::Report> {
        &self.generation.diagnostics
    }
    /// Generation, document and scene must describe the same revision.
    pub fn is_consistent(&self) -> bool {
        self.generation.stamp.is_current(self.document())
            && (
                self.generation.snapshot.document_id,
                self.generation.snapshot.revision,
            ) == (self.document().id(), self.document().revision())
            && self
                .generation
                .diagnostics
                .as_ref()
                .is_ok_and(|r| r.matches(self.document()))
                == self.generation.diagnostics.is_ok()
    }
}
pub type Imports = Worker<PathBuf, PreparedAsset>;
pub fn new_imports() -> Imports {
    Worker::new(prepare, "Importer could not safely decode this model")
}
#[cfg(test)]
pub type Completion = crate::worker::Completion<PreparedAsset>;

pub(crate) fn prepare(request: &Request<PathBuf>) -> AssetResult<PreparedAsset> {
    prepare_with_analysis_limits(request, ad_analysis::Limits::default())
}
fn prepare_with_analysis_limits(
    request: &Request<PathBuf>,
    analysis_limits: ad_analysis::Limits,
) -> AssetResult<PreparedAsset> {
    let limits = ImportLimits::default();
    request.cancel.check()?;
    let io = |error: std::io::Error| {
        AssetError::new(ErrorCode::Io, format!("Cannot read model: {error}"))
    };
    let mut file = std::fs::File::open(&request.input).map_err(io)?;
    if file.metadata().map_err(io)?.len() > limits.source_bytes as u64 {
        return Err(AssetError::new(
            ErrorCode::ResourceLimit,
            "Model file exceeds the import source budget",
        ));
    }
    let mut source = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        request.cancel.check()?;
        let count = file.read(&mut chunk).map_err(io)?;
        if count == 0 {
            break;
        }
        if source
            .len()
            .checked_add(count)
            .is_none_or(|size| size > limits.source_bytes)
        {
            return Err(AssetError::new(
                ErrorCode::ResourceLimit,
                "Model grew beyond the source budget while reading",
            ));
        }
        source.try_reserve(count).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate model input")
        })?;
        source.extend_from_slice(&chunk[..count]);
    }
    let name = request
        .input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(128)
        .collect();
    let document = ad_formats::import_glb(
        source.into(),
        name,
        DocumentId(request.ticket),
        limits,
        &request.cancel,
    )?;
    let snapshot = Arc::new(document.snapshot(limits, &request.cancel)?);
    let started = std::time::Instant::now();
    let diagnostics = ad_analysis::analyze(&document, analysis_limits, &request.cancel);
    request.cancel.check()?;
    match &diagnostics {
        Ok(report) => eprintln!(
            "analysis_{} document={} revision={} primitives={} triangles={} findings={} elapsed_ms={}",
            if report.is_complete() {
                "complete"
            } else {
                "partial"
            },
            report.document_id.0,
            report.revision,
            report.primitives,
            report.triangles,
            report.findings.len(),
            started.elapsed().as_millis()
        ),
        Err(error) => eprintln!("analysis_incomplete code={:?}", error.code),
    }
    if let Ok(report) = &diagnostics {
        for c in &report.coverage {
            let (entries, count) = report
                .findings
                .iter()
                .filter(|f| f.code == c.code)
                .fold((0usize, 0usize), |(entries, count), f| {
                    (entries + 1, count + f.count)
                });
            eprintln!(
                "analysis_coverage rule=G{:03} completed_primitives={} omitted_primitives={} excluded_triangles={} reason={:?} retained_findings={} retained_count={} not_applicable_primitives={}",
                c.code as usize + 1,
                c.completed_primitives,
                c.omitted_primitives,
                c.excluded_triangles,
                c.reason,
                entries,
                count,
                c.not_applicable_primitives
            );
        }
    }
    let stamp = document.stamp();
    let session = Session::new(document, HistoryLimits::default(), limits)?;
    Ok(PreparedAsset {
        session: Arc::new(session),
        generation: Generation {
            snapshot,
            diagnostics,
            stamp,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::Cancellation;
    use std::thread;
    use std::time::{Duration, Instant};
    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }
    fn completion(imports: &mut Imports) -> Completion {
        let start = Instant::now();
        loop {
            if let Some(result) = imports.poll() {
                return result;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn worker_latest_request_wins_and_preserves_fixture_source() {
        let original = std::fs::read(fixture("Twin_Cubes.glb")).unwrap();
        let mut imports = new_imports();
        imports.request(fixture("Twin_Cubes.glb"));
        let ticket = imports.request(fixture("Bad_Truncated.glb"));
        let result = completion(&mut imports);
        assert_eq!(result.ticket, ticket);
        assert!(result.result.is_err());
        let ticket = imports.request(fixture("Bad_Accessor.glb"));
        let result = completion(&mut imports);
        assert_eq!(result.ticket, ticket);
        assert!(result.result.is_err());
        let ticket = imports.request(fixture("Twin_Cubes.glb"));
        let result = completion(&mut imports);
        assert_eq!(result.ticket, ticket);
        let asset = result.result.unwrap();
        assert_eq!(&**asset.document().source(), original);
        assert_eq!(asset.snapshot().indices.len() / 3, 24);
        let report = asset.diagnostics().as_ref().unwrap();
        assert!(report.matches(asset.document()));
        assert_eq!((report.primitives, report.triangles), (2, 24));
        imports.request(fixture("Bad_Image.glb"));
        assert!(completion(&mut imports).result.is_err());
        imports.request(fixture("Color_Checker.glb"));
        assert_eq!(
            completion(&mut imports)
                .result
                .unwrap()
                .document()
                .texture_stats()
                .images,
            1
        );
    }
    #[test]
    fn authored_uv_fixture_keeps_source_channels_and_not_applicable_scope() {
        let request = Request {
            ticket: 1,
            input: fixture("UV_Checker.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare(&request).unwrap();
        let report = asset.diagnostics().as_ref().unwrap();
        assert!(report.is_complete());
        assert_eq!(
            (report.primitives, report.triangles, report.findings.len()),
            (3, 4, 9)
        );
        assert_eq!(asset.snapshot().indices.len() / 3, 6);
        let area = report.coverage[ad_analysis::Code::ZeroUvArea as usize];
        assert_eq!(
            (
                area.completed_primitives,
                area.not_applicable_primitives,
                area.excluded_triangles
            ),
            (2, 1, 1)
        );
        let outside = report
            .findings
            .iter()
            .find(|f| f.code == ad_analysis::Code::OutsideUnitUv)
            .unwrap();
        assert_eq!(
            outside.samples,
            ad_analysis::Samples::Uv {
                set: 2,
                element: ad_analysis::Element::Vertex,
                indices: vec![3]
            }
        );
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.code == ad_analysis::Code::MissingUv0)
                .count(),
            2
        );
    }
    #[test]
    fn malformed_unused_uv_replacement_is_an_explicit_import_error() {
        let request = Request {
            ticket: 1,
            input: fixture("Bad_Unused_UV.glb"),
            cancel: Cancellation::default(),
        };
        assert!(matches!(prepare(&request),Err(e) if e.code==ErrorCode::InvalidFormat));
    }
    #[test]
    fn authored_diagnostic_fixture_has_eleven_source_findings_and_eleven_displayed_triangles() {
        let request = Request {
            ticket: 1,
            input: fixture("Geometry_Findings.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare(&request).unwrap();
        let report = asset.diagnostics().as_ref().unwrap();
        assert_eq!(
            (report.primitives, report.triangles, report.findings.len()),
            (2, 6, 11)
        );
        assert_eq!(asset.snapshot().indices.len() / 3, 11);
        assert_eq!(
            report.findings[0].samples,
            ad_analysis::Samples::Indices {
                element: ad_analysis::Element::Triangle,
                indices: vec![3, 4]
            }
        );
        assert_eq!(
            report.findings[1].samples,
            ad_analysis::Samples::Indices {
                element: ad_analysis::Element::Triangle,
                indices: vec![1]
            }
        );
        assert_eq!(
            report.findings[4].samples,
            ad_analysis::Samples::Indices {
                element: ad_analysis::Element::Vertex,
                indices: vec![3, 4]
            }
        );
    }
    #[test]
    fn authored_edge_fixture_counts_topology_without_multiplying_instances() {
        let request = Request {
            ticket: 1,
            input: fixture("Edge_Topology.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare(&request).unwrap();
        let report = asset.diagnostics().as_ref().unwrap();
        assert!(report.is_complete());
        assert_eq!(
            (report.primitives, report.triangles, report.findings.len()),
            (7, 13, 25)
        );
        assert_eq!(asset.snapshot().indices.len() / 3, 16);
        let count = |code| {
            report
                .findings
                .iter()
                .filter(|f| f.code == code)
                .map(|f| f.count)
                .sum::<usize>()
        };
        // "Coincident split indices" duplicates three vertices exactly (G012).
        assert_eq!(count(ad_analysis::Code::DuplicateVertex), 3);
        assert_eq!(
            (
                count(ad_analysis::Code::BoundaryEdge),
                count(ad_analysis::Code::HighIncidenceEdge)
            ),
            (21, 1)
        );
        assert_eq!(
            report.coverage[ad_analysis::Code::BoundaryEdge as usize].excluded_triangles,
            1
        );
    }
    #[test]
    fn component_fixture_counts_source_groups_and_typed_sizes_without_instances() {
        let request = Request {
            ticket: 1,
            input: fixture("Components_Checker.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare(&request).unwrap();
        let report = asset.diagnostics().as_ref().unwrap();
        assert!(report.is_complete());
        assert_eq!(
            (report.primitives, report.triangles, report.findings.len()),
            (3, 7, 11)
        );
        assert_eq!(asset.snapshot().indices.len() / 3, 11);
        let f = report
            .findings
            .iter()
            .find(|f| f.code == ad_analysis::Code::DisconnectedComponents)
            .unwrap();
        assert_eq!((f.mesh, f.count), (ad_document::MeshId(0), 4));
        assert_eq!(
            f.samples,
            ad_analysis::Samples::Components(
                (0..4)
                    .map(|i| ad_analysis::ComponentSample {
                        representative_vertex: i * 3,
                        vertices: 3,
                        triangles: 1
                    })
                    .collect()
            )
        );
        assert_eq!(
            report.coverage[ad_analysis::Code::DisconnectedComponents as usize].excluded_triangles,
            1
        );
    }
    #[test]
    fn analysis_refusal_keeps_the_valid_viewable_asset_and_incomplete_status() {
        let request = Request {
            ticket: 1,
            input: fixture("Twin_Cubes.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare_with_analysis_limits(
            &request,
            ad_analysis::Limits {
                triangles: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(asset.snapshot().indices.len(), 72);
        assert!(matches!(asset.diagnostics(),Err(e) if e.code==ErrorCode::ResourceLimit));
    }
    #[test]
    fn cancellation_clears_pending_and_blocks_late_publication() {
        let mut imports = new_imports();
        imports.request(fixture("Twin_Cubes.glb"));
        imports.cancel();
        let start = Instant::now();
        loop {
            if imports.is_idle() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(1));
        }
        assert!(imports.poll().is_none());
    }
}
