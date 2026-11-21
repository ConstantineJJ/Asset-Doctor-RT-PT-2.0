//! ADR-0047 image based lighting, CPU side (no GPU types). An equirectangular
//! panorama in linear RGB becomes:
//! - a mip chain stored as RGBA16F; level 0 is the panorama for the sky,
//!   levels at most 256 texels wide are GGX-prefiltered for the roughness
//!   `level / last level` (wider levels are box averages, used only for
//!   nearly mirror-like surfaces);
//! - nine spherical-harmonic coefficients of the diffuse irradiance;
//! - zenith/horizon/ground display colours (the former panorama light).
//!
//! Direction ↔ texture mapping is the one of sky_panorama.frag:
//! u = atan(x, -z) / 2π + 0.5, v = acos(y) / π.
use std::f32::consts::PI;

/// SkyBox quality choices (widest level kept on the GPU) and the default.
pub const WIDTHS: [u32; 3] = [2048, 4096, 8192];
pub const DEFAULT_WIDTH: u32 = 4096;
/// Never wider than this.
pub const MAX_WIDTH: u32 = 8192;
/// Levels at most this wide are GGX-prefiltered.
const PREFILTER_WIDTH: usize = 256;
/// Importance samples per prefiltered texel.
const SAMPLES: u32 = 64;
/// The chain stops at this width.
const MIN_WIDTH: usize = 8;

/// A prepared environment, ready for upload.
#[derive(Clone, Debug)]
pub struct EnvironmentMap {
    pub width: u32,
    pub height: u32,
    /// RGBA16F texels (alpha 1) per level, level 0 first.
    pub levels: Vec<Vec<[u16; 4]>>,
    /// Irradiance E(n) = Σ sh[i] · Y_i(n), already convolved with the cosine.
    pub sh: [[f32; 3]; 9],
    /// Radiance values above 1 come from an HDR source.
    pub hdr: bool,
    /// Display zenith, horizon and ground colours (0..1).
    pub light: [[f32; 3]; 3],
}
impl EnvironmentMap {
    pub fn max_lod(&self) -> f32 {
        (self.levels.len().max(1) - 1) as f32
    }
}

#[derive(Clone)]
struct Level {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}
impl Level {
    fn get(&self, x: isize, y: isize) -> [f32; 3] {
        let x = x.rem_euclid(self.w as isize) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        self.px[y * self.w + x]
    }
    /// Bilinear sample at a world direction (repeat in u, clamp in v).
    fn sample(&self, d: [f32; 3]) -> [f32; 3] {
        let (u, v) = uv(d);
        let fx = u * self.w as f32 - 0.5;
        let fy = v * self.h as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let mut out = [0.; 3];
        for (dx, dy, w) in [
            (0, 0, (1. - tx) * (1. - ty)),
            (1, 0, tx * (1. - ty)),
            (0, 1, (1. - tx) * ty),
            (1, 1, tx * ty),
        ] {
            let c = self.get(x0 + dx, y0 + dy);
            for k in 0..3 {
                out[k] += c[k] * w;
            }
        }
        out
    }
    fn half(&self) -> Self {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0.; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let c = self.get((2 * x + dx) as isize, (2 * y + dy) as isize);
                    for k in 0..3 {
                        sum[k] += c[k] * 0.25;
                    }
                }
                px.push(sum);
            }
        }
        Self { w, h, px }
    }
}

/// Texture coordinates of a unit direction.
pub fn uv(d: [f32; 3]) -> (f32, f32) {
    (
        d[0].atan2(-d[2]) / (2. * PI) + 0.5,
        d[1].clamp(-1., 1.).acos() / PI,
    )
}
/// Unit direction at texture coordinates.
pub fn direction(u: f32, v: f32) -> [f32; 3] {
    let (theta, phi) = (v * PI, (u - 0.5) * 2. * PI);
    [
        theta.sin() * phi.sin(),
        theta.cos(),
        -theta.sin() * phi.cos(),
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-20);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Real spherical-harmonic basis, bands 0..2.
pub fn sh_basis(d: [f32; 3]) -> [f32; 9] {
    let [x, y, z] = d;
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3. * z * z - 1.),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}
/// Irradiance at a unit normal from convolved coefficients.
pub fn irradiance(sh: &[[f32; 3]; 9], n: [f32; 3]) -> [f32; 3] {
    let y = sh_basis(n);
    let mut e = [0.; 3];
    for i in 0..9 {
        for k in 0..3 {
            e[k] += sh[i][k] * y[i];
        }
    }
    e.map(|v| v.max(0.))
}

fn radical_inverse(mut bits: u32) -> f32 {
    bits = bits.rotate_right(16);
    bits = ((bits & 0x5555_5555) << 1) | ((bits & 0xAAAA_AAAA) >> 1);
    bits = ((bits & 0x3333_3333) << 2) | ((bits & 0xCCCC_CCCC) >> 2);
    bits = ((bits & 0x0F0F_0F0F) << 4) | ((bits & 0xF0F0_F0F0) >> 4);
    bits = ((bits & 0x00FF_00FF) << 8) | ((bits & 0xFF00_FF00) >> 8);
    bits as f32 * 2.328_306_4e-10
}

/// GGX-filtered radiance around `n` (view = normal), sampling the box
/// chain at a level matching each sample's footprint.
fn prefilter(chain: &[Level], n: [f32; 3], roughness: f32) -> [f32; 3] {
    let a = (roughness * roughness).max(1e-4);
    let a2 = a * a;
    let up = if n[1].abs() < 0.999 {
        [0., 1., 0.]
    } else {
        [1., 0., 0.]
    };
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    let base = &chain[0];
    let texel = 4. * PI / (base.w * base.h) as f32;
    let last = (chain.len() - 1) as f32;
    // Level 0 may already be released (large panoramas): sample from 1 up.
    let first = if base.px.is_empty() { 1. } else { 0. };
    let (mut sum, mut weight) = ([0f32; 3], 0f32);
    for i in 0..SAMPLES {
        let (x1, x2) = (i as f32 / SAMPLES as f32, radical_inverse(i));
        let phi = 2. * PI * x1;
        let cos_t = ((1. - x2) / (1. + (a2 - 1.) * x2)).max(0.).sqrt();
        let sin_t = (1. - cos_t * cos_t).max(0.).sqrt();
        let (hx, hy, hz) = (sin_t * phi.cos(), sin_t * phi.sin(), cos_t);
        let h = [
            t[0] * hx + b[0] * hy + n[0] * hz,
            t[1] * hx + b[1] * hy + n[1] * hz,
            t[2] * hx + b[2] * hy + n[2] * hz,
        ];
        let nh = dot(n, h);
        let l = [
            2. * nh * h[0] - n[0],
            2. * nh * h[1] - n[1],
            2. * nh * h[2] - n[2],
        ];
        let nl = dot(n, l);
        if nl <= 0. {
            continue;
        }
        let d = a2 / (PI * (nh * nh * (a2 - 1.) + 1.).powi(2));
        let pdf = d / 4.;
        let solid = 1. / (SAMPLES as f32 * pdf + 1e-6);
        let lod = (0.5 * (solid / texel).log2() + 1.).clamp(first, last.max(first));
        let c = chain[lod.round() as usize].sample(l);
        for k in 0..3 {
            sum[k] += c[k] * nl;
        }
        weight += nl;
    }
    if weight <= 0. {
        return chain[first as usize].sample(n);
    }
    sum.map(|v| v / weight)
}

/// IEEE half from a non-negative finite value (clamped to the half range).
pub fn half(v: f32) -> u16 {
    let v = if v.is_finite() {
        v.clamp(0., 65504.)
    } else {
        0.
    };
    if v < 6.103_515_6e-5 {
        // Subnormal: units of 2^-24.
        return (v * 16_777_216.).round() as u16;
    }
    let bits = v.to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    let h = ((exponent as u32) << 10) | (mantissa >> 13);
    let round =
        u32::from(mantissa & 0x1fff > 0x1000 || (mantissa & 0x1fff == 0x1000 && h & 1 == 1));
    (h + round).min(0x7bff) as u16
}

fn srgb(c: f32) -> f32 {
    let c = c.clamp(0., 1.);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

/// Builds the environment from a linear RGB panorama (row-major, top row
/// first), at most `max_width` texels wide (power-of-two reduction). Values
/// are clamped to 0..65504; non-finite values become 0.
pub fn build(
    width: u32,
    height: u32,
    rgb: Vec<[f32; 3]>,
    max_width: u32,
) -> Result<EnvironmentMap, String> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgb.len() != w * h {
        return Err("Панорама пуста или повреждена".into());
    }
    let mut base = Level {
        w,
        h,
        px: rgb
            .into_iter()
            .map(|c| {
                c.map(|v| {
                    if v.is_finite() {
                        v.clamp(0., 65504.)
                    } else {
                        0.
                    }
                })
            })
            .collect(),
    };
    while base.w > max_width.clamp(8, MAX_WIDTH) as usize {
        base = base.half();
    }
    let hdr = base.px.iter().any(|c| c.iter().any(|v| *v > 1.001));
    let mut chain = vec![base];
    while chain.last().is_some_and(|l| l.w > MIN_WIDTH && l.h > 1) {
        let next = chain.last().expect("chain").half();
        chain.push(next);
    }
    let last = (chain.len() - 1).max(1) as f32;
    let mut levels = Vec::with_capacity(chain.len());
    // Level 0 is stored at once; for wide panoramas its f32 copy is released
    // (prefiltering reads the coarser levels).
    let half_texels = |px: &[[f32; 3]]| -> Vec<[u16; 4]> {
        px.iter()
            .map(|c| [half(c[0]), half(c[1]), half(c[2]), 0x3c00])
            .collect()
    };
    if chain.len() > 1 && chain[0].w > PREFILTER_WIDTH * 4 {
        levels.push(half_texels(&chain[0].px));
        chain[0].px = Vec::new();
    }
    for (k, level) in chain.iter().enumerate().skip(levels.len()) {
        let filtered = if k > 0 && level.w <= PREFILTER_WIDTH {
            let roughness = k as f32 / last;
            let mut px = Vec::with_capacity(level.w * level.h);
            for y in 0..level.h {
                for x in 0..level.w {
                    let d = direction(
                        (x as f32 + 0.5) / level.w as f32,
                        (y as f32 + 0.5) / level.h as f32,
                    );
                    px.push(prefilter(&chain, d, roughness));
                }
            }
            px
        } else {
            level.px.clone()
        };
        levels.push(half_texels(&filtered));
    }
    // Irradiance from the first level at most 64 wide.
    let small = chain
        .iter()
        .find(|l| l.w <= 64)
        .unwrap_or_else(|| chain.last().expect("chain"));
    let mut sh = [[0f32; 3]; 9];
    let mut bands = [[0f64; 4]; 3];
    for y in 0..small.h {
        let v = (y as f32 + 0.5) / small.h as f32;
        let solid = (2. * PI / small.w as f32) * (PI / small.h as f32) * (v * PI).sin();
        let band = if v < 0.2 {
            Some(0)
        } else if (0.42..0.58).contains(&v) {
            Some(1)
        } else if v > 0.6 {
            Some(2)
        } else {
            None
        };
        for x in 0..small.w {
            let d = direction((x as f32 + 0.5) / small.w as f32, v);
            let c = small.px[y * small.w + x];
            let basis = sh_basis(d);
            for i in 0..9 {
                for k in 0..3 {
                    sh[i][k] += c[k] * basis[i] * solid;
                }
            }
            if let Some(band) = band {
                for k in 0..3 {
                    bands[band][k] += f64::from(c[k]);
                }
                bands[band][3] += 1.;
            }
        }
    }
    // Cosine-lobe convolution per band (π, 2π/3, π/4).
    for (i, coefficient) in sh.iter_mut().enumerate() {
        let a = match i {
            0 => PI,
            1..=3 => 2. * PI / 3.,
            _ => PI / 4.,
        };
        for v in coefficient.iter_mut() {
            *v *= a;
        }
    }
    let light = bands.map(|b| {
        let n = b[3].max(1.);
        [0, 1, 2].map(|k| srgb((b[k] / n) as f32))
    });
    Ok(EnvironmentMap {
        width: chain[0].w as u32,
        height: chain[0].h as u32,
        levels,
        sh,
        hdr,
        light,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_floats_round_like_ieee() {
        assert_eq!(half(0.), 0);
        assert_eq!(half(1.), 0x3c00);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(65504.), 0x7bff);
        assert_eq!(half(1e9), 0x7bff, "clamped, not infinity");
        assert_eq!(half(f32::NAN), 0);
        assert_eq!(half(-3.), 0);
        assert_eq!(half(0.1), 0x2e66);
        assert_eq!(half(2f32.powi(-24)), 1);
        assert_eq!(half(1. + 2f32.powi(-11)), 0x3c00, "tie to even");
    }
    #[test]
    fn mapping_matches_the_sky_shader_and_round_trips() {
        for d in [
            [0., 1., 0.],
            [0., 0., -1.],
            [1., 0., 0.],
            normalize([0.3, -0.5, 0.8]),
        ] {
            let (u, v) = uv(d);
            let back = direction(u, v);
            assert!(dot(back, d) > 0.9999, "{d:?} {back:?}");
        }
        // Straight ahead (-Z) is the centre of the image.
        let (u, v) = uv([0., 0., -1.]);
        assert!((u - 0.5).abs() < 1e-6 && (v - 0.5).abs() < 1e-6);
    }
    #[test]
    fn uniform_sky_gives_uniform_irradiance_and_unchanged_reflections() {
        let (w, h) = (64, 32);
        let map = build(w, h, vec![[0.5, 1.0, 2.0]; (w * h) as usize], MAX_WIDTH).unwrap();
        assert!(map.hdr);
        for n in [
            [0., 1., 0.],
            [0., -1., 0.],
            [1., 0., 0.],
            normalize([1., 1., -1.]),
        ] {
            let e = irradiance(&map.sh, n);
            // E = π · L for a constant radiance L.
            for (k, l) in [0.5, 1.0, 2.0].into_iter().enumerate() {
                assert!((e[k] / PI - l).abs() < 0.02 * l, "{n:?}: {e:?}");
            }
        }
        // Prefiltering a constant keeps it constant.
        for level in &map.levels {
            for t in level {
                assert_eq!(t[1], half(1.), "{t:?}");
            }
        }
        assert_eq!(map.levels[0].len(), 64 * 32);
        assert_eq!(map.levels.len(), 4, "64, 32, 16, 8");
        assert_eq!(map.max_lod(), 3.);
    }
    #[test]
    fn bright_sky_lights_upward_faces_and_rough_levels_blur() {
        let (w, h) = (128usize, 64usize);
        let mut rgb = vec![[0.05f32; 3]; w * h];
        for y in 0..h / 2 {
            for x in 0..w {
                rgb[y * w + x] = [1.; 3];
            }
        }
        // A small very bright sun near the horizon in front (-Z).
        for y in 28..30 {
            for x in 62..66 {
                rgb[y * w + x] = [500.; 3];
            }
        }
        let map = build(w as u32, h as u32, rgb, MAX_WIDTH).unwrap();
        assert!(map.hdr);
        let up = irradiance(&map.sh, [0., 1., 0.]);
        let down = irradiance(&map.sh, [0., -1., 0.]);
        assert!(up[0] > 4. * down[0], "{up:?} {down:?}");
        assert!(
            map.light[0][0] > map.light[2][0],
            "zenith brighter than ground"
        );
        // The sun spreads into its neighbourhood as roughness grows: a texel
        // away from the sun gets brighter in rougher levels.
        let at = |level: usize, d: [f32; 3]| {
            let lw = map.width as usize >> level;
            let lh = map.height as usize >> level;
            let (u, v) = uv(d);
            let x = ((u * lw as f32) as usize).min(lw - 1);
            let y = ((v * lh as f32) as usize).min(lh - 1);
            map.levels[level][y * lw + x][0]
        };
        let aside = normalize([0.5, 0.05, -1.]);
        assert!(
            at(4, aside) > at(1, aside),
            "{} {}",
            at(4, aside),
            at(1, aside)
        );
        assert!(build(0, 0, Vec::new(), MAX_WIDTH).is_err());
        assert!(build(2, 2, vec![[1.; 3]; 3], MAX_WIDTH).is_err());
    }
    #[test]
    fn wide_panoramas_release_level_zero_but_keep_every_level() {
        // 2048 wide: level 0 is stored first and its f32 copy released; the
        // prefiltered levels still match a build that keeps it.
        let (w, h) = (2048usize, 1024usize);
        let rgb: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                if (i / w) < h / 2 {
                    [1., 0.8, 0.6]
                } else {
                    [0.1; 3]
                }
            })
            .collect();
        let map = build(w as u32, h as u32, rgb, 4096).unwrap();
        assert_eq!(map.levels.len(), 9);
        let mut size = (w, h);
        for level in &map.levels {
            assert_eq!(level.len(), size.0 * size.1);
            size = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        }
        assert_eq!(map.levels[0][0], [half(1.), half(0.8), half(0.6), 0x3c00]);
        let up = irradiance(&map.sh, [0., 1., 0.]);
        assert!(up[0] > 2.5 && up[0] < 3.3, "{up:?}");
    }
    #[test]
    fn wide_panoramas_are_reduced_to_the_gpu_width() {
        let map = build(
            4096,
            8,
            vec![[f32::NAN, f32::INFINITY, 0.25]; 4096 * 8],
            2048,
        )
        .unwrap();
        assert_eq!((map.width, map.height), (2048, 4));
        assert_eq!(
            map.levels[0][0],
            [0, 0, half(0.25), 0x3c00],
            "non-finite values become black"
        );
    }
}
