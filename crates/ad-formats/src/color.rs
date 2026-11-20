use ad_document::{
    AssetError, AssetResult, Cancellation, ErrorCode, Filter, ImportLimits, Mipmap, Texture,
    TextureAdmission, TextureEncoding, TextureImage, TextureSampler, Wrap, mip_storage_bytes,
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
        format!("Cannot decode embedded material image: {error}"),
    )
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Material images exceed the texture budget",
    )
}

#[derive(Default)]
pub struct MaterialImages {
    images: HashMap<usize, Arc<TextureImage>>,
    textures: HashMap<usize, Arc<Texture>>,
    admission: TextureAdmission,
}
impl MaterialImages {
    pub fn load(
        &mut self,
        texture: gltf::Texture<'_>,
        encoding: TextureEncoding,
        bin: &[u8],
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<Arc<Texture>> {
        cancel.check()?;
        if let Some(existing) = self.textures.get(&texture.index()) {
            self.admission.admit(existing, encoding, limits)?;
            return Ok(existing.clone());
        }
        if self.textures.len() >= limits.textures {
            return Err(budget());
        }
        let source = texture.source();
        let image = if let Some(image) = self.images.get(&source.index()) {
            image.clone()
        } else {
            if self.images.len() >= limits.textures
                || self.admission.stats.sampled_images >= limits.textures
            {
                return Err(budget());
            }
            let gltf::image::Source::View { view, mime_type } = source.source() else {
                return Err(AssetError::new(
                    ErrorCode::Unsupported,
                    "Material images must use embedded PNG/JPEG buffer views in this build",
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
                .checked_sub(self.admission.stats.mip_bytes)
                .ok_or_else(budget)?;
            Arc::new(decode(encoded, mime_type, limits, remaining, cancel)?)
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
        let color = Arc::new(Texture {
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
        self.admission.admit(&color, encoding, limits)?;
        self.images.insert(source.index(), color.image.clone());
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
                "Only embedded PNG/JPEG material images are supported",
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
                "Material images above 8 bits per channel are not supported yet",
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

/// ADR-0040 skybox: decodes one PNG/JPEG file the owner chose (no glTF, no
/// network). Format comes from the file signature, not the extension; the side
/// and memory limits are those of material images.
pub fn decode_image_file(encoded: &[u8], cancel: &Cancellation) -> AssetResult<TextureImage> {
    let limits = ImportLimits::default();
    if encoded.len() > limits.source_bytes {
        return Err(budget());
    }
    let mime = if encoded.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if encoded.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else {
        return Err(AssetError::new(
            ErrorCode::Unsupported,
            "Only PNG and JPEG images are supported",
        ));
    };
    decode(encoded, mime, limits, limits.texture_bytes, cancel)
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
    fn exhausted_variant_count_refuses_new_image_before_entering_its_codec() {
        let glb =
            gltf::binary::Glb::from_slice(include_bytes!("../../../tests/fixtures/MR_Checker.glb"))
                .unwrap();
        let mut metadata: serde_json::Value = serde_json::from_slice(&glb.json).unwrap();
        let views = metadata["bufferViews"].as_array_mut().unwrap();
        let view = views.len();
        views.push(serde_json::json!({"buffer":0,"byteOffset":0,"byteLength":7}));
        metadata["images"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"bufferView":view,"mimeType":"image/png"}));
        metadata["textures"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"source":1}));
        let doc = gltf::Document::from_json(serde_json::from_value(metadata).unwrap()).unwrap();
        let bin = glb.bin.as_deref().unwrap();
        let limits = ImportLimits {
            textures: 2,
            ..Default::default()
        };
        let mut images = MaterialImages::default();
        let cancel = Cancellation::default();
        images
            .load(
                doc.textures().next().unwrap(),
                TextureEncoding::Srgb,
                bin,
                limits,
                &cancel,
            )
            .unwrap();
        images
            .load(
                doc.textures().next().unwrap(),
                TextureEncoding::Linear,
                bin,
                limits,
                &cancel,
            )
            .unwrap();
        let error = images
            .load(
                doc.textures().nth(1).unwrap(),
                TextureEncoding::Linear,
                bin,
                limits,
                &cancel,
            )
            .err()
            .unwrap();
        // The malformed PNG would return InvalidFormat if its codec were entered.
        assert_eq!(error.code, ErrorCode::ResourceLimit);
        assert_eq!(
            (
                images.admission.stats.images,
                images.admission.stats.sampled_images
            ),
            (1, 2)
        );
    }
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
    #[test]
    fn image_files_are_sniffed_by_signature_and_other_bytes_are_refused() {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &[10, 20, 30, 40, 50, 60],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let cancel = Cancellation::default();
        let image = decode_image_file(&png, &cancel).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(&image.rgba[..], &[10, 20, 30, 255, 40, 50, 60, 255]);
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(&[200; 12], 2, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(decode_image_file(&jpeg, &cancel).unwrap().width, 2);
        for bad in [&b"GIF89a"[..], &[], &png[1..]] {
            let Err(error) = decode_image_file(bad, &cancel) else {
                panic!("non-image bytes must be refused");
            };
            assert_eq!(error.code, ErrorCode::Unsupported);
        }
        assert!(decode_image_file(&png[..20], &cancel).is_err());
    }
}
