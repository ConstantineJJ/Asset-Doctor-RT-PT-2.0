//! SkyBox window library: panorama files of one folder with small previews
//! made on a background thread. Files are only read.
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
};

/// Preview width in pixels (height is half).
pub const THUMB_WIDTH: u32 = 256;

pub enum Thumb {
    Pending,
    Ready(egui::TextureHandle),
    Failed(String),
}
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub thumb: Thumb,
}

#[derive(Default)]
pub struct Library {
    pub folder: Option<PathBuf>,
    pub entries: Vec<Entry>,
    pub error: Option<String>,
    receiver: Option<Receiver<(usize, Result<egui::ColorImage, String>)>>,
}

/// Panorama extensions the SkyBox accepts.
pub fn is_panorama(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "hdr" | "png" | "jpg" | "jpeg"
        )
    })
}

/// Display preview: HDR radiance is exposed to a mid grey average and
/// compressed (Reinhard); PNG/JPEG keep their look. Box reduced to
/// `THUMB_WIDTH` x half.
pub fn thumbnail(path: &Path) -> Result<egui::ColorImage, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    );
    let bytes = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
    let cancel = ad_document::Cancellation::default();
    let (w, h, rgb): (usize, usize, Vec<[f32; 3]>) = if ad_formats::is_hdr(&bytes) {
        let image = ad_formats::decode_hdr_file(&bytes, THUMB_WIDTH, &cancel)
            .map_err(|e| format!("{name}: {}", e.message))?;
        let lum: Vec<f32> = image
            .rgb
            .iter()
            .map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2])
            .collect();
        let mean =
            (lum.iter().map(|l| (l + 1e-4).ln()).sum::<f32>() / lum.len().max(1) as f32).exp();
        let exposure = 0.18 / mean.max(1e-4);
        let rgb = image
            .rgb
            .iter()
            .map(|c| {
                c.map(|v| {
                    let x = (v * exposure).max(0.);
                    srgb(x / (1. + x))
                })
            })
            .collect();
        (image.width as usize, image.height as usize, rgb)
    } else {
        let image = ad_formats::decode_image_file(&bytes, &cancel)
            .map_err(|e| format!("{name}: {}", e.message))?;
        let rgb = image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.))
            .collect();
        (image.width as usize, image.height as usize, rgb)
    };
    let (tw, th) = (THUMB_WIDTH as usize, (THUMB_WIDTH / 2) as usize);
    let mut pixels = Vec::with_capacity(tw * th);
    for y in 0..th {
        let (y0, y1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1).min(h));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1).min(w));
            let (mut sum, mut n) = ([0f32; 3], 0f32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let c = rgb[yy * w + xx];
                    for k in 0..3 {
                        sum[k] += c[k];
                    }
                    n += 1.;
                }
            }
            let c = sum.map(|v| ((v / n.max(1.)).clamp(0., 1.) * 255. + 0.5) as u8);
            pixels.push(egui::Color32::from_rgb(c[0], c[1], c[2]));
        }
    }
    Ok(egui::ColorImage::new([tw, th], pixels))
}
fn srgb(c: f32) -> f32 {
    let c = c.clamp(0., 1.);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

impl Library {
    /// Lists the folder's panoramas (sorted by name) and starts their
    /// previews in the background.
    pub fn open(&mut self, folder: PathBuf) {
        self.error = None;
        self.entries.clear();
        self.receiver = None;
        let mut files: Vec<(PathBuf, u64)> = match std::fs::read_dir(&folder) {
            Ok(read) => read
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| (e.path(), e.metadata().map_or(0, |m| m.len())))
                .filter(|(p, _)| is_panorama(p))
                .collect(),
            Err(e) => {
                self.error = Some(format!("Папка не прочитана: {e}"));
                Vec::new()
            }
        };
        files.sort_by_key(|(p, _)| p.file_name().map(|n| n.to_ascii_lowercase()));
        self.entries = files
            .iter()
            .map(|(path, bytes)| Entry {
                name: path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into()),
                path: path.clone(),
                bytes: *bytes,
                thumb: Thumb::Pending,
            })
            .collect();
        self.folder = Some(folder);
        let paths: Vec<PathBuf> = files.into_iter().map(|(p, _)| p).collect();
        if paths.is_empty() {
            return;
        }
        let (send, receive) = channel();
        std::thread::spawn(move || {
            for (i, path) in paths.iter().enumerate() {
                if send.send((i, thumbnail(path))).is_err() {
                    return; // The library was reopened or closed.
                }
            }
        });
        self.receiver = Some(receive);
    }
    /// Takes finished previews into textures.
    pub fn poll(&mut self, context: &egui::Context) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        while let Ok((i, result)) = receiver.try_recv() {
            if let Some(entry) = self.entries.get_mut(i) {
                entry.thumb = match result {
                    Ok(image) => Thumb::Ready(context.load_texture(
                        format!("skybox_thumb_{}", entry.path.display()),
                        image,
                        egui::TextureOptions::LINEAR,
                    )),
                    Err(error) => Thumb::Failed(error),
                };
            }
        }
    }
    pub fn pending(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.thumb, Thumb::Pending))
            .count()
    }
}

/// Default library folder: `assets/360.HDR` next to the program or, during
/// development, in the source tree above `target/<profile>`.
pub fn default_folder() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("assets").join("360.HDR"),
        dir.join("../../assets/360.HDR"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_lists_panoramas_sorted_and_makes_previews() {
        let dir = std::env::temp_dir().join(format!("ad-sky-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hdr = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 8 +X 16\n".to_vec();
        for y in 0..8 {
            for _ in 0..16 {
                hdr.extend(if y < 4 {
                    [128, 160, 255, 129]
                } else {
                    [64, 48, 32, 128]
                });
            }
        }
        std::fs::write(dir.join("b_sky.hdr"), &hdr).unwrap();
        std::fs::write(dir.join("a_broken.HDR"), b"#?RADIANCE\n\n-Y 8 +X 16\n").unwrap();
        std::fs::write(dir.join("notes.txt"), b"skip me").unwrap();
        std::fs::write(dir.join("c_sky.exr"), b"not supported").unwrap();
        let mut library = Library::default();
        library.open(dir.clone());
        let names: Vec<_> = library.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a_broken.HDR", "b_sky.hdr"]);
        let preview = thumbnail(&dir.join("b_sky.hdr")).unwrap();
        assert_eq!(preview.size, [256, 128]);
        // Sky above is bluer than the ground below.
        let top = preview.pixels[10 * 256 + 10];
        let bottom = preview.pixels[120 * 256 + 10];
        assert!(
            top.b() > top.r() && bottom.r() > bottom.b(),
            "{top:?} {bottom:?}"
        );
        assert!(thumbnail(&dir.join("a_broken.HDR")).is_err());
        // Background previews arrive.
        let context = egui::Context::default();
        let started = std::time::Instant::now();
        while library.pending() > 0 && started.elapsed().as_secs() < 10 {
            library.poll(&context);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(library.entries[0].thumb, Thumb::Failed(_)));
        assert!(matches!(library.entries[1].thumb, Thumb::Ready(_)));
        let mut missing = Library::default();
        missing.open(dir.join("nowhere"));
        assert!(missing.error.is_some() && missing.entries.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
