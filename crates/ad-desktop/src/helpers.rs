//! ADR-0042 viewport helpers: world axes from the origin and the model's
//! bounding box with its size. View only; drawn as an overlay.
use crate::camera::Camera;
use ad_document::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Axis(usize),
    /// Negative half of an axis, drawn dimmer.
    Back(usize),
    Bounds,
}

/// Screen-space helper geometry for one frame (viewport 0..1, y down).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    pub lines: Vec<([f32; 2], [f32; 2], Kind)>,
    pub labels: Vec<([f32; 2], String, Kind)>,
}

/// Lines and labels for the enabled helpers; segments behind the camera are
/// clipped at the near plane, fully hidden ones are left out.
pub fn build(
    camera: &Camera,
    bounds: Bounds,
    axes: bool,
    box_: bool,
    width: u32,
    height: u32,
) -> Overlay {
    let mut overlay = Overlay::default();
    let extent = (0..3)
        .map(|a| bounds.max[a] - bounds.min[a])
        .fold(0f32, f32::max)
        .max(1e-6);
    let mut push = |a: [f32; 3], b: [f32; 3], kind: Kind| {
        if let Some((p, q)) = camera.project_segment(a, b, width, height) {
            overlay.lines.push((p, q, kind));
        }
    };
    if axes {
        let len = extent * 0.75;
        for axis in 0..3 {
            let mut end = [0.; 3];
            end[axis] = len;
            push([0.; 3], end, Kind::Axis(axis));
            end[axis] = -len;
            push([0.; 3], end, Kind::Back(axis));
        }
    }
    if box_ {
        let corner = |i: usize| {
            [0, 1, 2].map(|axis| {
                if i >> axis & 1 == 1 {
                    bounds.max[axis]
                } else {
                    bounds.min[axis]
                }
            })
        };
        for i in 0..8usize {
            for axis in 0..3 {
                let j = i | 1 << axis;
                if j != i {
                    push(corner(i), corner(j), Kind::Bounds);
                }
            }
        }
    }
    if axes {
        for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
            let mut end = [0.; 3];
            end[axis] = extent * 0.8;
            if let Some(p) = camera.project(end, width, height) {
                overlay.labels.push((p, (*name).into(), Kind::Axis(axis)));
            }
        }
    }
    if box_ && let Some(p) = camera.project(bounds.max, width, height) {
        let size = [0, 1, 2].map(|a| bounds.max[a] - bounds.min[a]);
        overlay.labels.push((
            p,
            format!(
                "{} x {} x {}",
                short(size[0]),
                short(size[1]),
                short(size[2])
            ),
            Kind::Bounds,
        ));
    }
    overlay
}

/// Size with three significant digits, no exponent for everyday sizes.
pub fn short(value: f32) -> String {
    if value == 0. {
        return "0".into();
    }
    let digits = (3 - value.abs().log10().floor() as i32 - 1).clamp(0, 6) as usize;
    format!("{value:.digits$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helpers_project_axes_and_twelve_box_edges_with_size() {
        let bounds = Bounds {
            min: [-1., 0., -0.5],
            max: [1., 2.5, 0.5],
        };
        let mut camera = Camera::default();
        camera.fit(bounds);
        let o = build(&camera, bounds, true, true, 1200, 760);
        let count = |k: fn(&Kind) -> bool| o.lines.iter().filter(|l| k(&l.2)).count();
        assert_eq!(count(|k| *k == Kind::Bounds), 12);
        assert_eq!(count(|k| matches!(k, Kind::Axis(_))), 3);
        assert_eq!(count(|k| matches!(k, Kind::Back(_))), 3);
        assert!(o.labels.iter().any(|l| l.1 == "2.00 x 2.50 x 1.00"));
        assert!(
            build(&camera, bounds, false, false, 1200, 760)
                .lines
                .is_empty()
        );
        assert_eq!(short(0.01234), "0.0123");
        assert_eq!(short(1234.), "1234");
    }
}
