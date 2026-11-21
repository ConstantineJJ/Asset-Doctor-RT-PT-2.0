//! ADR-0040 skybox: the owner's own panorama file, read-only, never part of the
//! document or its export. Decoding runs off the UI thread.
use ad_document::{Cancellation, TextureImage};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
};

/// A decoded panorama ready for upload.
pub struct Loaded {
    pub name: String,
    /// The file it came from (kept in the view settings).
    pub path: PathBuf,
    pub image: TextureImage,
    /// Display zenith/horizon/ground colours for the environment light.
    pub light: [[f32; 3]; 3],
    /// Not about 2:1: shown, but the owner is told it will look stretched.
    pub not_panorama: bool,
}

/// Reads and decodes one PNG/JPEG panorama.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    );
    let bytes = std::fs::read(path).map_err(|e| format!("Не удалось прочитать {name}: {e}"))?;
    let image = ad_formats::decode_image_file(&bytes, &Cancellation::default())
        .map_err(|e| format!("Скайбокс {name} не загружен: {}", e.message))?;
    let light = ad_render_vulkan::sky::panorama_light_colors(&image)
        .ok_or_else(|| format!("Скайбокс {name}: пустое изображение"))?;
    let ratio = image.width as f32 / image.height as f32;
    Ok(Loaded {
        name,
        path: path.to_path_buf(),
        not_panorama: !(1.9..=2.1).contains(&ratio),
        image,
        light,
    })
}

/// Decodes on a background thread; the receiver yields exactly one result.
pub fn spawn(path: PathBuf) -> Receiver<Result<Loaded, String>> {
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let _ = send.send(load(&path));
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
        let loaded = spawn(good.clone()).recv().unwrap().unwrap();
        assert_eq!(loaded.name, "sky.png");
        assert_eq!((loaded.image.width, loaded.image.height), (64, 32));
        assert!(!loaded.not_panorama);
        assert!(loaded.light[0][2] > loaded.light[2][2], "blue sky above");
        let square = dir.join("square.png");
        std::fs::write(&square, png(32, 32)).unwrap();
        assert!(load(&square).unwrap().not_panorama);
        let text = dir.join("notes.png");
        std::fs::write(&text, b"not an image").unwrap();
        let Err(error) = load(&text) else {
            panic!("text is not an image");
        };
        assert!(error.contains("notes.png"), "{error}");
        assert!(load(&dir.join("missing.png")).is_err());
        // The source file is only read.
        assert_eq!(std::fs::read(&good).unwrap(), png(64, 32));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
