//! RIG-05 in the desktop: the background search for joint fits and what the
//! Rigging card shows. The search reads a Session clone on the worker; the
//! result is tied to the document revision it was made for, so a changed
//! model shows it as stale instead of applying old positions.
use crate::worker::{Request, Worker};
use ad_commands::Session;
use ad_document::{AssetResult, FitConfidence, JointFit};
use std::sync::Arc;

pub struct FitItem {
    pub fit: JointFit,
    pub selected: bool,
}

#[derive(Default)]
pub struct FitState {
    /// Document and revision the results belong to.
    pub document: u64,
    pub revision: u64,
    pub items: Vec<FitItem>,
    pub running: bool,
    /// A search finished at least once for this document.
    pub searched: bool,
    pub error: Option<String>,
}

impl FitState {
    pub fn is_current(&self, document: &ad_document::AssetDocument) -> bool {
        self.searched && self.document == document.id().0 && self.revision == document.revision()
    }
    /// Results arrive: confident suggestions that move the joint by more
    /// than a millimetre start selected; the list runs by confidence, then
    /// by shift.
    pub fn publish(&mut self, document: u64, revision: u64, mut fits: Vec<JointFit>) {
        let rank = |c: FitConfidence| match c {
            FitConfidence::High => 0,
            FitConfidence::Medium => 1,
            FitConfidence::Low => 2,
        };
        fits.sort_by(|a, b| {
            rank(a.confidence)
                .cmp(&rank(b.confidence))
                .then(b.shift().total_cmp(&a.shift()))
        });
        self.document = document;
        self.revision = revision;
        self.items = fits
            .into_iter()
            .map(|fit| FitItem {
                selected: fit.confidence == FitConfidence::High && fit.shift() > 1e-3,
                fit,
            })
            .collect();
        self.running = false;
        self.searched = true;
        self.error = None;
    }
}

pub type FitWorker = Worker<Arc<Session>, (u64, u64, Vec<JointFit>)>;

fn search(request: &Request<Arc<Session>>) -> AssetResult<(u64, u64, Vec<JointFit>)> {
    let document = request.input.document();
    Ok((
        document.id().0,
        document.revision(),
        document.suggest_joint_fits(&request.cancel)?,
    ))
}

pub fn new_worker() -> FitWorker {
    Worker::new(search, "Joint fit search stopped unexpectedly")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::NodeId;

    fn fit(node: usize, confidence: FitConfidence, shift: f32) -> JointFit {
        JointFit {
            node: NodeId(node),
            current: [0.; 3],
            suggested: [shift, 0., 0.],
            samples: 20,
            bone: 1.,
            confidence,
        }
    }

    #[test]
    fn results_sort_by_confidence_and_preselect_confident_moves() {
        let mut state = FitState::default();
        state.publish(
            7,
            3,
            vec![
                fit(1, FitConfidence::Low, 0.5),
                fit(2, FitConfidence::High, 0.0001),
                fit(3, FitConfidence::High, 0.2),
                fit(4, FitConfidence::Medium, 0.3),
            ],
        );
        let order: Vec<usize> = state.items.iter().map(|i| i.fit.node.0).collect();
        assert_eq!(order, vec![3, 2, 4, 1]);
        let selected: Vec<usize> = state
            .items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.fit.node.0)
            .collect();
        assert_eq!(
            selected,
            vec![3],
            "a confident move; a tiny one is already in place"
        );
        assert!(state.searched && !state.running);
    }

    #[test]
    fn the_worker_finds_the_tip_fit_for_the_current_revision() {
        let asset = crate::imports::prepare(&Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Animation_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut worker = new_worker();
        worker.request(asset.session.clone());
        let started = std::time::Instant::now();
        let done = loop {
            if let Some(done) = worker.poll() {
                break done;
            }
            assert!(started.elapsed().as_secs() < 10, "search finishes");
            std::thread::yield_now();
        };
        let (document, revision, fits) = done.result.unwrap();
        assert_eq!(document, asset.document().id().0);
        assert_eq!(revision, asset.document().revision());
        assert!(fits.iter().any(|f| f.node == NodeId(1)));
        let mut state = FitState::default();
        state.publish(document, revision, fits);
        assert!(state.is_current(asset.document()));
    }
}
