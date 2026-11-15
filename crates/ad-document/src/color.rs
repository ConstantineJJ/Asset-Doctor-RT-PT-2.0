use crate::{AssetError, AssetResult, ErrorCode, ImportLimits};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    #[default]
    Linear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wrap {
    #[default]
    Repeat,
    MirroredRepeat,
    ClampToEdge,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mipmap {
    None,
    Nearest,
    #[default]
    Linear,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TextureSampler {
    pub min: Filter,
    pub mag: Filter,
    pub mipmap: Mipmap,
    pub u: Wrap,
    pub v: Wrap,
}
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}
pub struct ColorTexture {
    pub image: Arc<TextureImage>,
    pub sampler: TextureSampler,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct TextureStats {
    pub textures: usize,
    pub images: usize,
    pub rgba_bytes: usize,
    pub mip_bytes: usize,
}

pub fn mip_storage_bytes(width: u32, height: u32) -> AssetResult<usize> {
    if width == 0 || height == 0 {
        return Err(AssetError::new(
            ErrorCode::InvalidGeometry,
            "Texture dimensions must be nonzero",
        ));
    }
    let (mut w, mut h) = (width as usize, height as usize);
    let mut total = 0usize;
    loop {
        total = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| total.checked_add(v))
            .ok_or_else(|| AssetError::new(ErrorCode::ResourceLimit, "Texture storage overflow"))?;
        if w == 1 && h == 1 {
            return Ok(total);
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
}
#[derive(Default)]
pub struct TextureAdmission {
    images: std::collections::HashMap<usize, Arc<TextureImage>>,
    textures: std::collections::HashMap<usize, Arc<ColorTexture>>,
    pub stats: TextureStats,
}
impl TextureAdmission {
    pub fn admit(&mut self, texture: &Arc<ColorTexture>, limits: ImportLimits) -> AssetResult<()> {
        let texture_id = Arc::as_ptr(texture) as usize;
        let image_id = Arc::as_ptr(&texture.image) as usize;
        let new_texture = !self.textures.contains_key(&texture_id);
        let new_image = !self.images.contains_key(&image_id);
        let mut next = self.stats;
        if new_texture {
            next.textures += 1;
        }
        if new_image {
            let size = texture.image.validate(limits)?;
            next.mip_bytes = next.mip_bytes.checked_add(size).ok_or_else(|| {
                AssetError::new(ErrorCode::ResourceLimit, "Texture storage overflow")
            })?;
            next.rgba_bytes += texture.image.rgba.len();
            next.images += 1;
        }
        if next.textures > limits.textures
            || next.images > limits.textures
            || next.mip_bytes > limits.texture_bytes
        {
            return Err(AssetError::new(
                ErrorCode::ResourceLimit,
                "Scene textures exceed the image budget",
            ));
        }
        // Retain owners during the pass so pointer identities cannot be recycled.
        if new_texture {
            self.textures.insert(texture_id, texture.clone());
        }
        if new_image {
            self.images.insert(image_id, texture.image.clone());
        }
        self.stats = next;
        Ok(())
    }
}
impl TextureImage {
    pub fn validate(&self, limits: ImportLimits) -> AssetResult<usize> {
        let size = mip_storage_bytes(self.width, self.height)?;
        if self.width > limits.texture_side
            || self.height > limits.texture_side
            || size > limits.texture_bytes
        {
            return Err(AssetError::new(
                ErrorCode::ResourceLimit,
                "Texture exceeds image budget",
            ));
        }
        if self.rgba.len() != (self.width as usize) * (self.height as usize) * 4 {
            return Err(AssetError::new(
                ErrorCode::InvalidGeometry,
                "Image RGBA size does not match dimensions",
            ));
        }
        Ok(size)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_texture_does_not_poison_admission_and_shared_pixels_count_once() {
        let image = Arc::new(TextureImage {
            width: 2,
            height: 2,
            rgba: Arc::from([255; 16]),
        });
        let a = Arc::new(ColorTexture {
            image: image.clone(),
            sampler: TextureSampler::default(),
        });
        let b = Arc::new(ColorTexture {
            image,
            sampler: TextureSampler {
                u: Wrap::ClampToEdge,
                ..Default::default()
            },
        });
        let mut admission = TextureAdmission::default();
        assert!(
            admission
                .admit(
                    &a,
                    ImportLimits {
                        texture_bytes: 19,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert_eq!(admission.stats.textures, 0);
        admission.admit(&a, ImportLimits::default()).unwrap();
        admission.admit(&b, ImportLimits::default()).unwrap();
        assert_eq!(
            (
                admission.stats.textures,
                admission.stats.images,
                admission.stats.mip_bytes
            ),
            (2, 1, 20)
        );
    }
    #[test]
    fn mip_budget_counts_non_square_tail_and_refuses_invalid_dimensions() {
        assert_eq!(mip_storage_bytes(4, 2).unwrap(), 44);
        assert_eq!(mip_storage_bytes(1, 8).unwrap(), 60);
        assert!(mip_storage_bytes(0, 2).is_err());
        let image = TextureImage {
            width: 2,
            height: 2,
            rgba: Arc::from([255; 16]),
        };
        assert_eq!(image.validate(ImportLimits::default()).unwrap(), 20);
        assert!(
            image
                .validate(ImportLimits {
                    texture_bytes: 16,
                    ..Default::default()
                })
                .is_err()
        );
    }
}
