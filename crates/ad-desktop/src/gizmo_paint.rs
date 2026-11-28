//! Shared drawing and pointer math of the model and joint gizmos (owner
//! 2026-11-28): arrow heads on the axes, plane squares that move in two
//! axes, and the turn indicator — an arc from where a ring was grabbed with
//! the angle in degrees. A turn follows the pointer's angle around the
//! gizmo centre, so the farther the pointer, the finer the turn.
use egui::{Color32, Pos2, Stroke, Vec2};

/// Pointer distance (points) from the centre below which a ring drag does
/// not turn: there the angle jumps with every pixel.
pub const DEAD_ZONE: f32 = 14.;
/// Plane squares span this share of the arrow length, from the centre.
pub const PLANE_FROM: f32 = 0.22;
pub const PLANE_TO: f32 = 0.42;
/// Axis pairs of the plane squares: 0 = XY, 1 = YZ, 2 = XZ.
pub const PLANES: [[usize; 2]; 3] = [[0, 1], [1, 2], [0, 2]];
/// The axis a plane square is coloured by (its normal, as in other editors).
pub const PLANE_NORMAL: [usize; 3] = [2, 0, 1];

const OUTLINE: Color32 = Color32::from_rgba_premultiplied(10, 14, 22, 220);

/// A filled arrow head at `tip`, pointing away from `from`. Seen end-on (the
/// axis points at the camera) it is a dot.
pub fn arrow_head(painter: &egui::Painter, from: Pos2, tip: Pos2, size: f32, color: Color32) {
    let dir = tip - from;
    if dir.length() < 6. {
        painter.circle(tip, size * 0.6, color, Stroke::new(1.5, OUTLINE));
        return;
    }
    let d = dir.normalized();
    let side = Vec2::new(-d.y, d.x);
    let apex = tip + d * size * 1.7;
    let left = tip + side * size * 0.75;
    let right = tip - side * size * 0.75;
    painter.add(egui::Shape::convex_polygon(
        vec![apex, left, right],
        color,
        Stroke::new(1.5, OUTLINE),
    ));
}

/// How open a ring looks on screen: 1 for a circle facing the camera, 0
/// edge-on (a line). Rings picked at about the same distance go to the
/// more open one, since an edge-on ring crosses the others.
pub fn openness(points: &[Pos2]) -> f32 {
    if points.len() < 3 {
        return 0.;
    }
    let n = points.len() as f32;
    let c = points.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / n;
    let r2 = points
        .iter()
        .map(|p| (p.to_vec2() - c).length_sq())
        .fold(0f32, f32::max);
    if r2 < 1e-6 {
        return 0.;
    }
    let area: f32 = (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        * 0.5;
    (area.abs() / (std::f32::consts::PI * r2)).clamp(0., 1.)
}
/// Pick score of a ring at pointer distance `d`: lower is better.
pub fn ring_score(d: f32, openness: f32) -> f32 {
    d + (1. - openness) * 5.
}

/// Whether `p` is inside the convex quad `quad` (any winding).
pub fn in_quad(p: Pos2, quad: &[Pos2; 4]) -> bool {
    let mut sign = 0.;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let cross = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0. {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.
}

/// A plane square: translucent fill, brighter while hot.
pub fn plane(painter: &egui::Painter, quad: &[Pos2; 4], color: Color32, hot: bool) {
    let fill = color.gamma_multiply(if hot { 0.75 } else { 0.38 });
    let stroke = Stroke::new(if hot { 2. } else { 1.2 }, color);
    painter.add(egui::Shape::convex_polygon(quad.to_vec(), fill, stroke));
}

/// Solves `drag = a·u + b·v` for the two screen vectors of a plane's axes
/// (each: screen motion for one unit along that axis). None when the plane
/// is seen edge-on and the split is unreliable.
pub fn split(drag: [f32; 2], u: [f32; 2], v: [f32; 2]) -> Option<(f32, f32)> {
    let det = u[0] * v[1] - u[1] * v[0];
    let scale = (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1e-9);
    if det.abs() < scale * 0.12 {
        return None;
    }
    let a = (drag[0] * v[1] - drag[1] * v[0]) / det;
    let b = (u[0] * drag[1] - u[1] * drag[0]) / det;
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

/// The pointer's turn around a centre during one ring drag (screen angles,
/// y down; positive is clockwise on screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnTrack {
    /// Screen angle where the ring was grabbed.
    pub start: f32,
    /// Turn since then, unwrapped (may pass ±180°).
    pub swept: f32,
    /// Last pointer angle that counted.
    last: f32,
}
impl TurnTrack {
    /// None when the grab is too close to the centre to give an angle.
    pub fn new(centre: Pos2, pointer: Pos2) -> Option<Self> {
        let a = angle(centre, pointer)?;
        Some(Self {
            start: a,
            swept: 0.,
            last: a,
        })
    }
    /// Follows the pointer; returns this step (radians), 0 inside the dead
    /// zone. Steps never jump across ±180°.
    pub fn step(&mut self, centre: Pos2, pointer: Pos2) -> f32 {
        let Some(a) = angle(centre, pointer) else {
            return 0.;
        };
        let step = wrap(a - self.last);
        self.last = a;
        self.swept += step;
        step
    }
}

fn angle(centre: Pos2, p: Pos2) -> Option<f32> {
    let d = p - centre;
    (d.length() >= DEAD_ZONE).then(|| d.y.atan2(d.x))
}

/// Wraps an angle difference into (-π, π].
pub fn wrap(mut a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

/// The turn indicator: a translucent sector from the grab direction through
/// the swept angle out to the pointer, the two rays, and the angle in
/// degrees next to the pointer.
pub fn turn_indicator(
    painter: &egui::Painter,
    centre: Pos2,
    pointer: Pos2,
    track: &TurnTrack,
    degrees: f32,
    color: Color32,
) {
    let radius = (pointer - centre).length().max(DEAD_ZONE);
    let ray = |a: f32| centre + Vec2::angled(a) * radius;
    let steps = ((track.swept.abs() / 0.05).ceil() as usize).clamp(2, 400);
    let mut fan = vec![centre];
    for i in 0..=steps {
        fan.push(ray(track.start + track.swept * i as f32 / steps as f32));
    }
    // A fan of thin triangles: convex pieces even past half a turn.
    for pair in fan[1..].windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![centre, pair[0], pair[1]],
            color.gamma_multiply(0.18),
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        fan[1..].to_vec(),
        Stroke::new(1.5, color),
    ));
    painter.line_segment(
        [centre, ray(track.start)],
        Stroke::new(1.2, color.gamma_multiply(0.7)),
    );
    painter.line_segment([centre, pointer], Stroke::new(1.5, Color32::WHITE));
    let text = format!("{degrees:+.1}°");
    let at = pointer + Vec2::new(16., -18.);
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(14.), Color32::WHITE);
    let rect = egui::Rect::from_min_size(at, galley.size()).expand2(Vec2::new(6., 3.));
    painter.rect_filled(rect, 4., Color32::from_rgba_unmultiplied(12, 14, 20, 220));
    painter.galley(at, galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_is_unwrapped_and_ignores_the_centre() {
        let c = Pos2::new(100., 100.);
        let mut t = TurnTrack::new(c, Pos2::new(200., 100.)).unwrap();
        // A full clockwise circle on screen in small steps.
        for i in 1..=36 {
            let a = i as f32 * 10f32.to_radians();
            t.step(c, c + Vec2::angled(a) * 100.);
        }
        assert!(
            (t.swept - std::f32::consts::TAU).abs() < 1e-3,
            "{}",
            t.swept
        );
        // Inside the dead zone nothing turns.
        assert_eq!(t.step(c, c + Vec2::new(3., 4.)), 0.);
        assert!(TurnTrack::new(c, c + Vec2::new(5., 0.)).is_none());
        // The same pointer motion is a smaller angle farther out: finer.
        let near = {
            let mut t = TurnTrack::new(c, c + Vec2::new(40., 0.)).unwrap();
            t.step(c, c + Vec2::new(40., 10.))
        };
        let far = {
            let mut t = TurnTrack::new(c, c + Vec2::new(200., 0.)).unwrap();
            t.step(c, c + Vec2::new(200., 10.))
        };
        assert!(far < near * 0.25, "{far} {near}");
    }

    #[test]
    fn plane_drags_split_into_both_axes_and_quads_hit() {
        let (a, b) = split([30., -20.], [10., 0.], [0., -10.]).unwrap();
        assert!((a - 3.).abs() < 1e-5 && (b - 2.).abs() < 1e-5);
        // Edge-on: both axes on one screen line.
        assert!(split([1., 1.], [10., 0.], [20., 0.5]).is_none());
        let quad = [
            Pos2::new(0., 0.),
            Pos2::new(10., 0.),
            Pos2::new(12., 10.),
            Pos2::new(2., 10.),
        ];
        assert!(in_quad(Pos2::new(6., 5.), &quad));
        assert!(!in_quad(Pos2::new(-1., 5.), &quad));
        let reversed = [quad[3], quad[2], quad[1], quad[0]];
        assert!(in_quad(Pos2::new(6., 5.), &reversed));
        // A circle is open, a line is not.
        let circle: Vec<Pos2> = (0..32)
            .map(|i| Pos2::new(0., 0.) + Vec2::angled(i as f32 / 32. * std::f32::consts::TAU) * 50.)
            .collect();
        assert!(openness(&circle) > 0.95);
        let line: Vec<Pos2> = (0..32).map(|i| Pos2::new(0., i as f32)).collect();
        assert!(openness(&line) < 0.05);
        assert!(ring_score(1., openness(&circle)) < ring_score(0., openness(&line)));
    }
}
