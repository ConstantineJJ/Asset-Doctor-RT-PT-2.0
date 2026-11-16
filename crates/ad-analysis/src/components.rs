use crate::{
    AssetDocument, AssetResult, Cancellation, Code, ComponentSample, Finding, LimitReason, Limits,
    MeshId, Report, Samples, Severity, budget, eligible_triangle,
};

const CODE: Code = Code::DisconnectedComponents;
const INACTIVE: u32 = u32::MAX;

#[derive(Default)]
struct Summary {
    count: usize,
    samples: [ComponentSample; 16],
    len: usize,
}
impl Summary {
    fn observe(&mut self, sample: ComponentSample) {
        self.count += 1;
        if self.len < 16 {
            self.samples[self.len] = sample;
            self.len += 1;
        }
    }
    fn finding(self, mesh: MeshId) -> AssetResult<Finding> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(self.len).map_err(|_| budget())?;
        samples.extend_from_slice(&self.samples[..self.len]);
        Ok(Finding {
            code: CODE,
            mesh,
            count: self.count,
            severity: Severity::Information,
            samples: Samples::Components(samples),
        })
    }
}

struct Forest {
    parent: Vec<u32>,
    members: Vec<u32>,
    triangles: Vec<u32>,
    vertices: usize,
}
impl Forest {
    fn reserve(vertices: usize) -> Result<Self, std::collections::TryReserveError> {
        let mut forest = Self {
            parent: Vec::new(),
            members: Vec::new(),
            triangles: Vec::new(),
            vertices,
        };
        forest.parent.try_reserve_exact(vertices)?;
        forest.members.try_reserve_exact(vertices)?;
        forest.triangles.try_reserve_exact(vertices)?;
        Ok(forest)
    }
    fn root(&mut self, mut index: usize) -> usize {
        let mut root = index;
        while self.parent[root] as usize != root {
            root = self.parent[root] as usize;
        }
        while self.parent[index] as usize != index {
            let next = self.parent[index] as usize;
            self.parent[index] = root as u32;
            index = next;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.root(a), self.root(b));
        if a == b {
            return;
        }
        if self.members[a] < self.members[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a as u32;
        // Source admission bounds vertices and per-primitive triangle counts to u32.
        self.members[a] += self.members[b];
        self.triangles[a] += self.triangles[b];
    }
    fn measure(
        mut self,
        faces: &[[u32; 3]],
        mut check: impl FnMut() -> AssetResult<()>,
    ) -> AssetResult<Summary> {
        for i in 0..self.vertices {
            if i % 1024 == 0 {
                check()?;
            }
            self.parent.push(INACTIVE);
            self.members.push(0);
            self.triangles.push(0);
        }
        for (i, face) in faces.iter().enumerate() {
            if i % 1024 == 0 {
                check()?;
            }
            if !eligible_triangle(face) {
                continue;
            }
            for &index in face {
                let v = index as usize;
                if self.parent[v] == INACTIVE {
                    self.parent[v] = index;
                    self.members[v] = 1;
                }
            }
            let [a, b, c] = face.map(|v| v as usize);
            self.union(a, b);
            self.union(a, c);
            let root = self.root(a);
            self.triangles[root] += 1;
        }
        let mut summary = Summary::default();
        // No unions follow. Consuming the forest lets member counts mark roots
        // already summarized, without another vertex-sized allocation. Ascending
        // source indices make the first encounter the stable minimum member.
        for i in 0..self.vertices {
            if i % 1024 == 0 {
                check()?;
            }
            if self.parent[i] == INACTIVE {
                continue;
            }
            let root = self.root(i);
            let vertices = self.members[root];
            if vertices == 0 {
                continue;
            }
            self.members[root] = 0;
            summary.observe(ComponentSample {
                representative_vertex: i as u32,
                vertices,
                triangles: self.triangles[root],
            });
        }
        check()?;
        Ok(summary)
    }
}
fn scratch_bytes(vertices: usize) -> Option<usize> {
    vertices
        .checked_mul(3)?
        .checked_mul(std::mem::size_of::<u32>())
}

pub(crate) fn analyze(
    document: &AssetDocument,
    report: &mut Report,
    limits: Limits,
    cancel: &Cancellation,
) -> AssetResult<()> {
    for (index, mesh) in document.primitives().iter().enumerate() {
        cancel.check()?;
        let mesh_id = MeshId(index);
        let faces = mesh.indices.as_chunks::<3>().0;
        let mut excluded = 0usize;
        for (i, face) in faces.iter().enumerate() {
            if i % 1024 == 0 {
                cancel.check()?;
            }
            excluded += usize::from(!eligible_triangle(face));
        }
        report.coverage[CODE as usize].excluded_triangles += excluded;
        if excluded == faces.len() {
            report.retain(CODE, mesh_id, 0, limits, || {
                Summary::default().finding(mesh_id)
            });
            continue;
        }
        if scratch_bytes(mesh.positions.len()).is_none_or(|bytes| bytes > limits.scratch_bytes) {
            report.omit(CODE, mesh_id, LimitReason::Scratch);
            continue;
        }
        let Ok(forest) = Forest::reserve(mesh.positions.len()) else {
            report.omit(CODE, mesh_id, LimitReason::Allocation);
            continue;
        };
        let summary = forest.measure(faces, || cancel.check())?;
        report.retain(
            CODE,
            mesh_id,
            if summary.count > 1 { summary.count } else { 0 },
            limits,
            || summary.finding(mesh_id),
        );
    }
    cancel.check()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scratch_matches_three_u32_arrays_and_refuses_overflow() {
        assert_eq!(scratch_bytes(852_516), Some(10_230_192));
        assert_eq!(scratch_bytes(usize::MAX), None);
    }
    #[test]
    fn cancellation_during_union_or_after_a_partial_summary_returns_no_result() {
        let faces = vec![[0, 1, 2]; 2050];
        let mut probes = 0;
        let cancel = Cancellation::default();
        let result = Forest::reserve(3).unwrap().measure(&faces, || {
            probes += 1;
            if probes == 3 {
                cancel.cancel();
            }
            cancel.check()
        });
        assert!(matches!(result,Err(e) if e.code==crate::ErrorCode::Cancelled));
        let mut probes = 0;
        let cancel = Cancellation::default();
        let result =
            Forest::reserve(2050)
                .unwrap()
                .measure(&[[0, 1, 2], [1024, 1025, 1026]], || {
                    probes += 1;
                    if probes == 6 {
                        cancel.cancel();
                    }
                    cancel.check()
                });
        assert!(matches!(result,Err(e) if e.code==crate::ErrorCode::Cancelled));
    }
}
