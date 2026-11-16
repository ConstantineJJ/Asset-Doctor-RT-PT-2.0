use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, ErrorCode, GeometrySnapshot,
    ImportLimits,
};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

pub struct PreparedAsset {
    pub document: AssetDocument,
    pub snapshot: Arc<GeometrySnapshot>,
    pub diagnostics: AssetResult<ad_analysis::Report>,
}
pub struct Completion {
    pub ticket: u64,
    pub result: AssetResult<PreparedAsset>,
}
struct Request {
    ticket: u64,
    path: PathBuf,
    cancel: Cancellation,
}
#[derive(Default)]
struct State {
    pending: Option<Request>,
    result: Option<Completion>,
    ticket: u64,
    stop: bool,
    active: bool,
}
pub struct Imports {
    shared: Arc<(Mutex<State>, Condvar)>,
    cancel: Cancellation,
    ticket: u64,
}

impl Imports {
    pub fn new() -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        thread::spawn(move || {
            loop {
                let request = {
                    let (lock, ready) = &*worker;
                    let mut state = lock.lock().expect("Import worker lock");
                    while state.pending.is_none() && !state.stop {
                        state = ready.wait(state).expect("Import wake lock");
                    }
                    if state.stop {
                        return;
                    }
                    state.active = true;
                    state.pending.take().expect("Pending request checked")
                };
                // A third-party parser failure must not strand every future
                // request. Known malformed inputs are rejected before readers;
                // this boundary also keeps the worker usable after an unwind.
                let result = std::panic::catch_unwind(|| prepare(&request)).unwrap_or_else(|_| {
                    Err(AssetError::new(
                        ErrorCode::InvalidFormat,
                        "Importer could not safely decode this model",
                    ))
                });
                let mut state = worker.0.lock().expect("Import completion lock");
                state.active = false;
                if state.ticket == request.ticket && !state.stop {
                    state.result = Some(Completion {
                        ticket: request.ticket,
                        result,
                    });
                }
            }
        });
        Self {
            shared,
            cancel: Cancellation::default(),
            ticket: 0,
        }
    }
    pub fn request(&mut self, path: PathBuf) -> u64 {
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        self.ticket += 1;
        let mut state = self.shared.0.lock().expect("Import request lock");
        state.ticket = self.ticket;
        state.result = None;
        state.pending = Some(Request {
            ticket: self.ticket,
            path,
            cancel: self.cancel.clone(),
        });
        self.shared.1.notify_one();
        self.ticket
    }
    pub fn cancel(&mut self) {
        self.cancel.cancel();
        self.ticket += 1;
        let mut state = self.shared.0.lock().expect("Import cancel lock");
        state.ticket = self.ticket;
        state.pending = None;
        state.result = None;
    }
    pub fn ticket(&self) -> u64 {
        self.ticket
    }
    pub fn poll(&mut self) -> Option<Completion> {
        self.shared
            .0
            .lock()
            .expect("Import poll lock")
            .result
            .take()
    }
}
impl Drop for Imports {
    fn drop(&mut self) {
        self.cancel.cancel();
        let mut state = self.shared.0.lock().expect("Import shutdown lock");
        state.stop = true;
        state.pending = None;
        state.result = None;
        self.shared.1.notify_one();
        // No UI-thread join on slow filesystem reads; worker owns only read data
        // and cooperatively exits. No user files can be written by this worker.
    }
}
fn prepare(request: &Request) -> AssetResult<PreparedAsset> {
    prepare_with_analysis_limits(request, ad_analysis::Limits::default())
}
fn prepare_with_analysis_limits(
    request: &Request,
    analysis_limits: ad_analysis::Limits,
) -> AssetResult<PreparedAsset> {
    let limits = ImportLimits::default();
    request.cancel.check()?;
    let io = |error: std::io::Error| {
        AssetError::new(ErrorCode::Io, format!("Cannot read model: {error}"))
    };
    let mut file = std::fs::File::open(&request.path).map_err(io)?;
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
        .path
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
    Ok(PreparedAsset {
        document,
        snapshot,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut imports = Imports::new();
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
        assert_eq!(&**asset.document.source(), original);
        assert_eq!(asset.snapshot.indices.len() / 3, 24);
        let report = asset.diagnostics.as_ref().unwrap();
        assert!(report.matches(&asset.document));
        assert_eq!((report.primitives, report.triangles), (2, 24));
        imports.request(fixture("Bad_Image.glb"));
        assert!(completion(&mut imports).result.is_err());
        imports.request(fixture("Color_Checker.glb"));
        assert_eq!(
            completion(&mut imports)
                .result
                .unwrap()
                .document
                .texture_stats()
                .images,
            1
        );
    }
    #[test]
    fn authored_diagnostic_fixture_has_six_source_findings_and_eleven_displayed_triangles() {
        let request = Request {
            ticket: 1,
            path: fixture("Geometry_Findings.glb"),
            cancel: Cancellation::default(),
        };
        let asset = prepare(&request).unwrap();
        let report = asset.diagnostics.unwrap();
        assert_eq!(
            (report.primitives, report.triangles, report.findings.len()),
            (2, 6, 6)
        );
        assert_eq!(asset.snapshot.indices.len() / 3, 11);
        assert_eq!(report.findings[0].samples, [3, 4]);
        assert_eq!(report.findings[1].samples, [1]);
        assert_eq!(report.findings[4].samples, [3, 4]);
    }
    #[test]
    fn analysis_refusal_keeps_the_valid_viewable_asset_and_incomplete_status() {
        let request = Request {
            ticket: 1,
            path: fixture("Twin_Cubes.glb"),
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
        assert_eq!(asset.snapshot.indices.len(), 72);
        assert!(matches!(asset.diagnostics,Err(e) if e.code==ErrorCode::ResourceLimit));
    }
    #[test]
    fn cancellation_clears_pending_and_blocks_late_publication() {
        let mut imports = Imports::new();
        imports.request(fixture("Twin_Cubes.glb"));
        imports.cancel();
        let start = Instant::now();
        loop {
            let state = imports.shared.0.lock().unwrap();
            let idle = !state.active && state.pending.is_none();
            drop(state);
            if idle {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(1));
        }
        assert!(imports.poll().is_none());
    }
}
