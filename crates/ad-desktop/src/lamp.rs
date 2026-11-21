//! ADR-0040 lamp as a scene object: direction as azimuth/elevation and a
//! screen-space drag that makes the gizmo follow the pointer. View only; the
//! lamp is never part of the document or its export.

/// Degrees of rotation per logical point of pointer travel.
const STEP: f32 = 0.4;
/// Elevation stays off the poles so the azimuth keeps a meaning.
pub const MAX_ELEVATION: f32 = 89.;

/// Point lamp: perspective matrix (depth 0..1) from `lamp` that holds the
/// model and, for a lamp above the floor, where the model's shadow falls.
/// Returns the matrix and the world size of one shadow texel at the model.
pub fn point_shadow_matrix(
    lamp: [f32; 3],
    bounds: ad_document::Bounds,
    floor_y: Option<f32>,
    texels: u32,
) -> Option<(glam::Mat4, f32)> {
    use glam::Vec3;
    let eye = Vec3::from_array(lamp);
    let (min, max) = (Vec3::from_array(bounds.min), Vec3::from_array(bounds.max));
    if !eye.is_finite() || !min.is_finite() || !max.is_finite() {
        return None;
    }
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(1e-4);
    let mut points = Vec::with_capacity(16);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        points.push(corner);
        // Where the corner's shadow meets the floor, if not too far.
        if let Some(y) = floor_y
            && eye.y > corner.y + radius * 0.05
        {
            let t = (eye.y - y) / (eye.y - corner.y);
            if t.is_finite() && t > 0. && t < 6. {
                points.push(eye + (corner - eye) * t);
            }
        }
    }
    let to_center = (center - eye).try_normalize()?;
    let up = if to_center.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = glam::camera::rh::view::look_at_mat4(eye, center, up);
    let (mut spread, mut near, mut far) = (0f32, f32::INFINITY, 0f32);
    for p in &points {
        let v = view.transform_point3(*p);
        let depth = -v.z;
        if depth <= radius * 1e-3 {
            return None; // The lamp is inside the model's box.
        }
        spread = spread.max(v.x.abs() / depth).max(v.y.abs() / depth);
        near = near.min(depth);
        far = far.max(depth);
    }
    let spread = spread * 1.05;
    let fov = 2. * spread.atan();
    if !(0.01..3.0).contains(&fov) {
        return None;
    }
    let projection = glam::camera::rh::proj::vulkan::perspective(
        fov,
        1.,
        (near * 0.9).max(radius * 1e-3),
        far * 1.05,
    );
    let matrix = projection * view;
    let texel = 2. * spread * (center - eye).length() / texels.max(1) as f32;
    (matrix.is_finite() && texel.is_finite() && texel > 0.).then_some((matrix, texel))
}

/// ADR-0042 lighting presets: lamp direction, colour, key, fill and
/// exposure in one choice. The shading mode and environment are untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightPreset {
    Studio,
    Contrast,
    Soft,
    Top,
    Rim,
    Warm,
}
impl LightPreset {
    pub const ALL: [Self; 6] = [
        Self::Studio,
        Self::Contrast,
        Self::Soft,
        Self::Top,
        Self::Rim,
        Self::Warm,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Studio => "Студия",
            Self::Contrast => "Контраст",
            Self::Soft => "Мягкий",
            Self::Top => "Сверху",
            Self::Rim => "Контровой",
            Self::Warm => "Тёплый вечер",
        }
    }
    pub fn apply(self, lighting: &mut ad_render_vulkan::lighting::ViewLighting) {
        let defaults = ad_render_vulkan::lighting::ViewLighting::default();
        let (direction, key, fill, color) = match self {
            Self::Studio => (defaults.direction, defaults.key, defaults.fill, [1.; 3]),
            Self::Contrast => (from_angles(70., 35.), 5., 0.03, [1.; 3]),
            Self::Soft => (from_angles(30., 60.), 1.6, 0.45, [1.; 3]),
            Self::Top => (from_angles(0., MAX_ELEVATION), 3.5, 0.12, [1.; 3]),
            Self::Rim => (from_angles(180., 25.), 5., 0.2, [1.; 3]),
            Self::Warm => (from_angles(-60., 12.), 3.5, 0.18, kelvin_to_rgb(2800.)),
        };
        lighting.direction = direction;
        lighting.key = key;
        lighting.fill = fill;
        lighting.color = color;
        lighting.exposure_ev = 0.;
        lighting.enabled = true;
    }
}

/// Lamp colour for a colour temperature in kelvin (1000..=12000), scaled so
/// the brightest channel is 1. An approximation of the black-body curve.
pub fn kelvin_to_rgb(kelvin: f32) -> [f32; 3] {
    let t = kelvin.clamp(1000., 12000.) / 100.;
    let red = if t <= 66. {
        255.
    } else {
        329.698_73 * (t - 60.).powf(-0.133_204_76)
    };
    let green = if t <= 66. {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_16 * (t - 60.).powf(-0.075_514_85)
    };
    let blue = if t >= 66. {
        255.
    } else if t <= 19. {
        0.
    } else {
        138.517_73 * (t - 10.).ln() - 305.044_8
    };
    let rgb = [red, green, blue].map(|c| c.clamp(0., 255.) / 255.);
    let max = rgb.iter().copied().fold(0f32, f32::max).max(1e-6);
    rgb.map(|c| c / max)
}

/// (azimuth, elevation) in degrees of a unit direction; azimuth 0 is +Z,
/// 90 is +X.
pub fn angles(d: [f32; 3]) -> (f32, f32) {
    (
        d[0].atan2(d[2]).to_degrees(),
        d[1].clamp(-1., 1.).asin().to_degrees(),
    )
}

pub fn from_angles(azimuth: f32, elevation: f32) -> [f32; 3] {
    let (a, e) = (
        azimuth.to_radians(),
        elevation.clamp(-MAX_ELEVATION, MAX_ELEVATION).to_radians(),
    );
    [a.sin() * e.cos(), e.sin(), a.cos() * e.cos()]
}

/// Moves the lamp for a pointer drag of (dx, dy) logical points. `project`
/// maps a direction to the gizmo's screen position (0..1, y down); each axis
/// turns the way that moves the gizmo with the pointer.
/// Points of the lamp's azimuth orbit and elevation arc on screen.
pub const ORBIT_POINTS: usize = 48;
pub const ARC_POINTS: usize = 25;
/// Lamp handles (ADR-0040 amendment): degrees for a drag (pixels) along a
/// handle whose screen motion is `tangent_px` pixels per degree. A nearly
/// edge-on handle still turns, at most 2 degrees per pixel and 20 per event.
pub fn handle_turn(drag: [f32; 2], tangent_px: [f32; 2]) -> f32 {
    let length2 = tangent_px[0] * tangent_px[0] + tangent_px[1] * tangent_px[1];
    if length2 < 1e-9 {
        return 0.;
    }
    ((drag[0] * tangent_px[0] + drag[1] * tangent_px[1]) / length2.max(0.25)).clamp(-20., 20.)
}
/// Screen guides for the lamp handles: the horizontal orbit at the current
/// elevation, the vertical arc at the current azimuth and the screen motion
/// (viewport units) per degree of azimuth and of elevation at the lamp.
pub struct Guides {
    pub orbit: [Option<[f32; 2]>; ORBIT_POINTS],
    pub arc: [Option<[f32; 2]>; ARC_POINTS],
    pub azimuth: Option<[f32; 2]>,
    pub elevation: Option<[f32; 2]>,
    /// Screen motion (viewport units) per model radius of lamp distance.
    pub distance: Option<[f32; 2]>,
}
pub fn guides(d: [f32; 3], project: impl Fn([f32; 3]) -> Option<[f32; 2]>) -> Guides {
    let (azimuth, elevation) = angles(d);
    let here = project(from_angles(azimuth, elevation));
    let per_degree = |other: Option<[f32; 2]>, sign: f32| {
        let (a, b) = (here?, other?);
        Some([(b[0] - a[0]) * sign, (b[1] - a[1]) * sign])
    };
    let up = elevation + 1. <= MAX_ELEVATION;
    Guides {
        orbit: std::array::from_fn(|i| {
            project(from_angles(
                i as f32 * 360. / ORBIT_POINTS as f32,
                elevation,
            ))
        }),
        arc: std::array::from_fn(|i| {
            let e = -MAX_ELEVATION + i as f32 * 2. * MAX_ELEVATION / (ARC_POINTS - 1) as f32;
            project(from_angles(azimuth, e))
        }),
        azimuth: per_degree(project(from_angles(azimuth + 1., elevation)), 1.),
        elevation: if up {
            per_degree(project(from_angles(azimuth, elevation + 1.)), 1.)
        } else {
            per_degree(project(from_angles(azimuth, elevation - 1.)), -1.)
        },
        distance: None,
    }
}
pub fn drag(
    d: [f32; 3],
    dx: f32,
    dy: f32,
    project: impl Fn([f32; 3]) -> Option<[f32; 2]>,
) -> [f32; 3] {
    let (azimuth, elevation) = angles(d);
    let here = project(d);
    let toward = |candidate: [f32; 3], axis: usize, delta: f32| match (here, project(candidate)) {
        (Some(a), Some(b)) => (b[axis] - a[axis]) * delta >= 0.,
        _ => true,
    };
    let mut az = azimuth;
    if dx != 0. {
        let plus = azimuth + dx.abs() * STEP;
        az = if toward(from_angles(plus, elevation), 0, dx) {
            plus
        } else {
            azimuth - dx.abs() * STEP
        };
    }
    let mut el = elevation;
    if dy != 0. {
        let up = (elevation + dy.abs() * STEP).min(MAX_ELEVATION);
        el = if toward(from_angles(az, up), 1, dy) {
            up
        } else {
            (elevation - dy.abs() * STEP).max(-MAX_ELEVATION)
        };
    }
    from_angles(az, el)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use ad_document::Bounds;
    #[test]
    fn handles_turn_only_their_own_angle_with_the_pointer() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1., 0., -1.],
            max: [1., 2., 1.],
        });
        let (w, h) = (1200u32, 760u32);
        let d = from_angles(30., 25.);
        let project = |v: [f32; 3]| camera.project_direction(v, 1.25, w, h);
        let g = guides(d, project);
        assert!(g.orbit.iter().filter(|p| p.is_some()).count() > ORBIT_POINTS / 2);
        assert!(g.arc.iter().all(Option::is_some));
        for (tangent, which) in [(g.azimuth.unwrap(), 0), (g.elevation.unwrap(), 1)] {
            let px = [tangent[0] * w as f32, tangent[1] * h as f32];
            // Dragging 10 handle-lengths along the handle turns 10 degrees.
            let turn = handle_turn([px[0] * 10., px[1] * 10.], px);
            assert!((turn - 10.).abs() < 1e-3, "{which}: {turn}");
            // A perpendicular drag does not turn.
            assert!(handle_turn([-px[1] * 10., px[0] * 10.], px).abs() < 1e-3);
            let (mut a, mut e) = angles(d);
            if which == 0 {
                a += turn;
            } else {
                e += turn;
            }
            let moved = project(from_angles(a, e)).unwrap();
            let start = project(d).unwrap();
            // The lamp moved along the handle, not elsewhere.
            let shift = [
                (moved[0] - start[0]) * w as f32,
                (moved[1] - start[1]) * h as f32,
            ];
            let along = (shift[0] * px[0] + shift[1] * px[1])
                / (px[0].hypot(px[1]) * shift[0].hypot(shift[1]));
            assert!(along > 0.95, "{which}: {along}");
        }
        assert_eq!(handle_turn([5., 5.], [0., 0.]), 0.);
        assert!(handle_turn([1000., 0.], [0.01, 0.]) <= 20.);
    }
    #[test]
    fn angles_round_trip_and_default_lamp_is_kept() {
        let d = ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION;
        let (a, e) = angles(d);
        let back = from_angles(a, e);
        for i in 0..3 {
            assert!((back[i] - d[i]).abs() < 1e-5);
        }
        assert!((angles(from_angles(-120., 30.)).0 + 120.).abs() < 1e-3);
        let pole = from_angles(0., 95.);
        assert!((angles(pole).1 - MAX_ELEVATION).abs() < 1e-3);
    }
    #[test]
    fn point_shadow_matrix_holds_the_model_and_its_floor_shadow() {
        use glam::Vec3;
        let bounds = ad_document::Bounds {
            min: [-1., 0., -0.5],
            max: [1., 2., 0.5],
        };
        let centre = Vec3::new(0., 1., 0.);
        let radius = Vec3::new(2., 2., 1.).length() * 0.5;
        for direction in [
            ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION,
            from_angles(-120., 20.),
            from_angles(45., 70.),
        ] {
            for distance in [1.2, 2.5, 5.] {
                let lamp = centre + Vec3::from_array(direction) * radius * distance;
                let (m, texel) =
                    point_shadow_matrix(lamp.to_array(), bounds, Some(0.), 2048).unwrap();
                assert!(texel > 0.);
                let inside = |p: Vec3| {
                    let c = m * p.extend(1.);
                    let n = c.truncate() / c.w;
                    c.w > 0.
                        && n.x.abs() <= 1.0001
                        && n.y.abs() <= 1.0001
                        && (-1e-4..=1.0001).contains(&n.z)
                };
                for i in 0..8 {
                    let corner = Vec3::new(
                        if i & 1 == 0 { -1. } else { 1. },
                        if i & 2 == 0 { 0. } else { 2. },
                        if i & 4 == 0 { -0.5 } else { 0.5 },
                    );
                    assert!(inside(corner), "{direction:?} {distance} corner {corner}");
                }
                // Nearer to the lamp means smaller depth.
                let toward = (lamp - centre).normalize();
                let depth = |p: Vec3| {
                    let c = m * p.extend(1.);
                    c.z / c.w
                };
                assert!(depth(centre + toward * 0.3) < depth(centre));
            }
        }
        // A lamp inside the model's box gives no shadow.
        assert!(point_shadow_matrix([0., 1., 0.], bounds, Some(0.), 2048).is_none());
    }
    #[test]
    fn light_presets_are_valid_distinct_and_keep_the_shading_mode() {
        use ad_render_vulkan::lighting::{ShadingMode, ViewLighting};
        let mut seen = Vec::new();
        for preset in LightPreset::ALL {
            let mut l = ViewLighting {
                mode: ShadingMode::DirectPbr,
                enabled: false,
                ..ViewLighting::default()
            };
            preset.apply(&mut l);
            assert!(l.valid(), "{preset:?}");
            assert_eq!(l.mode, ShadingMode::DirectPbr);
            assert!(l.enabled);
            assert!(!seen.contains(&l), "{preset:?} duplicates another preset");
            seen.push(l);
        }
        let mut l = ViewLighting::default();
        LightPreset::Studio.apply(&mut l);
        assert_eq!(l, ViewLighting::default(), "studio is the default look");
    }
    #[test]
    fn colour_temperature_goes_from_warm_to_cool() {
        let warm = kelvin_to_rgb(2700.);
        let day = kelvin_to_rgb(6600.);
        let cool = kelvin_to_rgb(10000.);
        assert!(warm[0] == 1. && warm[2] < 0.6, "{warm:?}");
        assert!(day.iter().all(|c| *c > 0.95), "{day:?}");
        assert!(cool[2] == 1. && cool[0] < 0.85, "{cool:?}");
        for k in [0., 1000., 4000., 20000., f32::MAX] {
            assert!(kelvin_to_rgb(k).iter().all(|c| (0.0..=1.0).contains(c)));
        }
    }
    #[test]
    fn dragging_moves_the_gizmo_with_the_pointer() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        });
        let project = |d: [f32; 3]| camera.project_direction(d, 1.25, 1200, 760);
        for start in [
            ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION,
            from_angles(-150., 20.),
            from_angles(170., -30.),
        ] {
            let Some(before) = project(start) else {
                continue;
            };
            for (dx, dy) in [(20., 0.), (-20., 0.), (0., 20.), (0., -20.)] {
                let moved = drag(start, dx, dy, project);
                let after = project(moved).unwrap();
                let axis = if dx != 0. { 0 } else { 1 };
                let delta = if dx != 0. { dx } else { dy };
                assert!(
                    (after[axis] - before[axis]) * delta > 0.,
                    "start {start:?} drag {dx},{dy}: {before:?} -> {after:?}"
                );
                let len: f32 = moved.iter().map(|v| v * v).sum();
                assert!((len - 1.).abs() < 1e-4);
            }
        }
        // Elevation never passes the pole.
        let mut d = from_angles(0., 80.);
        for _ in 0..100 {
            d = drag(d, 0., -50., project);
        }
        assert!(angles(d).1 <= MAX_ELEVATION + 1e-3);
    }
}
