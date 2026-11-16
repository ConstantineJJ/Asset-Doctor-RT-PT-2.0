use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, ErrorCode, Texture, TextureImage,
};
use std::sync::Arc;
pub(crate) fn mismatch() -> AssetError {
    AssetError::new(
        ErrorCode::VerificationFailed,
        "Export readback differs from the current document",
    )
}
fn equal<T: PartialEq>(a: &[T], b: &[T], c: &Cancellation) -> AssetResult<()> {
    if a.len() != b.len() {
        return Err(mismatch());
    }
    for (a, b) in a.chunks(4096).zip(b.chunks(4096)) {
        c.check()?;
        if a != b {
            return Err(mismatch());
        }
    }
    Ok(())
}
fn optional<T: PartialEq>(a: Option<&[T]>, b: Option<&[T]>, c: &Cancellation) -> AssetResult<()> {
    match (a, b) {
        (None, None) => Ok(()),
        (Some(a), Some(b)) => equal(a, b, c),
        _ => Err(mismatch()),
    }
}
struct Images {
    pairs: Vec<(usize, usize)>,
}
impl Images {
    fn texture(
        &mut self,
        a: Option<&Arc<Texture>>,
        b: Option<&Arc<Texture>>,
        c: &Cancellation,
    ) -> AssetResult<()> {
        let (a, b) = match (a, b) {
            (None, None) => return Ok(()),
            (Some(a), Some(b)) => (a, b),
            _ => return Err(mismatch()),
        };
        if a.sampler != b.sampler {
            return Err(mismatch());
        }
        self.image(&a.image, &b.image, c)
    }
    fn image(
        &mut self,
        a: &Arc<TextureImage>,
        b: &Arc<TextureImage>,
        c: &Cancellation,
    ) -> AssetResult<()> {
        let pair = (Arc::as_ptr(a) as usize, Arc::as_ptr(b) as usize);
        if self.pairs.contains(&pair) {
            return Ok(());
        }
        if a.width != b.width || a.height != b.height {
            return Err(mismatch());
        }
        equal(&a.rgba, &b.rgba, c)?;
        self.pairs.try_reserve(1).map_err(|_| {
            AssetError::new(
                ErrorCode::ResourceLimit,
                "Cannot retain export image checks",
            )
        })?;
        self.pairs.push(pair);
        Ok(())
    }
}
pub(crate) fn compare(a: &AssetDocument, b: &AssetDocument, c: &Cancellation) -> AssetResult<()> {
    c.check()?;
    if a.nodes().len() != b.nodes().len()
        || a.primitives().len() != b.primitives().len()
        || a.texture_stats() != b.texture_stats()
    {
        return Err(mismatch());
    }
    equal(a.roots(), b.roots(), c)?;
    for (a, b) in a.nodes().iter().zip(b.nodes()) {
        c.check()?;
        if a.name != b.name || a.local != b.local {
            return Err(mismatch());
        }
        equal(&a.children, &b.children, c)?;
        equal(&a.meshes, &b.meshes, c)?;
    }
    let mut images = Images { pairs: Vec::new() };
    for (a, b) in a.primitives().iter().zip(b.primitives()) {
        c.check()?;
        equal(&a.indices, &b.indices, c)?;
        equal(&a.positions, &b.positions, c)?;
        optional(a.normals.as_deref(), b.normals.as_deref(), c)?;
        optional(a.tangents.as_deref(), b.tangents.as_deref(), c)?;
        optional(a.colors.as_deref(), b.colors.as_deref(), c)?;
        if a.uv_bindings != b.uv_bindings
            || a.uv_channels.len() != b.uv_channels.len()
            || a.base_color != b.base_color
            || a.metallic_roughness != b.metallic_roughness
            || a.normal_scale != b.normal_scale
            || a.occlusion_strength != b.occlusion_strength
            || a.emissive_factor != b.emissive_factor
            || a.alpha_cutoff != b.alpha_cutoff
        {
            return Err(mismatch());
        }
        for (a, b) in a.uv_channels.iter().zip(&b.uv_channels) {
            if a.set != b.set {
                return Err(mismatch());
            }
            equal(&a.values, &b.values, c)?;
        }
        for (a, b) in [
            (&a.color_texture, &b.color_texture),
            (&a.metallic_roughness_texture, &b.metallic_roughness_texture),
            (&a.normal_texture, &b.normal_texture),
            (&a.occlusion_texture, &b.occlusion_texture),
            (&a.emissive_texture, &b.emissive_texture),
        ] {
            images.texture(a.as_ref(), b.as_ref(), c)?;
        }
    }
    c.check()
}
