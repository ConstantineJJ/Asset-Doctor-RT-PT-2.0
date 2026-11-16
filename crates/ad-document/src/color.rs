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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
pub struct Texture {
    pub image: Arc<TextureImage>,
    pub sampler: TextureSampler,
}
/// Transfer encoding of a sampled use, independent of the stored pixel bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureEncoding {
    Srgb,
    Linear,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct TextureStats {
    pub textures: usize,
    pub images: usize,
    pub sampled_images: usize,
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
    textures: std::collections::HashMap<usize, Arc<Texture>>,
    sampled: std::collections::HashSet<(usize, TextureEncoding)>,
    pub stats: TextureStats,
}
impl TextureAdmission {
    pub fn admit(
        &mut self,
        texture: &Arc<Texture>,
        encoding: TextureEncoding,
        limits: ImportLimits,
    ) -> AssetResult<()> {
        let texture_id = Arc::as_ptr(texture) as usize;
        let image_id = Arc::as_ptr(&texture.image) as usize;
        let new_texture = !self.textures.contains_key(&texture_id);
        let new_image = !self.images.contains_key(&image_id);
        let new_sampled = !self.sampled.contains(&(image_id, encoding));
        let mut next = self.stats;
        if new_texture {
            next.textures += 1;
        }
        if new_sampled {
            let size = texture.image.validate(limits)?;
            next.mip_bytes = next.mip_bytes.checked_add(size).ok_or_else(|| {
                AssetError::new(ErrorCode::ResourceLimit, "Texture storage overflow")
            })?;
            next.sampled_images += 1;
        }
        if new_image {
            next.rgba_bytes += texture.image.rgba.len();
            next.images += 1;
        }
        if next.textures > limits.textures
            || next.images > limits.textures
            || next.sampled_images > limits.textures
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
        self.sampled.insert((image_id, encoding));
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
        let a = Arc::new(Texture {
            image: image.clone(),
            sampler: TextureSampler::default(),
        });
        let b = Arc::new(Texture {
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
                    TextureEncoding::Srgb,
                    ImportLimits {
                        texture_bytes: 19,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert_eq!(admission.stats.textures, 0);
        admission
            .admit(&a, TextureEncoding::Srgb, ImportLimits::default())
            .unwrap();
        admission
            .admit(&b, TextureEncoding::Srgb, ImportLimits::default())
            .unwrap();
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
    fn image_encodings_share_cpu_pixels_but_each_consume_sampled_storage_atomically() {
        let texture = Arc::new(Texture {
            image: Arc::new(TextureImage {
                width: 2,
                height: 2,
                rgba: Arc::from([128; 16]),
            }),
            sampler: TextureSampler::default(),
        });
        let mut admission = TextureAdmission::default();
        admission
            .admit(&texture, TextureEncoding::Srgb, ImportLimits::default())
            .unwrap();
        assert!(
            admission
                .admit(
                    &texture,
                    TextureEncoding::Linear,
                    ImportLimits {
                        texture_bytes: 39,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert_eq!(admission.stats.mip_bytes, 20);
        admission
            .admit(&texture, TextureEncoding::Linear, ImportLimits::default())
            .unwrap();
        admission
            .admit(&texture, TextureEncoding::Linear, ImportLimits::default())
            .unwrap();
        assert_eq!(
            (
                admission.stats.images,
                admission.stats.textures,
                admission.stats.sampled_images,
                admission.stats.rgba_bytes,
                admission.stats.mip_bytes
            ),
            (1, 1, 2, 16, 40)
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
