//! ADR-0040 skybox: the owner's own panorama file, read-only, never part of the
//! document or its export. Decoding runs off the UI thread.
use ad_document::{Cancellation, TextureImage};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
};

/// A decoded panorama with its lighting data, ready for upload.
pub struct Loaded {
    pub name: String,
    /// The file it came from (kept in the view settings).
    pub path: PathBuf,
    /// Source size in pixels.
    pub width: u32,
    pub height: u32,
    /// ADR-0047: GPU levels, irradiance and display band colours.
    pub map: ad_render_vulkan::ibl::EnvironmentMap,
    /// Display zenith/horizon/ground colours for the environment light.
    pub light: [[f32; 3]; 3],
    /// Not about 2:1: shown, but the owner is told it will look stretched.
    pub not_panorama: bool,
    /// Reading, decoding and preparing the lighting, milliseconds.
    pub prepare_ms: u128,
}

/// sRGB bytes to linear values, reduced by a power of two to at most
/// `max_width` texels (box average in linear light).
fn linear_reduced(image: &TextureImage, max_width: u32) -> (u32, u32, Vec<[f32; 3]>) {
    let table: Vec<f32> = (0..256)
        .map(|v| ad_render_vulkan::sky::srgb_to_linear(v as f32 / 255.))
        .collect();
    let (w, h) = (image.width as usize, image.height as usize);
    let mut factor = 1;
    while w / factor > max_width as usize {
        factor *= 2;
    }
    let (out_w, out_h) = ((w / factor).max(1), (h / factor).max(1));
    let mut sums = vec![[0f32; 3]; out_w * out_h];
    let mut counts = vec![0u32; out_w * out_h];
    for y in 0..h {
        let oy = (y / factor).min(out_h - 1);
        for x in 0..w {
            let i = (y * w + x) * 4;
            let o = oy * out_w + (x / factor).min(out_w - 1);
            for k in 0..3 {
                sums[o][k] += table[usize::from(image.rgba[i + k])];
            }
            counts[o] += 1;
        }
    }
    for (sum, count) in sums.iter_mut().zip(&counts) {
        let n = (*count).max(1) as f32;
        for v in sum.iter_mut() {
            *v /= n;
        }
    }
    (out_w as u32, out_h as u32, sums)
}

/// Reads and decodes one PNG/JPEG or Radiance HDR panorama and prepares its
/// image based lighting (ADR-0047).
pub fn load(path: &Path, max_width: u32) -> Result<Loaded, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    );
    let started = std::time::Instant::now();
    let bytes = std::fs::read(path).map_err(|e| format!("Не удалось прочитать {name}: {e}"))?;
    let cancel = Cancellation::default();
    let max = max_width.clamp(8, ad_render_vulkan::ibl::MAX_WIDTH);
    let (source, (width, height, rgb)) = if ad_formats::is_hdr(&bytes) {
        let image = ad_formats::decode_hdr_file(&bytes, max, &cancel)
            .map_err(|e| format!("Скайбокс {name} не загружен: {}", e.message))?;
        // The decoder already reduced wide files; the source ratio is kept.
        let size = (image.width, image.height);
        (size, (image.width, image.height, image.rgb))
    } else {
        let image = ad_formats::decode_image_file(&bytes, &cancel)
            .map_err(|e| format!("Скайбокс {name} не загружен: {}", e.message))?;
        ((image.width, image.height), linear_reduced(&image, max))
    };
    let map = ad_render_vulkan::ibl::build(width, height, rgb, max)
        .map_err(|e| format!("Скайбокс {name}: {e}"))?;
    let ratio = source.0 as f32 / source.1 as f32;
    Ok(Loaded {
        name,
        path: path.to_path_buf(),
        width: source.0,
        height: source.1,
        light: map.light,
        map,
        not_panorama: !(1.9..=2.1).contains(&ratio),
        prepare_ms: started.elapsed().as_millis(),
    })
}

/// Decodes on a background thread; the receiver yields exactly one result.
pub fn spawn(path: PathBuf, max_width: u32) -> Receiver<Result<Loaded, String>> {
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let _ = send.send(load(&path, max_width));
    });
    receive
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        let mut data = Vec::new();
        for y in 0..height {
            let c: [u8; 3] = if y < height / 2 {
                [60, 120, 220]
            } else {
                [90, 70, 50]
            };
            for _ in 0..width {
                data.extend_from_slice(&c);
            }
        }
        writer.write_image_data(&data).unwrap();
        drop(writer);
        out
    }
    #[test]
    fn panorama_files_load_with_light_and_bad_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("ad-skybox-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("sky.png");
        std::fs::write(&good, png(64, 32)).unwrap();
        let loaded = spawn(good.clone(), 4096).recv().unwrap().unwrap();
        assert_eq!(loaded.name, "sky.png");
        assert_eq!((loaded.width, loaded.height), (64, 32));
        assert!(!loaded.not_panorama && !loaded.map.hdr);
        assert_eq!(loaded.map.levels.len(), 4, "64, 32, 16, 8");
        assert!(loaded.light[0][2] > loaded.light[2][2], "blue sky above");
        let square = dir.join("square.png");
        std::fs::write(&square, png(32, 32)).unwrap();
        assert!(load(&square, 4096).unwrap().not_panorama);
        let text = dir.join("notes.png");
        std::fs::write(&text, b"not an image").unwrap();
        let Err(error) = load(&text, 4096) else {
            panic!("text is not an image");
        };
        assert!(error.contains("notes.png"), "{error}");
        assert!(load(&dir.join("missing.png"), 4096).is_err());
        // The source file is only read.
        assert_eq!(std::fs::read(&good).unwrap(), png(64, 32));
        // A Radiance HDR panorama with a sun far above 1.
        let hdr = dir.join("studio.hdr");
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 16 +X 32\n".to_vec();
        for y in 0..16 {
            for x in 0..32 {
                bytes.extend(if y == 3 && x == 16 {
                    [128, 128, 128, 140]
                } else {
                    [128, 128, 128, 128]
                });
            }
        }
        std::fs::write(&hdr, &bytes).unwrap();
        let loaded = load(&hdr, 4096).unwrap();
        assert!(loaded.map.hdr && !loaded.not_panorama);
        assert_eq!((loaded.width, loaded.height), (32, 16));
        assert_eq!(std::fs::read(&hdr).unwrap(), bytes);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
