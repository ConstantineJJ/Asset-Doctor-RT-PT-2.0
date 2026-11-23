use ad_document::{
    AnimationClip, AssetError, AssetResult, Cancellation, ErrorCode, Interpolation, NodeId, Track,
    TrackKind, Transform,
};
use gltf::accessor::{DataType, Dimensions};
use gltf::animation::{Property, util::ReadOutputs};

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, message)
}

fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Animation data exceeds the import budget",
    )
}

fn collect_keys<T>(
    iter: impl ExactSizeIterator<Item = T>,
    cancel: &Cancellation,
) -> AssetResult<Vec<T>> {
    let mut keys = Vec::new();
    keys.try_reserve_exact(iter.len()).map_err(|_| budget())?;
    for (i, key) in iter.enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        keys.push(key);
    }
    Ok(keys)
}

pub(crate) fn read_clip(
    source: gltf::Animation<'_>,
    bin: &[u8],
    remaining: &mut usize,
    cancel: &Cancellation,
) -> AssetResult<AnimationClip> {
    let mut tracks = Vec::new();
    for channel in source.channels() {
        cancel.check()?;
        let sampler = channel.sampler();
        let target = channel.target();
        let node = target.node();
        if !matches!(node.transform(), gltf::scene::Transform::Decomposed { .. }) {
            return Err(invalid("Animation target must use TRS transforms"));
        }
        let (kind, dimensions) = match target.property() {
            Property::Translation => (TrackKind::Translation, Dimensions::Vec3),
            Property::Rotation => (TrackKind::Rotation, Dimensions::Vec4),
            Property::Scale => (TrackKind::Scale, Dimensions::Vec3),
            Property::MorphTargetWeights => {
                return Err(AssetError::new(
                    ErrorCode::Unsupported,
                    "Анимация морф-целей пока не поддерживается.",
                ));
            }
        };
        let input = sampler.input();
        let output = sampler.output();
        let output_type_valid = match output.data_type() {
            DataType::F32 => !output.normalized(),
            DataType::I8 | DataType::U8 | DataType::I16 | DataType::U16 => {
                kind == TrackKind::Rotation && output.normalized()
            }
            _ => false,
        };
        if input.dimensions() != Dimensions::Scalar
            || input.data_type() != DataType::F32
            || input.normalized()
            || output.dimensions() != dimensions
            || !output_type_valid
        {
            return Err(invalid("Invalid animation time or TRS accessor"));
        }
        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };
        let stride = if interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if input.count().checked_mul(stride) != Some(output.count()) {
            return Err(invalid("Animation input/output counts do not match"));
        }
        let bytes = input
            .count()
            .checked_mul(size_of::<f32>())
            .and_then(|n| n.checked_add(size_of::<Track>()))
            .and_then(|n| n.checked_add(output.count().checked_mul(size_of::<[f32; 4]>())?))
            .ok_or_else(budget)?;
        *remaining = remaining.checked_sub(bytes).ok_or_else(budget)?;
        let reader = channel.reader(|_| Some(bin));
        let times = collect_keys(
            reader
                .read_inputs()
                .ok_or_else(|| invalid("Cannot read animation times"))?,
            cancel,
        )?;
        let values = match reader.read_outputs() {
            Some(ReadOutputs::Translations(values) | ReadOutputs::Scales(values)) => {
                collect_keys(values.map(|[x, y, z]| [x, y, z, 0.]), cancel)?
            }
            Some(ReadOutputs::Rotations(values)) => collect_keys(values.into_f32(), cancel)?,
            _ => return Err(invalid("Cannot read animation values")),
        };
        let (translation, rotation, scale) = node.transform().decomposed();
        tracks.push(Track::new(
            NodeId(node.index()),
            Transform {
                translation,
                rotation,
                scale,
            },
            kind,
            interpolation,
            times,
            values,
            cancel,
        )?);
    }
    let name = source
        .name()
        .filter(|name| !name.is_empty())
        .map(|name| name.chars().filter(|c| !c.is_control()).take(120).collect())
        .unwrap_or_else(|| format!("Клип {}", source.index() + 1));
    AnimationClip::new(name, tracks, cancel)
}
