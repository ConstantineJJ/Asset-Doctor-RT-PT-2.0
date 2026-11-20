//! ADR-0040 lamp as a scene object: direction as azimuth/elevation and a
//! screen-space drag that makes the gizmo follow the pointer. View only; the
//! lamp is never part of the document or its export.

/// Degrees of rotation per logical point of pointer travel.
const STEP: f32 = 0.4;
/// Elevation stays off the poles so the azimuth keeps a meaning.
pub const MAX_ELEVATION: f32 = 89.;

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
