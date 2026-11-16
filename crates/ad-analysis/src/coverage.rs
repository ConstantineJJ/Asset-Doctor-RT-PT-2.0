use crate::{AssetResult, Code, Finding, Limits, MeshId, Report};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitReason {
    Findings,
    Allocation,
    Scratch,
}

/// Coverage of a rule over document-local source primitives, never instances.
/// A reason is retained even if later primitives complete without findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coverage {
    pub code: Code,
    pub completed_primitives: usize,
    pub omitted_primitives: usize,
    pub excluded_triangles: usize,
    pub reason: Option<LimitReason>,
    omitted_samples: [MeshId; 16],
    omitted_sample_count: usize,
}
impl Coverage {
    pub(crate) fn new(code: Code) -> Self {
        Self {
            code,
            completed_primitives: 0,
            omitted_primitives: 0,
            excluded_triangles: 0,
            reason: None,
            omitted_samples: [MeshId(0); 16],
            omitted_sample_count: 0,
        }
    }
    pub fn is_complete(&self) -> bool {
        self.omitted_primitives == 0 && self.reason.is_none()
    }
    pub fn omitted_samples(&self) -> &[MeshId] {
        &self.omitted_samples[..self.omitted_sample_count]
    }
}

impl Report {
    pub fn is_complete(&self) -> bool {
        self.coverage
            .iter()
            .all(|c| c.is_complete() && c.completed_primitives == self.primitives)
    }
    pub(crate) fn omit(&mut self, code: Code, mesh: MeshId, reason: LimitReason) {
        let coverage = &mut self.coverage[code as usize];
        coverage.omitted_primitives += 1;
        coverage.reason.get_or_insert(reason);
        if coverage.omitted_sample_count < 16 {
            coverage.omitted_samples[coverage.omitted_sample_count] = mesh;
            coverage.omitted_sample_count += 1;
        }
    }
    // A primitive/rule result is retained atomically. A resource refusal cannot
    // erase an earlier result or masquerade as a completed zero-finding rule.
    pub(crate) fn retain(
        &mut self,
        code: Code,
        mesh: MeshId,
        count: usize,
        limits: Limits,
        build: impl FnOnce() -> AssetResult<Finding>,
    ) {
        if count > 0 {
            if self.findings.len() >= limits.findings {
                self.omit(code, mesh, LimitReason::Findings);
                return;
            }
            if self.findings.try_reserve(1).is_err() {
                self.omit(code, mesh, LimitReason::Allocation);
                return;
            }
            match build() {
                Ok(finding) => self.findings.push(finding),
                Err(_) => {
                    self.omit(code, mesh, LimitReason::Allocation);
                    return;
                }
            }
        }
        self.coverage[code as usize].completed_primitives += 1;
    }
}
