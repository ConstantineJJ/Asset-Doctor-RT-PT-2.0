//! Derived viewport normals. The source mesh remains unchanged.
pub(crate) fn normal_matrix(world: glam::Mat4) -> Option<glam::DMat3> {
    let linear = glam::DMat3::from_cols(
        world.x_axis.truncate().as_dvec3(),
        world.y_axis.truncate().as_dvec3(),
        world.z_axis.truncate().as_dvec3(),
    );
    let determinant = linear.determinant();
    if !determinant.is_finite() || determinant == 0. {
        return None;
    }
    let matrix = linear.inverse().transpose();
    matrix.is_finite().then_some(matrix)
}
pub(crate) fn transform_normal(matrix: Option<glam::DMat3>, source: [f32; 3]) -> [f32; 3] {
    let Some(matrix) = matrix else {
        return [0.; 3];
    };
    let v = matrix * glam::DVec3::from_array(source.map(f64::from));
    let n = v.normalize_or_zero();
    if n.is_finite() {
        n.as_vec3().to_array()
    } else {
        [0.; 3]
    }
}

pub(crate) fn tangent_matrix(world: glam::Mat4) -> Option<(glam::DMat3, f32)> {
    let linear = glam::DMat3::from_cols(
        world.x_axis.truncate().as_dvec3(),
        world.y_axis.truncate().as_dvec3(),
        world.z_axis.truncate().as_dvec3(),
    );
    let determinant = linear.determinant();
    (determinant.is_finite() && determinant != 0.).then_some((linear, determinant.signum() as f32))
}
pub(crate) fn transform_tangent(
    matrix: Option<(glam::DMat3, f32)>,
    normal: [f32; 3],
    source: [f32; 4],
) -> [f32; 4] {
    let Some((matrix, parity)) = matrix else {
        return [0.; 4];
    };
    let n = glam::DVec3::from_array(normal.map(f64::from)).normalize_or_zero();
    if n == glam::DVec3::ZERO {
        return [0.; 4];
    }
    let raw = (matrix
        * glam::DVec3::new(
            f64::from(source[0]),
            f64::from(source[1]),
            f64::from(source[2]),
        ))
    .normalize_or_zero();
    let projected = raw - n * n.dot(raw);
    if projected.length_squared() < 1e-12 {
        return [0.; 4];
    }
    let t = projected.normalize_or_zero();
    if !t.is_finite() || t == glam::DVec3::ZERO {
        return [0.; 4];
    }
    [t.x as f32, t.y as f32, t.z as f32, source[3] * parity]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tangent_linear_transform_and_handedness_follow_mirrored_nonuniform_scale() {
        let world = glam::Mat4::from_scale(glam::vec3(-2., 1., 0.5));
        let n = transform_normal(normal_matrix(world), [1., 1., 1.]);
        let t = transform_tangent(tangent_matrix(world), n, [1., -1., 0., 1.]);
        let tangent = glam::Vec3::new(t[0], t[1], t[2]);
        assert!((tangent - glam::vec3(-2., -1., 0.).normalize()).length() < 1e-6);
        assert!(tangent.dot(glam::Vec3::from_array(n)).abs() < 1e-6);
        assert_eq!(t[3], -1.);
        assert_eq!(
            transform_tangent(tangent_matrix(world), n, [1., -1., 0., -1.])[3],
            1.
        );
        assert_eq!(
            transform_tangent(
                tangent_matrix(glam::Mat4::IDENTITY),
                [0.; 3],
                [1., 0., 0., 1.]
            ),
            [0.; 4]
        );
        assert_eq!(
            transform_tangent(
                tangent_matrix(glam::Mat4::IDENTITY),
                [0., 0., 1.],
                [0., 0., 1., 1.]
            ),
            [0.; 4]
        );
        assert_eq!(
            transform_tangent(
                tangent_matrix(glam::Mat4::IDENTITY),
                [1., 2., 3.],
                [1., 2., 3., 1.]
            ),
            [0.; 4]
        );
        assert_eq!(
            transform_tangent(
                tangent_matrix(glam::Mat4::from_scale(glam::vec3(1., 0., 1.))),
                n,
                [1., 0., 0., 1.]
            ),
            [0.; 4]
        );
    }
    #[test]
    fn inverse_transpose_handles_nonuniform_mirrored_and_singular_scales() {
        let world = glam::Mat4::from_scale(glam::vec3(-2., 1., 0.5));
        let n = transform_normal(normal_matrix(world), [1., 1., 1.]);
        let expected = glam::dvec3(-0.5, 1., 2.).normalize();
        assert!((glam::DVec3::from_array(n.map(f64::from)) - expected).length() < 1e-6);
        assert_eq!(
            transform_normal(
                normal_matrix(glam::Mat4::from_scale(glam::vec3(1., 0., 1.))),
                [0., 1., 0.]
            ),
            [0.; 3]
        );
        assert_eq!(
            transform_normal(normal_matrix(glam::Mat4::IDENTITY), [0.; 3]),
            [0.; 3]
        );
        assert!(
            transform_normal(
                normal_matrix(glam::Mat4::from_scale(glam::Vec3::splat(1e-20))),
                [0., 1., 0.]
            )[1] > 0.99
        );
    }
}
