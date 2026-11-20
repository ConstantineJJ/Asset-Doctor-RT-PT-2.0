//! G012 (ADR-0034): exact duplicate vertices of one source primitive. Two
//! vertices are equal only when every decoded stream is bitwise equal; the
//! same order is shared with repair planning so a plan equals the finding.
use crate::{
    AssetResult, Cancellation, Code, DocumentView, Element, LimitReason, Limits, MeshId, Report,
    Tally,
};
use ad_document::MeshPrimitive;
use std::cmp::Ordering;

fn bits<const N: usize>(values: &[[f32; N]], i: u32) -> [u32; N] {
    values[i as usize].map(f32::to_bits)
}
fn optional<const N: usize>(values: &Option<Vec<[f32; N]>>, a: u32, b: u32) -> Ordering {
    values
        .as_deref()
        .map_or(Ordering::Equal, |v| bits(v, a).cmp(&bits(v, b)))
}

/// Total order over the bit patterns of every decoded stream of two vertices
/// of the same primitive: position, normal, tangent, color, UV channels.
pub fn vertex_order(mesh: &MeshPrimitive, a: u32, b: u32) -> Ordering {
    bits(&mesh.positions, a)
        .cmp(&bits(&mesh.positions, b))
        .then_with(|| optional(&mesh.normals, a, b))
        .then_with(|| optional(&mesh.tangents, a, b))
        .then_with(|| optional(&mesh.colors, a, b))
        .then_with(|| {
            mesh.uv_channels
                .iter()
                .map(|c| bits(&c.values, a).cmp(&bits(&c.values, b)))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        })
}
/// True when every decoded stream of the two vertices is bitwise equal.
pub fn same_vertex(mesh: &MeshPrimitive, a: u32, b: u32) -> bool {
    vertex_order(mesh, a, b).is_eq()
}

pub(crate) fn analyze(
    view: &DocumentView,
    report: &mut Report,
    limits: Limits,
    cancel: &Cancellation,
) -> AssetResult<()> {
    for (i, mesh) in view.primitives().iter().enumerate() {
        cancel.check()?;
        let id = MeshId(i);
        let n = mesh.positions.len();
        if n.checked_mul(4).is_none_or(|b| b > limits.scratch_bytes) {
            report.omit(Code::DuplicateVertex, id, LimitReason::Scratch);
            continue;
        }
        let mut order = Vec::new();
        if order.try_reserve_exact(n).is_err() {
            report.omit(Code::DuplicateVertex, id, LimitReason::Allocation);
            continue;
        }
        order.extend(0..n as u32);
        // Ties are broken by ordinal, so the first member of an equal group is
        // the earliest vertex and every later member is reported once.
        order.sort_unstable_by(|&a, &b| vertex_order(mesh, a, b).then(a.cmp(&b)));
        cancel.check()?;
        let mut tally = Tally::default();
        for (k, pair) in order.windows(2).enumerate() {
            if k % 1024 == 0 {
                cancel.check()?;
            }
            if same_vertex(mesh, pair[0], pair[1]) {
                tally.observe(pair[1]);
            }
        }
        report.retain(Code::DuplicateVertex, id, tally.count, limits, || {
            tally.finding(id, Code::DuplicateVertex, Element::Vertex)
        });
    }
    Ok(())
}
