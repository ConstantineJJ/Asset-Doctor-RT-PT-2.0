//! ADR-0047: Radiance HDR (RGBE, `.hdr`) panoramas for the skybox and image
//! based lighting. Read only, never part of a document. The standard
//! orientation (`-Y h +X w`), flat and run-length scanlines are accepted;
//! wide images are box-averaged while decoding, so memory stays bounded.
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode};

/// Linear RGB radiance, row-major from the top row.
#[derive(Clone, Debug, PartialEq)]
pub struct HdrImage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<[f32; 3]>,
}

/// Largest accepted file and side.
const MAX_BYTES: usize = 512 << 20;
const MAX_SIDE: usize = 32768;

fn refused(message: impl Into<String>) -> AssetError {
    AssetError::new(ErrorCode::Unsupported, message.into())
}
fn damaged() -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, "HDR file is damaged or truncated")
}

/// True when the bytes start like a Radiance file.
pub fn is_hdr(bytes: &[u8]) -> bool {
    bytes.starts_with(b"#?RADIANCE") || bytes.starts_with(b"#?RGBE")
}

fn rgbe(p: [u8; 4]) -> [f32; 3] {
    if p[3] == 0 {
        return [0.; 3];
    }
    // Radiance's colr_color: (value + 0.5) * 2^(e - 136).
    let f = 2f32.powi(i32::from(p[3]) - 136);
    [
        (f32::from(p[0]) + 0.5) * f,
        (f32::from(p[1]) + 0.5) * f,
        (f32::from(p[2]) + 0.5) * f,
    ]
}

/// Reads one scanline of `width` RGBE pixels starting at `*at`.
fn scanline(bytes: &[u8], at: &mut usize, width: usize, row: &mut [[u8; 4]]) -> AssetResult<()> {
    let take = |at: &mut usize| -> AssetResult<u8> {
        let b = *bytes.get(*at).ok_or_else(damaged)?;
        *at += 1;
        Ok(b)
    };
    let head = bytes.get(*at..*at + 4).ok_or_else(damaged)?;
    let new_rle = (8..=0x7fff).contains(&width)
        && head[0] == 2
        && head[1] == 2
        && head[2] & 0x80 == 0
        && ((usize::from(head[2]) << 8) | usize::from(head[3])) == width;
    if new_rle {
        *at += 4;
        for channel in 0..4 {
            let mut x = 0;
            while x < width {
                let count = take(at)?;
                if count > 128 {
                    let run = usize::from(count - 128);
                    let value = take(at)?;
                    if run == 0 || x + run > width {
                        return Err(damaged());
                    }
                    for pixel in &mut row[x..x + run] {
                        pixel[channel] = value;
                    }
                    x += run;
                } else {
                    let run = usize::from(count);
                    if run == 0 || x + run > width {
                        return Err(damaged());
                    }
                    for pixel in &mut row[x..x + run] {
                        pixel[channel] = take(at)?;
                    }
                    x += run;
                }
            }
        }
        return Ok(());
    }
    // Flat pixels, with the old run-length form (1, 1, 1, n) repeating the
    // previous pixel n << shift times.
    let mut x = 0;
    let mut shift = 0u32;
    while x < width {
        let p = [take(at)?, take(at)?, take(at)?, take(at)?];
        if p[0] == 1 && p[1] == 1 && p[2] == 1 {
            let run = usize::from(p[3]).checked_shl(shift).ok_or_else(damaged)?;
            if x == 0 || x + run > width || shift > 24 {
                return Err(damaged());
            }
            let previous = row[x - 1];
            for pixel in &mut row[x..x + run] {
                *pixel = previous;
            }
            x += run;
            shift += 8;
        } else {
            row[x] = p;
            x += 1;
            shift = 0;
        }
    }
    Ok(())
}

/// Decodes a Radiance file. Images wider than `max_width` are reduced by a
/// power of two (box average) while decoding.
pub fn decode_hdr_file(
    bytes: &[u8],
    max_width: u32,
    cancel: &Cancellation,
) -> AssetResult<HdrImage> {
    if bytes.len() > MAX_BYTES {
        return Err(AssetError::new(
            ErrorCode::ResourceLimit,
            "HDR file is larger than 512 MiB",
        ));
    }
    if !is_hdr(bytes) {
        return Err(refused("Not a Radiance HDR file"));
    }
    // Header lines up to an empty line, then the resolution line.
    let mut at = 0;
    let mut line = || -> AssetResult<&[u8]> {
        let rest = bytes.get(at..).ok_or_else(damaged)?;
        let end = rest
            .iter()
            .take(4096)
            .position(|b| *b == b'\n')
            .ok_or_else(damaged)?;
        at += end + 1;
        Ok(&rest[..end])
    };
    let mut format = None;
    loop {
        let text = line()?;
        if text.is_empty() {
            break;
        }
        if let Some(value) = text.strip_prefix(b"FORMAT=") {
            format = Some(value.to_vec());
        }
    }
    if format.as_deref().is_some_and(|f| f != b"32-bit_rle_rgbe") {
        return Err(refused("Only RGBE Radiance files are supported (not XYZE)"));
    }
    let resolution = String::from_utf8_lossy(line()?).into_owned();
    let parts: Vec<&str> = resolution.split_whitespace().collect();
    let (height, width) = match parts[..] {
        ["-Y", h, "+X", w] => (
            h.parse::<usize>().map_err(|_| damaged())?,
            w.parse::<usize>().map_err(|_| damaged())?,
        ),
        _ => {
            return Err(refused(
                "Only the standard HDR orientation (-Y height +X width) is supported",
            ));
        }
    };
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(refused("HDR image side must be 1 to 32768 pixels"));
    }
    let mut factor = 1usize;
    while width / factor > max_width.max(1) as usize {
        factor *= 2;
    }
    let (out_w, out_h) = ((width / factor).max(1), (height / factor).max(1));
    let mut sums = vec![[0f32; 3]; out_w * out_h];
    let mut counts = vec![0u32; out_w * out_h];
    let mut row = vec![[0u8; 4]; width];
    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        scanline(bytes, &mut at, width, &mut row)?;
        let oy = (y / factor).min(out_h - 1);
        for (x, pixel) in row.iter().enumerate() {
            let i = oy * out_w + (x / factor).min(out_w - 1);
            let c = rgbe(*pixel);
            for k in 0..3 {
                sums[i][k] += c[k];
            }
            counts[i] += 1;
        }
    }
    for (sum, count) in sums.iter_mut().zip(&counts) {
        let n = (*count).max(1) as f32;
        for v in sum.iter_mut() {
            *v /= n;
        }
    }
    Ok(HdrImage {
        width: out_w as u32,
        height: out_h as u32,
        rgb: sums,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(w: usize, h: usize) -> Vec<u8> {
        format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nEXPOSURE=1.0\n\n-Y {h} +X {w}\n").into_bytes()
    }
    /// RGBE of a value with exponent 129 (2^(129-136) = 1/128).
    fn value(m: u8) -> f32 {
        (f32::from(m) + 0.5) / 128.
    }
    #[test]
    fn flat_and_run_length_scanlines_decode_to_the_same_radiance() {
        let c = Cancellation::default();
        let (w, h) = (16, 2);
        let mut flat = header(w, h);
        let mut rle = header(w, h);
        for y in 0..h {
            let pixels: Vec<[u8; 4]> = (0..w)
                .map(|x| [if x < 10 { 200 } else { x as u8 }, 64, (y * 50) as u8, 129])
                .collect();
            for p in &pixels {
                flat.extend(p);
            }
            rle.extend([2, 2, 0, w as u8]);
            for channel in 0..4 {
                if channel == 0 {
                    // A run of ten, then six literals.
                    rle.extend([128 + 10, 200, 6]);
                    rle.extend(pixels[10..].iter().map(|p| p[0]));
                } else {
                    rle.extend([128 + w as u8, pixels[0][channel]]);
                }
            }
        }
        let a = decode_hdr_file(&flat, 4096, &c).unwrap();
        let b = decode_hdr_file(&rle, 4096, &c).unwrap();
        assert_eq!(a, b);
        assert_eq!((a.width, a.height), (16, 2));
        assert_eq!(a.rgb[0], [value(200), value(64), value(0)]);
        assert_eq!(a.rgb[w + 12], [value(12), value(64), value(50)]);
        // Zero exponent is black; bright values go well above 1.
        assert_eq!(rgbe([255, 0, 0, 0]), [0.; 3]);
        assert!(rgbe([128, 128, 128, 140])[0] > 1000.);
    }
    #[test]
    fn wide_images_are_box_averaged_while_decoding() {
        let c = Cancellation::default();
        let (w, h) = (8, 4);
        let mut bytes = header(w, h);
        for y in 0..h {
            for x in 0..w {
                bytes.extend([((x + y) * 10) as u8, 0, 0, 129]);
            }
        }
        let small = decode_hdr_file(&bytes, 4, &c).unwrap();
        assert_eq!((small.width, small.height), (4, 2));
        let full = decode_hdr_file(&bytes, 4096, &c).unwrap();
        let mean = (full.rgb[0][0] + full.rgb[1][0] + full.rgb[w][0] + full.rgb[w + 1][0]) / 4.;
        assert!((small.rgb[0][0] - mean).abs() < 1e-6);
    }
    #[test]
    fn damaged_or_unsupported_files_are_refused() {
        let c = Cancellation::default();
        let mut truncated = header(16, 2);
        truncated.extend([1, 2, 3, 129]);
        assert_eq!(
            decode_hdr_file(&truncated, 4096, &c).unwrap_err().code,
            ErrorCode::InvalidFormat
        );
        let mut flipped = b"#?RADIANCE\n\n+Y 2 +X 16\n".to_vec();
        flipped.extend([0; 128]);
        assert_eq!(
            decode_hdr_file(&flipped, 4096, &c).unwrap_err().code,
            ErrorCode::Unsupported
        );
        let xyze = b"#?RADIANCE\nFORMAT=32-bit_rle_xyze\n\n-Y 1 +X 1\n\x80\x80\x80\x81".to_vec();
        assert!(decode_hdr_file(&xyze, 4096, &c).is_err());
        assert!(decode_hdr_file(b"\x89PNG", 4096, &c).is_err());
        // A run past the line end.
        let mut overrun = header(16, 1);
        overrun.extend([2, 2, 0, 16, 128 + 17, 1]);
        assert!(decode_hdr_file(&overrun, 4096, &c).is_err());
        let cancelled = Cancellation::default();
        cancelled.cancel();
        let mut ok = header(1, 1);
        ok.extend([1, 1, 1, 129]);
        assert_eq!(
            decode_hdr_file(&ok, 4096, &cancelled).unwrap_err().code,
            ErrorCode::Cancelled
        );
    }
}
