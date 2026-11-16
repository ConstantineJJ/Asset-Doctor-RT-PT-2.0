use crate::{
    AssetDocument, AssetResult, Cancellation, Code, Finding, LimitReason, Limits, MeshId, Report,
    Samples, Severity, budget,
};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeRecord {
    vertices: [u32; 2],
    triangle: u32,
}
const EDGE_CODES: [Code; 2] = [Code::BoundaryEdge, Code::HighIncidenceEdge];

#[derive(Default)]
struct Tally {
    count: usize,
    samples: [[u32; 2]; 16],
    len: usize,
}
impl Tally {
    // Edges arrive in lexicographic order, once per unique source pair.
    fn observe(&mut self, pair: [u32; 2]) {
        self.count += 1;
        if self.len < 16 {
            self.samples[self.len] = pair;
            self.len += 1;
        }
    }
    fn finding(self, mesh: MeshId, code: Code) -> AssetResult<Finding> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(self.len).map_err(|_| budget())?;
        samples.extend_from_slice(&self.samples[..self.len]);
        Ok(Finding {
            code,
            mesh,
            count: self.count,
            samples: Samples::Edges(samples),
            severity: if code == Code::BoundaryEdge {
                Severity::Information
            } else {
                Severity::Warning
            },
        })
    }
}
fn eligible(face: &[u32; 3]) -> bool {
    face[0] != face[1] && face[0] != face[2] && face[1] != face[2]
}
fn omit(report: &mut Report, mesh: MeshId, reason: LimitReason) {
    for code in EDGE_CODES {
        report.omit(code, mesh, reason);
    }
}
fn scratch_bytes(triangles: usize) -> Option<usize> {
    triangles
        .checked_mul(3)?
        .checked_mul(std::mem::size_of::<EdgeRecord>())
}

/// Called after ALL core primitives finish; their per-primitive sort/reference
/// buffers are gone. At most one edge buffer is alive; output evidence is bounded.
pub(crate) fn analyze(
    document: &AssetDocument,
    report: &mut Report,
    limits: Limits,
    cancel: &Cancellation,
) -> AssetResult<()> {
    for (index, mesh) in document.primitives().iter().enumerate() {
        let mesh_id = MeshId(index);
        let mut count = 0usize;
        let mut excluded = 0usize;
        for (i, face) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if i % 1024 == 0 {
                cancel.check()?;
            }
            if eligible(face) {
                count += 1;
            } else {
                excluded += 1;
            }
        }
        for code in EDGE_CODES {
            report.coverage[code as usize].excluded_triangles += excluded;
        }
        if scratch_bytes(count).is_none_or(|bytes| bytes > limits.scratch_bytes) {
            omit(report, mesh_id, LimitReason::Scratch);
            continue;
        }
        let mut edges = Vec::new();
        // checked by scratch_bytes above, including the same multiplication.
        if edges.try_reserve_exact(count * 3).is_err() {
            omit(report, mesh_id, LimitReason::Allocation);
            continue;
        }
        for (i, face) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if i % 1024 == 0 {
                cancel.check()?;
            }
            if !eligible(face) {
                continue;
            }
            let [a, b, c] = *face;
            // Three distinct vertices guarantee one incidence per edge/triangle.
            // Reversed/cyclic duplicate source faces retain distinct ordinals.
            for (u, v) in [(a, b), (b, c), (c, a)] {
                edges.push(EdgeRecord {
                    vertices: [u.min(v), u.max(v)],
                    triangle: i as u32,
                });
            }
        }
        cancel.check()?;
        edges.sort_unstable(); // Bounded in-place, not internally interruptible.
        cancel.check()?;
        let (boundary, high) = scan_sorted(&edges, || cancel.check())?;
        for (tally, code) in [
            (boundary, Code::BoundaryEdge),
            (high, Code::HighIncidenceEdge),
        ] {
            report.retain(code, mesh_id, tally.count, limits, || {
                tally.finding(mesh_id, code)
            });
        }
    }
    cancel.check()
}

fn scan_sorted(
    edges: &[EdgeRecord],
    mut check: impl FnMut() -> AssetResult<()>,
) -> AssetResult<(Tally, Tally)> {
    let (mut boundary, mut high) = (Tally::default(), Tally::default());
    let mut cursor = 0usize;
    while cursor < edges.len() {
        check()?;
        let first = cursor;
        let pair = edges[first].vertices;
        cursor += 1;
        while cursor < edges.len() && edges[cursor].vertices == pair {
            if cursor.is_multiple_of(1024) {
                check()?;
            }
            cursor += 1;
        }
        match cursor - first {
            1 => boundary.observe(pair),
            2 => {}
            _ => high.observe(pair),
        }
    }
    Ok((boundary, high))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_record_admission_matches_actual_storage_and_refuses_overflow() {
        assert_eq!(std::mem::size_of::<EdgeRecord>(), 12);
        assert_eq!(scratch_bytes(1_559_826), Some(56_153_736));
        assert_eq!(scratch_bytes(usize::MAX), None);
    }
    #[test]
    fn cancellation_inside_a_dense_incidence_group_returns_no_partial_counts() {
        let edges: Vec<_> = (0..2050)
            .map(|triangle| EdgeRecord {
                vertices: [0, 1],
                triangle,
            })
            .collect();
        let cancel = Cancellation::default();
        let mut probes = 0;
        let result = scan_sorted(&edges, || {
            probes += 1;
            if probes == 2 {
                cancel.cancel();
            }
            cancel.check()
        });
        assert!(matches!(result,Err(e) if e.code==crate::ErrorCode::Cancelled));
    }
}
