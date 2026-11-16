//! Bounded read-only diagnostics over immutable source primitives, never GPU data.
mod coverage;
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, ErrorCode, MeshId,
};
pub use coverage::{Coverage, LimitReason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Code {
    ZeroArea,
    DuplicateTriangle,
    UnusedVertex,
    MissingNormals,
    ZeroNormal,
}
impl Code {
    pub const ALL: [Self; 5] = [
        Self::ZeroArea,
        Self::DuplicateTriangle,
        Self::UnusedVertex,
        Self::MissingNormals,
        Self::ZeroNormal,
    ];
}
pub const ALGORITHM_VERSION: u32 = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Information,
    Warning,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    Vertex,
    Triangle,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: Code,
    pub severity: Severity,
    pub mesh: MeshId,
    pub element: Element,
    pub count: usize,
    pub samples: Vec<u32>,
}
pub struct Report {
    pub document_id: DocumentId,
    pub revision: u64,
    pub algorithm: u32,
    pub primitives: usize,
    pub triangles: usize,
    pub findings: Vec<Finding>,
    pub coverage: [Coverage; 5],
}
impl Report {
    pub fn matches(&self, document: &AssetDocument) -> bool {
        self.document_id == document.id()
            && self.revision == document.revision()
            && self.algorithm == ALGORITHM_VERSION
    }
}
#[derive(Clone, Copy)]
pub struct Limits {
    pub triangles: usize,
    pub scratch_bytes: usize,
    pub findings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            triangles: 4_000_000,
            scratch_bytes: 64 << 20,
            findings: 4096,
        }
    }
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Geometry analysis exceeds its working/result budget",
    )
}

#[derive(Default)]
struct Tally {
    count: usize,
    samples: [u32; 16],
    len: usize,
}
impl Tally {
    fn observe(&mut self, index: u32) {
        self.count += 1;
        let position = self.samples[..self.len].partition_point(|&v| v < index);
        if position < 16 {
            let end = self.len.min(15);
            self.samples.copy_within(position..end, position + 1);
            self.samples[position] = index;
            self.len = (self.len + 1).min(16);
        }
    }
    fn finding(self, mesh: MeshId, code: Code, element: Element) -> AssetResult<Finding> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(self.len).map_err(|_| budget())?;
        samples.extend_from_slice(&self.samples[..self.len]);
        Ok(Finding {
            code,
            mesh,
            element,
            count: self.count,
            samples,
            severity: if code == Code::MissingNormals {
                Severity::Information
            } else {
                Severity::Warning
            },
        })
    }
}

// All three projections must be collinear. Source f32 converts exactly to f64;
// adaptive predicates avoid cancellation false positives in a plain cross product.
fn zero_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    [(0, 1), (1, 2), (2, 0)].into_iter().all(|(x, y)| {
        let point = |v: [f32; 3]| robust::Coord { x: v[x], y: v[y] };
        robust::orient2d(point(a), point(b), point(c)) == 0.
    })
}

pub fn analyze(
    document: &AssetDocument,
    limits: Limits,
    cancel: &Cancellation,
) -> AssetResult<Report> {
    cancel.check()?;
    let mut triangles = 0usize;
    // Admission covers the whole scan before any working buffers are allocated.
    for mesh in document.primitives() {
        cancel.check()?;
        let count = mesh.indices.len() / 3;
        triangles = triangles.checked_add(count).ok_or_else(budget)?;
        let scratch = count
            .checked_mul(std::mem::size_of::<([u32; 3], u32)>())
            .and_then(|v| v.checked_add(mesh.positions.len()))
            .ok_or_else(budget)?;
        if triangles > limits.triangles
            || scratch > limits.scratch_bytes
            || count > u32::MAX as usize
            || mesh.positions.len() > u32::MAX as usize
        {
            return Err(budget());
        }
    }
    let mut report = Report {
        document_id: document.id(),
        revision: document.revision(),
        algorithm: ALGORITHM_VERSION,
        primitives: document.primitives().len(),
        triangles,
        findings: Vec::new(),
        coverage: Code::ALL.map(Coverage::new),
    };
    for (mesh_id, mesh) in document.primitives().iter().enumerate() {
        cancel.check()?;
        let mut referenced = Vec::new();
        referenced
            .try_reserve_exact(mesh.positions.len())
            .map_err(|_| budget())?;
        referenced.resize(mesh.positions.len(), 0u8);
        let mut faces = Vec::new();
        faces
            .try_reserve_exact(mesh.indices.len() / 3)
            .map_err(|_| budget())?;
        let (mut zero, mut duplicate, mut unused, mut missing, mut zero_normal) = (
            Tally::default(),
            Tally::default(),
            Tally::default(),
            Tally::default(),
            Tally::default(),
        );
        for (index, face) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if index % 1024 == 0 {
                cancel.check()?;
            }
            let [a, b, c] = [face[0], face[1], face[2]];
            for &v in face {
                referenced[v as usize] = 1;
            }
            let key = [a, b, c].min([b, c, a]).min([c, a, b]);
            faces.push((key, index as u32));
            let pa = mesh.positions[a as usize];
            let pb = mesh.positions[b as usize];
            let pc = mesh.positions[c as usize];
            if zero_area(pa, pb, pc) {
                zero.observe(index as u32);
            }
        }
        cancel.check()?;
        faces.sort_unstable(); // Bounded in-place sort; cooperative checks surround it.
        cancel.check()?;
        for (index, pair) in faces.windows(2).enumerate() {
            if index % 1024 == 0 {
                cancel.check()?;
            }
            if pair[0].0 == pair[1].0 {
                duplicate.observe(pair[1].1);
            }
        }
        for (index, &used) in referenced.iter().enumerate() {
            if index % 1024 == 0 {
                cancel.check()?;
            }
            if used == 0 {
                unused.observe(index as u32);
            }
            match &mesh.normals {
                None => missing.observe(index as u32),
                Some(normals) if normals[index] == [0.; 3] => zero_normal.observe(index as u32),
                _ => {}
            }
        }
        for (tally, code, element) in [
            (zero, Code::ZeroArea, Element::Triangle),
            (duplicate, Code::DuplicateTriangle, Element::Triangle),
            (unused, Code::UnusedVertex, Element::Vertex),
            (missing, Code::MissingNormals, Element::Vertex),
            (zero_normal, Code::ZeroNormal, Element::Vertex),
        ] {
            report.retain(code, MeshId(mesh_id), tally.count, limits, || {
                tally.finding(MeshId(mesh_id), code, element)
            });
        }
    }
    cancel.check()?;
    Ok(report)
}

#[cfg(test)]
mod tests;
