use super::{AnimationClip, Interpolation, Track, TrackKind, invalid};
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId, NodeId,
    PreparedRemoval, RemovalPreview,
    indices::{Alternate, budget},
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum ClipEdit {
    Create { name: String, node: NodeId },
    Rename { clip: usize, name: String },
    Duplicate { clip: usize, name: String },
    Delete { clip: usize },
    Retime { clip: usize, duration: f32 },
}

impl AssetDocument {
    pub(crate) fn copy_clip_list(&self, extra: usize) -> AssetResult<Vec<Arc<AnimationClip>>> {
        let mut clips = Vec::new();
        clips
            .try_reserve_exact(
                self.animation_clips
                    .len()
                    .checked_add(extra)
                    .ok_or_else(budget)?,
            )
            .map_err(|_| budget())?;
        clips.extend(self.animation_clips.iter().cloned());
        Ok(clips)
    }

    pub(super) fn animation_plan(
        &self,
        expected: DocumentTag,
        clips: Option<Vec<Arc<AnimationClip>>>,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        let retained = clips.as_ref().map_or(Ok(0), |next| {
            animation_history_bytes(&self.animation_clips, next, cancel)
        })?;
        let mesh = self
            .parts
            .meshes
            .first()
            .ok_or_else(|| invalid("Animation requires a mesh"))?;
        let triangles = mesh.indices.len() / 3;
        let vertices = mesh.positions.len();
        Ok(PreparedRemoval::new(
            RemovalPreview {
                tag: expected,
                mesh: MeshId(0),
                before_triangles: triangles,
                after_triangles: triangles,
                before_vertices: vertices,
                after_vertices: vertices,
            },
            self.origin.clone(),
            clips.map(Alternate::Animation),
            retained,
        ))
    }

    pub fn prepare_clip_edit(
        &self,
        expected: DocumentTag,
        edit: &ClipEdit,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.verify_tag(expected, cancel)?;
        if !self.is_read_only() {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Создание клипов доступно для GLB со скином или импортированной анимацией.",
            ));
        }
        let add = matches!(edit, ClipEdit::Create { .. } | ClipEdit::Duplicate { .. });
        let mut clips = self.copy_clip_list(usize::from(add))?;
        let current = |index: usize| {
            self.animation_clips
                .get(index)
                .ok_or_else(|| invalid("Неизвестный клип"))
        };
        let next_id = || {
            self.revision()
                .checked_add(self.source_clip_count() as u64)
                .ok_or_else(budget)
        };
        match edit {
            ClipEdit::Create { name, node } => {
                let name = clip_name(name)?.to_owned();
                let base = self.animation_base(*node)?;
                let mut track = Track::new(
                    *node,
                    base,
                    TrackKind::Rotation,
                    Interpolation::Linear,
                    vec![0.],
                    vec![base.rotation],
                    cancel,
                )?;
                track.edited = true;
                let mut clip = AnimationClip::build(name, vec![Arc::new(track)], true, cancel)?;
                clip.id = next_id()?;
                clips.push(Arc::new(clip));
            }
            ClipEdit::Rename { clip, name } => {
                let current = current(*clip)?;
                let name = clip_name(name)?;
                if current.name == name {
                    return self.animation_plan(expected, None, cancel);
                }
                let mut renamed = current.as_ref().clone();
                renamed.name = name.to_owned();
                renamed.edited = true;
                clips[*clip] = Arc::new(renamed);
            }
            ClipEdit::Duplicate { clip, name } => {
                let mut copy = current(*clip)?.as_ref().clone();
                copy.name = clip_name(name)?.to_owned();
                copy.id = next_id()?;
                copy.edited = true;
                clips.push(Arc::new(copy));
            }
            ClipEdit::Delete { clip } => {
                current(*clip)?;
                clips.remove(*clip);
            }
            ClipEdit::Retime { clip, duration } => {
                let source = current(*clip)?;
                if !duration.is_finite() || *duration <= 0. || source.duration <= 0. {
                    return Err(invalid(
                        "Для растяжения клипу нужна положительная длительность.",
                    ));
                }
                if source.duration == *duration {
                    return self.animation_plan(expected, None, cancel);
                }
                let forward = f64::from(*duration) / f64::from(source.duration);
                let mut tracks = Vec::new();
                tracks
                    .try_reserve_exact(source.tracks.len())
                    .map_err(|_| budget())?;
                for track in &source.tracks {
                    let retimed = track.retimed(source.duration, *duration, forward, cancel)?;
                    tracks.push(Arc::new(retimed));
                }
                clips[*clip] = Arc::new(source.with_tracks(tracks, cancel)?);
            }
        }
        self.animation_plan(expected, Some(clips), cancel)
    }
}

fn clip_name(name: &str) -> AssetResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().take(121).count() > 120 || name.chars().any(char::is_control)
    {
        return Err(invalid(
            "Название клипа должно содержать от 1 до 120 символов без управляющих знаков.",
        ));
    }
    Ok(name)
}

impl AnimationClip {
    pub(crate) fn with_tracks(
        &self,
        tracks: Vec<Arc<Track>>,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        let mut clip = Self::build(self.name.clone(), tracks, true, cancel)?;
        clip.id = self.id;
        clip.source_clip = self.source_clip;
        Ok(clip)
    }
}

impl Track {
    fn retimed(
        &self,
        from: f32,
        duration: f32,
        forward: f64,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        let mut times = Vec::new();
        times
            .try_reserve_exact(self.times.len())
            .map_err(|_| budget())?;
        for chunk in self.times.chunks(4096) {
            cancel.check()?;
            for &time in chunk {
                let retimed = if time == from {
                    duration
                } else {
                    (f64::from(time) * forward) as f32
                };
                if times.last().is_some_and(|&previous| previous >= retimed) {
                    return Err(invalid(
                        "Новая длительность сближает ключи до одного времени.",
                    ));
                }
                times.push(retimed);
            }
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.values.len())
            .map_err(|_| budget())?;
        if self.interpolation == Interpolation::CubicSpline {
            for (index, value) in self.values.iter().enumerate() {
                if index % 4096 == 0 {
                    cancel.check()?;
                }
                if index % 3 == 1 {
                    values.push(*value);
                    continue;
                }
                let scaled = value.map(|tangent| (f64::from(tangent) / forward) as f32);
                if scaled
                    .iter()
                    .zip(value)
                    .any(|(&new, &old)| !new.is_finite() || (old != 0. && new == 0.))
                {
                    return Err(invalid(
                        "Новая длительность выходит за диапазон касательных.",
                    ));
                }
                values.push(scaled);
            }
        } else {
            for chunk in self.values.chunks(4096) {
                cancel.check()?;
                values.extend_from_slice(chunk);
            }
        }
        let mut retimed = Self::new(
            self.node,
            self.base,
            self.kind,
            self.interpolation,
            times,
            values,
            cancel,
        )?;
        retimed.edited = true;
        retimed.source_channel = self.source_channel;
        Ok(retimed)
    }
}

// Clip identities are assigned on import and appended in ascending order.
// Charge the list allocation and clip data held exclusively by either side.
pub(crate) fn animation_history_bytes(
    current: &Vec<Arc<AnimationClip>>,
    alternate: &Vec<Arc<AnimationClip>>,
    cancel: &Cancellation,
) -> AssetResult<usize> {
    let slot = size_of::<Arc<AnimationClip>>();
    let mut current_bytes = current.capacity().checked_mul(slot).ok_or_else(budget)?;
    let mut alternate_bytes = alternate.capacity().checked_mul(slot).ok_or_else(budget)?;
    let mut current = current.iter().peekable();
    let mut alternate = alternate.iter().peekable();
    loop {
        cancel.check()?;
        match (current.peek(), alternate.peek()) {
            (Some(a), Some(b)) if Arc::ptr_eq(a, b) => {
                current.next();
                alternate.next();
            }
            (Some(a), Some(b)) if a.id == b.id => {
                current_bytes = current_bytes
                    .checked_add(a.retained_bytes())
                    .ok_or_else(budget)?;
                alternate_bytes = alternate_bytes
                    .checked_add(b.retained_bytes())
                    .ok_or_else(budget)?;
                current.next();
                alternate.next();
            }
            (Some(a), Some(b)) if a.id > b.id => {
                alternate_bytes = alternate_bytes
                    .checked_add(b.retained_bytes())
                    .ok_or_else(budget)?;
                alternate.next();
            }
            (Some(a), _) => {
                current_bytes = current_bytes
                    .checked_add(a.retained_bytes())
                    .ok_or_else(budget)?;
                current.next();
            }
            (None, Some(b)) => {
                alternate_bytes = alternate_bytes
                    .checked_add(b.retained_bytes())
                    .ok_or_else(budget)?;
                alternate.next();
            }
            (None, None) => return Ok(current_bytes.max(alternate_bytes)),
        }
    }
}
