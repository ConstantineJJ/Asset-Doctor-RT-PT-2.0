use crate::{AssetError, AssetResult, Cancellation, ErrorCode, MeshPrimitive};

pub const MAX_UV_CHANNELS: usize = 8;
pub struct UvChannel {
    pub set: u32,
    pub values: Vec<[f32; 2]>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UvBindings {
    pub color: Option<u32>,
    pub metallic_roughness: Option<u32>,
    pub normal: Option<u32>,
    pub occlusion: Option<u32>,
    pub emissive: Option<u32>,
}
impl UvBindings {
    pub fn sets(self) -> [Option<u32>; 5] {
        [
            self.color,
            self.metallic_roughness,
            self.normal,
            self.occlusion,
            self.emissive,
        ]
    }
}
impl MeshPrimitive {
    pub fn source_uv(&self, set: u32) -> Option<&[[f32; 2]]> {
        self.uv_channels
            .iter()
            .find(|c| c.set == set)
            .map(|c| c.values.as_slice())
    }
}
pub fn source_uv_bytes(vertices: usize, channels: usize) -> AssetResult<usize> {
    if channels > MAX_UV_CHANNELS {
        return Err(AssetError::new(
            ErrorCode::ResourceLimit,
            "Too many source UV channels",
        ));
    }
    vertices
        .checked_mul(8)
        .and_then(|v| v.checked_mul(channels))
        .ok_or_else(|| AssetError::new(ErrorCode::ResourceLimit, "Source UV byte count overflow"))
}
pub(crate) fn validate(mesh: &MeshPrimitive, cancel: &Cancellation) -> AssetResult<()> {
    source_uv_bytes(mesh.positions.len(), mesh.uv_channels.len())?;
    let invalid = || {
        AssetError::new(
            ErrorCode::InvalidGeometry,
            "Invalid source UV channels or material channel binding",
        )
    };
    let mut previous = None;
    for channel in &mesh.uv_channels {
        if previous.is_some_and(|s| s >= channel.set)
            || channel.values.len() != mesh.positions.len()
        {
            return Err(invalid());
        }
        previous = Some(channel.set);
        for (i, value) in channel.values.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if value.iter().any(|v| !v.is_finite()) {
                return Err(invalid());
            }
        }
    }
    for (set, required) in mesh.uv_bindings.sets().into_iter().zip([
        mesh.color_texture.is_some(),
        mesh.metallic_roughness_texture.is_some(),
        mesh.normal_texture.is_some(),
        mesh.occlusion_texture.is_some(),
        mesh.emissive_texture.is_some(),
    ]) {
        if (required && set.is_none()) || set.is_some_and(|s| mesh.source_uv(s).is_none()) {
            return Err(invalid());
        }
    }
    Ok(())
}
