use ad_document::{
    AssetError, AssetResult, Cancellation, ColorTexture, ErrorCode, Filter, ImportLimits, Mipmap,
    TextureImage, TextureSampler, Wrap, mip_storage_bytes,
};
use image::{ImageDecoder, ImageReader};
use std::{collections::HashMap, io::Cursor, sync::Arc};

fn image_error(error: image::ImageError) -> AssetError {
    let code = if matches!(error, image::ImageError::Limits(_)) {
        ErrorCode::ResourceLimit
    } else {
        ErrorCode::InvalidFormat
    };
    AssetError::new(
        code,
        format!("Cannot decode embedded base-color image: {error}"),
    )
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Base-color images exceed the texture budget",
    )
}

#[derive(Default)]
pub struct ColorImages {
    images: HashMap<usize, Arc<TextureImage>>,
    textures: HashMap<usize, Arc<ColorTexture>>,
    mip_bytes: usize,
}
impl ColorImages {
    pub fn load(
        &mut self,
        texture: gltf::Texture<'_>,
        bin: &[u8],
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<Arc<ColorTexture>> {
        cancel.check()?;
        if let Some(existing) = self.textures.get(&texture.index()) {
            return Ok(existing.clone());
        }
        if self.textures.len() >= limits.textures {
            return Err(budget());
        }
        let source = texture.source();
        let image = if let Some(image) = self.images.get(&source.index()) {
            image.clone()
        } else {
            if self.images.len() >= limits.textures {
                return Err(budget());
            }
            let gltf::image::Source::View { view, mime_type } = source.source() else {
                return Err(AssetError::new(
                    ErrorCode::Unsupported,
                    "Base-color images must use embedded PNG/JPEG buffer views in this build",
                ));
            };
            let end = view
                .offset()
                .checked_add(view.length())
                .ok_or_else(budget)?;
            let encoded = bin.get(view.offset()..end).ok_or_else(|| {
                AssetError::new(
                    ErrorCode::InvalidFormat,
                    "Image buffer view is outside BIN data",
                )
            })?;
            let remaining = limits
                .texture_bytes
                .checked_sub(self.mip_bytes)
                .ok_or_else(budget)?;
            let image = Arc::new(decode(encoded, mime_type, limits, remaining, cancel)?);
            self.mip_bytes += mip_storage_bytes(image.width, image.height)?;
            self.images.insert(source.index(), image.clone());
            image
        };
        use gltf::texture::{MagFilter, MinFilter, WrappingMode};
        let sampler = texture.sampler();
        let (min, mipmap) = match sampler.min_filter() {
            Some(MinFilter::Nearest) => (Filter::Nearest, Mipmap::None),
            Some(MinFilter::Linear) => (Filter::Linear, Mipmap::None),
            Some(MinFilter::NearestMipmapNearest) => (Filter::Nearest, Mipmap::Nearest),
            Some(MinFilter::LinearMipmapNearest) => (Filter::Linear, Mipmap::Nearest),
            Some(MinFilter::NearestMipmapLinear) => (Filter::Nearest, Mipmap::Linear),
            Some(MinFilter::LinearMipmapLinear) | None => (Filter::Linear, Mipmap::Linear),
        };
        let wrap = |value| match value {
            WrappingMode::ClampToEdge => Wrap::ClampToEdge,
            WrappingMode::MirroredRepeat => Wrap::MirroredRepeat,
            WrappingMode::Repeat => Wrap::Repeat,
        };
        let color = Arc::new(ColorTexture {
            image,
            sampler: TextureSampler {
                min,
                mipmap,
                mag: if sampler.mag_filter() == Some(MagFilter::Nearest) {
                    Filter::Nearest
                } else {
                    Filter::Linear
                },
                u: wrap(sampler.wrap_s()),
                v: wrap(sampler.wrap_t()),
            },
        });
        self.textures.insert(texture.index(), color.clone());
        Ok(color)
    }
}
fn decode(
    encoded: &[u8],
    mime: &str,
    limits: ImportLimits,
    remaining: usize,
    cancel: &Cancellation,
) -> AssetResult<TextureImage> {
    let format = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        _ => {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Only embedded PNG/JPEG base-color images are supported",
            ));
        }
    };
    let reader = || {
        let mut reader = ImageReader::with_format(Cursor::new(encoded), format);
        let mut cap = image::Limits::default();
        cap.max_image_width = Some(limits.texture_side);
        cap.max_image_height = Some(limits.texture_side);
        cap.max_alloc = Some(remaining as u64);
        reader.limits(cap);
        reader
    };
    let (width, height) = {
        let decoder = reader().into_decoder().map_err(image_error)?;
        if decoder.color_type().bits_per_pixel() / u16::from(decoder.color_type().channel_count())
            > 8
        {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Base-color images above 8 bits per channel are not supported yet",
            ));
        }
        decoder.dimensions()
    };
    if mip_storage_bytes(width, height)? > remaining {
        return Err(budget());
    }
    cancel.check()?;
    let rgba = reader()
        .decode()
        .map_err(image_error)?
        .into_rgba8()
        .into_raw();
    cancel.check()?;
    Ok(TextureImage {
        width,
        height,
        rgba: rgba.into(),
    })
}

pub fn validate_attribute(
    accessor: gltf::Accessor<'_>,
    count: usize,
    color: bool,
) -> AssetResult<()> {
    use gltf::accessor::{DataType, Dimensions};
    let layout = if color {
        matches!(accessor.dimensions(), Dimensions::Vec3 | Dimensions::Vec4)
    } else {
        accessor.dimensions() == Dimensions::Vec2
    };
    let encoding = match accessor.data_type() {
        DataType::F32 => !accessor.normalized(),
        DataType::U8 | DataType::U16 => accessor.normalized(),
        _ => false,
    };
    if !layout || !encoding || accessor.count() != count {
        return Err(AssetError::new(
            ErrorCode::InvalidFormat,
            "Invalid UV/COLOR_0 layout, component type or vertex count",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_is_decoded_with_admission_and_cancel_checks() {
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut data)
            .encode(
                &[240, 10, 20, 240, 10, 20, 240, 10, 20, 240, 10, 20],
                2,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let image = decode(
            &data,
            "image/jpeg",
            ImportLimits::default(),
            20,
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!((image.width, image.height, image.rgba.len()), (2, 2, 16));
        assert!(image.rgba[0] > 200 && image.rgba[1] < 40);
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(decode(&data, "image/jpeg", ImportLimits::default(), 256, &cancel).is_err());
    }
}
