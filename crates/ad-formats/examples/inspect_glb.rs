//! Read-only developer probe; never uploads or rewrites the chosen model.
use ad_document::{Cancellation, DocumentId, ImportLimits};
use std::{io::Read, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("Pass one GLB path")?;
    let limits = ImportLimits::default();
    let mut source = Vec::new();
    std::fs::File::open(path)?
        .take(limits.source_bytes as u64 + 1)
        .read_to_end(&mut source)?;
    let cancel = Cancellation::default();
    let doc = ad_formats::import_glb(
        Arc::from(source),
        "read-only probe".into(),
        DocumentId(1),
        limits,
        &cancel,
    )?;
    let scene = doc.snapshot(limits, &cancel)?;
    let stats = doc.texture_stats();
    println!(
        "nodes={} vertices={} triangles={} images={} sampled_images={} textures={} rgba_bytes={} mip_bytes={} fallback_normal_vertices={} mr_primitives={} normal_primitives={} fallback_tangent_vertices={} ao_primitives={} emissive_primitives={}",
        doc.nodes().len(),
        scene.positions.len(),
        scene.indices.len() / 3,
        stats.images,
        stats.sampled_images,
        stats.textures,
        stats.rgba_bytes,
        stats.mip_bytes,
        scene.fallback_normal_vertices,
        scene
            .draws
            .iter()
            .filter(|d| d.metallic_roughness_texture.is_some())
            .count(),
        scene
            .draws
            .iter()
            .filter(|d| d.normal_texture.is_some())
            .count(),
        scene.fallback_tangent_vertices,
        scene
            .draws
            .iter()
            .filter(|d| d.occlusion_texture.is_some())
            .count(),
        scene
            .draws
            .iter()
            .filter(|d| d.emissive_factor.iter().any(|&v| v > 0.))
            .count()
    );
    Ok(())
}
