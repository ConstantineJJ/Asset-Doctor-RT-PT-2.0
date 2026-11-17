use crate::{
    AssetResult, Cancellation, Code, DocumentView, Element, Finding, Limits, MeshId, Report,
    Samples, Severity, Tally, budget, eligible_triangle,
};
use ad_document::MAX_UV_CHANNELS;

#[derive(Default)]
struct Channel {
    set: u32,
    zero: Tally,
    outside: Tally,
}

fn zero_area(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let point = |v: [f32; 2]| robust::Coord { x: v[0], y: v[1] };
    robust::orient2d(point(a), point(b), point(c)) == 0.
}

fn measure(
    set: u32,
    values: &[[f32; 2]],
    faces: &[[u32; 3]],
    mut check: impl FnMut() -> AssetResult<()>,
) -> AssetResult<(Channel, usize)> {
    let mut stats = Channel {
        set,
        ..Default::default()
    };
    for (vertex, value) in values.iter().enumerate() {
        if vertex % 1024 == 0 {
            check()?;
        }
        if value.iter().any(|v| !(0.0..=1.0).contains(v)) {
            stats.outside.observe(vertex as u32);
        }
    }
    let mut excluded = 0;
    for (triangle, face) in faces.iter().enumerate() {
        if triangle % 1024 == 0 {
            check()?;
        }
        if !eligible_triangle(face) {
            excluded += 1;
            continue;
        }
        let [a, b, c] = face.map(|v| values[v as usize]);
        if zero_area(a, b, c) {
            stats.zero.observe(triangle as u32);
        }
    }
    check()?;
    Ok((stats, excluded))
}

fn group(channels: &mut [Channel], code: Code, mesh: MeshId) -> AssetResult<Vec<Finding>> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(channels.len())
        .map_err(|_| budget())?;
    for channel in channels {
        let (tally, element, severity) = if code == Code::ZeroUvArea {
            (&mut channel.zero, Element::Triangle, Severity::Warning)
        } else {
            (&mut channel.outside, Element::Vertex, Severity::Information)
        };
        if tally.count > 0 {
            let mut indices = Vec::new();
            indices.try_reserve_exact(tally.len).map_err(|_| budget())?;
            indices.extend_from_slice(&tally.samples[..tally.len]);
            rows.push(Finding {
                code,
                mesh,
                count: tally.count,
                severity,
                samples: Samples::Uv {
                    set: channel.set,
                    element,
                    indices,
                },
            });
        }
    }
    Ok(rows)
}

pub(crate) fn analyze(
    view: &DocumentView,
    report: &mut Report,
    limits: Limits,
    cancel: &Cancellation,
) -> AssetResult<()> {
    for (id, mesh) in view.primitives().iter().enumerate() {
        cancel.check()?;
        let mesh_id = MeshId(id);
        report.retain(
            Code::MissingUv0,
            mesh_id,
            usize::from(mesh.source_uv(0).is_none()),
            limits,
            || {
                Ok(Finding {
                    code: Code::MissingUv0,
                    mesh: mesh_id,
                    count: 1,
                    severity: Severity::Information,
                    samples: Samples::MissingUv { set: 0 },
                })
            },
        );
        if mesh.uv_channels.is_empty() {
            report.not_applicable(Code::ZeroUvArea);
            report.not_applicable(Code::OutsideUnitUv);
            continue;
        }
        // Domain admission bounds this stack inventory to8source channels.
        // No per-vertex or per-triangle working allocation is required.
        let mut channels: [Channel; MAX_UV_CHANNELS] = std::array::from_fn(|_| Channel::default());
        let mut excluded = 0;
        for (index, (source, stats)) in mesh.uv_channels.iter().zip(&mut channels).enumerate() {
            let (result, repeated) = measure(
                source.set,
                &source.values,
                view.indices(mesh_id).as_chunks::<3>().0,
                || cancel.check(),
            )?;
            *stats = result;
            if index == 0 {
                excluded = repeated;
            }
        }
        report.coverage[Code::ZeroUvArea as usize].excluded_triangles += excluded;
        let channels = &mut channels[..mesh.uv_channels.len()];
        for code in [Code::ZeroUvArea, Code::OutsideUnitUv] {
            cancel.check()?;
            let count = channels
                .iter()
                .filter(|c| {
                    if code == Code::ZeroUvArea {
                        c.zero.count > 0
                    } else {
                        c.outside.count > 0
                    }
                })
                .count();
            report.retain_many(code, mesh_id, count, limits, || {
                group(channels, code, mesh_id)
            });
        }
    }
    cancel.check()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_after_partial_vertex_or_triangle_tallies_returns_no_channel_result() {
        for (vertices, faces, stop) in [(2050, 1, 2), (3, 2050, 3)] {
            let values = vec![[-1., 2.]; vertices];
            let faces = vec![[0, 1, 2]; faces];
            let cancel = Cancellation::default();
            let mut probes = 0;
            let result = measure(2, &values, &faces, || {
                probes += 1;
                if probes == stop {
                    cancel.cancel();
                }
                cancel.check()
            });
            assert_eq!(probes, stop);
            assert!(matches!(result,Err(e) if e.code==crate::ErrorCode::Cancelled));
        }
    }
}
