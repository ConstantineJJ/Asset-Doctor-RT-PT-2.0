//! Safe repair queue (HEAL-02, ADR-0038): chooses the next repair from the
//! current complete report after every step, skips refused steps and stops on
//! regression, incomplete analysis or failure. Publication is the normal edit
//! pipeline; this module only decides.
use crate::edits::{findings_in_mesh, target_code};
use ad_analysis::Report;
use ad_document::MeshId;
use ad_heal::RepairKind;

/// Rule order of ADR-0038 decision 2.
pub const ORDER: [RepairKind; 6] = [
    RepairKind::DegenerateTriangles,
    RepairKind::DuplicateTriangles,
    RepairKind::DuplicateVertices,
    RepairKind::UnusedVertices,
    RepairKind::MissingNormals,
    RepairKind::ZeroNormals,
];
/// Upper bound on queue steps (decision 5).
pub const MAX_STEPS: usize = 1024;

/// Every repairable (primitive, rule) of a report, in queue order.
pub fn candidates(report: &Report) -> Vec<(MeshId, RepairKind)> {
    let mut list = Vec::new();
    for kind in ORDER {
        let code = target_code(kind);
        let mut meshes: Vec<MeshId> = report
            .findings
            .iter()
            .filter(|f| f.code == code)
            .map(|f| f.mesh)
            .collect();
        meshes.sort_by_key(|m| m.0);
        meshes.dedup();
        list.extend(meshes.into_iter().map(|m| (m, kind)));
    }
    list
}

#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Step(MeshId, RepairKind),
    Finished,
}
#[derive(Default)]
pub struct Queue {
    pub done: Vec<(MeshId, RepairKind)>,
    pub skipped: Vec<(MeshId, RepairKind, String)>,
    pub stop: Option<String>,
    pub current: Option<(MeshId, RepairKind)>,
    pub finished: bool,
}
impl Queue {
    pub fn running(&self) -> bool {
        !self.finished
    }
    fn halt(&mut self, reason: impl Into<String>) -> Next {
        self.stop.get_or_insert_with(|| reason.into());
        self.finished = true;
        self.current = None;
        Next::Finished
    }
    /// Decides the next step from the report of the current model.
    pub fn next(&mut self, report: &Report) -> Next {
        if self.finished {
            return Next::Finished;
        }
        if !report.is_complete() {
            return self.halt("анализ неполный — продолжение не доказано безопасным");
        }
        if let Some(&(mesh, kind)) = self.done.last()
            && findings_in_mesh(report, mesh, kind) > 0
        {
            return self.halt(format!(
                "регрессия: {} снова найдено в примитиве #{} после ремонта",
                crate::edits::repair_label(kind),
                mesh.0
            ));
        }
        if self.done.len() + self.skipped.len() >= MAX_STEPS {
            return self.halt("достигнут предел шагов очереди");
        }
        let blocked = |key: &(MeshId, RepairKind)| {
            self.skipped.iter().any(|(m, k, _)| (*m, *k) == *key) || self.done.contains(key)
        };
        match candidates(report).into_iter().find(|key| !blocked(key)) {
            Some((mesh, kind)) => {
                self.current = Some((mesh, kind));
                Next::Step(mesh, kind)
            }
            None => {
                self.finished = true;
                self.current = None;
                Next::Finished
            }
        }
    }
    /// The current step was published.
    pub fn completed(&mut self) {
        if let Some(key) = self.current.take() {
            self.done.push(key);
        }
    }
    /// The current step's repair refused; it is not tried again.
    pub fn skip(&mut self, reason: impl Into<String>) {
        if let Some((mesh, kind)) = self.current.take() {
            self.skipped.push((mesh, kind, reason.into()));
        }
    }
    /// A failure that makes continuing unsafe.
    pub fn fail(&mut self, reason: impl Into<String>) {
        self.halt(reason);
    }
    /// Findings of repairable rules still present (for the summary).
    pub fn remaining(report: &Report) -> usize {
        candidates(report).len()
    }
}
/// Rule label for the queue UI and logs.
pub fn step_name(mesh: MeshId, kind: RepairKind) -> String {
    format!(
        "{} · примитив #{}",
        crate::edits::repair_label(kind),
        mesh.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imports::prepare;
    use crate::worker::Request;
    use ad_document::Cancellation;

    fn report(name: &str) -> crate::imports::PreparedAsset {
        prepare(&Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures")
                .join(name),
            cancel: Cancellation::default(),
        })
        .unwrap()
    }

    #[test]
    fn candidates_follow_rule_then_primitive_order() {
        let asset = report("Geometry_Findings.glb");
        let list = candidates(asset.diagnostics().as_ref().unwrap());
        assert_eq!(
            list,
            [
                (MeshId(0), RepairKind::DegenerateTriangles),
                (MeshId(0), RepairKind::DuplicateTriangles),
                (MeshId(0), RepairKind::UnusedVertices),
                (MeshId(1), RepairKind::UnusedVertices),
                (MeshId(0), RepairKind::MissingNormals),
                (MeshId(1), RepairKind::ZeroNormals),
            ]
        );
    }

    #[test]
    fn skipped_and_done_keys_are_not_retried_and_regression_stops() {
        let asset = report("Geometry_Findings.glb");
        let r = asset.diagnostics().as_ref().unwrap();
        let mut q = Queue::default();
        assert_eq!(
            q.next(r),
            Next::Step(MeshId(0), RepairKind::DegenerateTriangles)
        );
        q.skip("refused");
        assert_eq!(
            q.next(r),
            Next::Step(MeshId(0), RepairKind::DuplicateTriangles)
        );
        // Pretend it was published but the report still shows G002 on #0.
        q.completed();
        assert_eq!(q.next(r), Next::Finished);
        assert!(q.stop.as_deref().unwrap().contains("регрессия"));
        assert!(!q.running());
    }
}
