use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, ErrorCode, GeometrySnapshot, ImportLimits,
    NodeId, DocumentView,
};
use glam::{Mat4, Quat, Vec3, Vec4};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
    CubicSpline,
}

#[derive(Clone, Copy)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Transform {
    fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from_array(self.scale),
            Quat::from_array(self.rotation).normalize(),
            Vec3::from_array(self.translation),
        )
    }
}

pub struct Track {
    node: NodeId,
    base: Transform,
    kind: TrackKind,
    interpolation: Interpolation,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
    source_channel: Option<usize>,
    edited: bool,
}

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, message)
}

impl Track {
    pub fn new(
        node: NodeId,
        base: Transform,
        kind: TrackKind,
        interpolation: Interpolation,
        times: Vec<f32>,
        mut values: Vec<[f32; 4]>,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        let stride = if interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if times.is_empty() || times.len().checked_mul(stride) != Some(values.len()) {
            return Err(invalid("Animation input/output counts do not match"));
        }
        let mut last = -1.;
        for (i, &time) in times.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if !time.is_finite() || time < 0. || time <= last {
                return Err(invalid("Animation times must be finite and strictly increasing"));
            }
            last = time;
            let keys = &mut values[i * stride..(i + 1) * stride];
            if keys.iter().flatten().any(|v| !v.is_finite()) {
                return Err(invalid("Non-finite animation key or tangent"));
            }
            if kind == TrackKind::Rotation {
                let value = &mut keys[stride / 2];
                let q = Quat::from_array(*value);
                if (q.length_squared() - 1.).abs() > 0.03 {
                    return Err(invalid("Animation rotation keys must be unit quaternions"));
                }
                *value = q.normalize().to_array();
            }
        }
        Ok(Self {
            node,
            base,
            kind,
            interpolation,
            times,
            values,
            source_channel: None,
            edited: false,
        })
    }

    pub fn from_source_channel(mut self, index: usize) -> Self {
        self.source_channel = Some(index);
        self
    }

    pub fn source_channel(&self) -> Option<usize> {
        self.source_channel
    }

    pub fn is_edited(&self) -> bool {
        self.edited
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn kind(&self) -> TrackKind {
        self.kind
    }

    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    pub fn times(&self) -> &[f32] {
        &self.times
    }

    pub fn values(&self) -> &[[f32; 4]] {
        &self.values
    }

    fn sample(&self, time: f32) -> AssetResult<[f32; 4]> {
        let stride = if self.interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        let at = |index| self.values[index * stride + stride / 2];
        let next = self.times.partition_point(|&key| key <= time);
        if next == 0 {
            return Ok(at(0));
        }
        let prev = next - 1;
        if next == self.times.len() || self.interpolation == Interpolation::Step {
            return Ok(at(prev));
        }
        let dt = self.times[next] - self.times[prev];
        let u = (time - self.times[prev]) / dt;
        let start = at(prev);
        let end = at(next);
        if self.interpolation == Interpolation::Linear && self.kind == TrackKind::Rotation {
            return Ok(Quat::from_array(start)
                .slerp(Quat::from_array(end), u)
                .to_array());
        }
        let start = Vec4::from_array(start);
        let end = Vec4::from_array(end);
        let value = if self.interpolation == Interpolation::CubicSpline {
            let outgoing = Vec4::from_array(self.values[prev * 3 + 2]);
            let incoming = Vec4::from_array(self.values[next * 3]);
            let u2 = u * u;
            let u3 = u2 * u;
            start * (2. * u3 - 3. * u2 + 1.)
                + outgoing * ((u3 - 2. * u2 + u) * dt)
                + end * (-2. * u3 + 3. * u2)
                + incoming * ((u3 - u2) * dt)
        } else {
            start.lerp(end, u)
        };
        if self.kind == TrackKind::Rotation {
            let q = Quat::from_array(value.to_array()).normalize();
            if q.is_finite() {
                return Ok(q.to_array());
            }
            return Err(invalid("Animation produced an invalid rotation"));
        }
        Ok(value.to_array())
    }
}

#[path = "clip_edit.rs"]
mod clip_edit;
pub use clip_edit::ClipEdit;
pub(crate) use clip_edit::animation_history_bytes;

#[derive(Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    tracks: Vec<Arc<Track>>,
    edited: bool,
    id: u64,
    source_clip: Option<usize>,
}

impl AnimationClip {
    pub fn new(name: String, tracks: Vec<Track>, cancel: &Cancellation) -> AssetResult<Self> {
        Self::build(name, tracks.into_iter().map(Arc::new).collect(), false, cancel)
    }

    fn build(
        name: String,
        tracks: Vec<Arc<Track>>,
        edited: bool,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        if tracks.is_empty() {
            return Err(invalid("Animation clip has no channels"));
        }
        let mut targets = HashSet::new();
        let mut duration: f32 = 0.;
        for track in &tracks {
            cancel.check()?;
            if !targets.insert((track.node.0, track.kind)) {
                return Err(invalid("Duplicate animation channel targeting the same property"));
            }
            duration = duration.max(track.times[track.times.len() - 1]);
        }
        Ok(Self {
            name,
            duration,
            tracks,
            edited,
            id: 0,
            source_clip: None,
        })
    }
}

impl AssetDocument {
    pub fn animation_clips(&self) -> &[Arc<AnimationClip>] {
        &self.animation_clips
    }

    pub fn with_animation_clips(
        mut self,
        clips: Vec<AnimationClip>,
        bases: Vec<Option<Transform>>,
    ) -> AssetResult<Self> {
        if self.revision() != 0
            || !self.animation_clips.is_empty()
            || (!bases.is_empty() && bases.len() != self.nodes().len())
            || clips
                .iter()
                .flat_map(|clip| &clip.tracks)
                .any(|track| track.node.0 >= self.nodes().len())
        {
            return Err(invalid("Animation does not match the loaded document"));
        }
        self.source_clip_count = clips.len();
        self.animation_clips = clips
            .into_iter()
            .enumerate()
            .map(|(index, mut clip)| {
                clip.id = index as u64;
                clip.source_clip = Some(index);
                Arc::new(clip)
            })
            .collect();
        self.animation_bases = bases;
        Ok(self)
    }

    /// Rigged and animated documents protect geometry and scene transforms from edits.
    pub fn is_read_only(&self) -> bool {
        self.rig().is_some() || self.source_clip_count != 0
    }

    pub fn source_clip_count(&self) -> usize {
        self.source_clip_count
    }

    pub fn animated_snapshot(
        &self,
        clip: usize,
        time: f32,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<GeometrySnapshot> {
        self.view().animated_snapshot(clip, time, None, limits, cancel)
    }
}

impl DocumentView<'_> {
    pub fn animated_snapshot(
        &self,
        clip: usize,
        time: f32,
        preview: Option<ChannelValue>,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<GeometrySnapshot> {
        let clip = self
            .animation_clip(clip)
            .ok_or_else(|| invalid("Unknown animation clip"))?;
        if !time.is_finite() {
            return Err(invalid("Animation time must be finite"));
        }
        let mut pose = vec![None; self.document().nodes().len()];
        for track in &clip.tracks {
            cancel.check()?;
            let value = track.sample(time.clamp(0., clip.duration))?;
            let transform = pose[track.node.0].get_or_insert(track.base);
            transform.set(track.kind, value);
        }
        if let Some(preview) = preview {
            let base = self.document().animation_base(preview.node)?;
            let value = normalize_key(preview.kind, preview.value)?;
            pose[preview.node.0]
                .get_or_insert(base)
                .set(preview.kind, value);
        }
        let locals: Vec<_> = pose
            .into_iter()
            .map(|transform| transform.map(Transform::matrix))
            .collect();
        self.snapshot_with_pose(Some(&locals), limits, cancel)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelValue {
    pub node: NodeId,
    pub kind: TrackKind,
    pub value: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationKey {
    pub clip: usize,
    pub time: f32,
    pub channel: ChannelValue,
    pub remove: bool,
}

impl Transform {
    fn get(self, kind: TrackKind) -> [f32; 4] {
        match kind {
            TrackKind::Translation => {
                let [x, y, z] = self.translation;
                [x, y, z, 0.]
            }
            TrackKind::Rotation => self.rotation,
            TrackKind::Scale => {
                let [x, y, z] = self.scale;
                [x, y, z, 0.]
            }
        }
    }

    fn set(&mut self, kind: TrackKind, value: [f32; 4]) {
        let [x, y, z, _] = value;
        match kind {
            TrackKind::Translation => self.translation = [x, y, z],
            TrackKind::Rotation => self.rotation = value,
            TrackKind::Scale => self.scale = [x, y, z],
        }
    }
}

fn normalize_key(kind: TrackKind, mut value: [f32; 4]) -> AssetResult<[f32; 4]> {
    if value.iter().any(|v| !v.is_finite()) {
        return Err(invalid("Значение ключа должно быть конечным"));
    }
    if kind == TrackKind::Rotation {
        let q = Quat::from_array(value);
        if (q.length_squared() - 1.).abs() > 0.03 {
            return Err(invalid("Поворот ключа должен быть единичным кватернионом"));
        }
        return Ok(q.normalize().to_array());
    }
    value[3] = 0.;
    Ok(value)
}

impl AnimationClip {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn source_clip(&self) -> Option<usize> {
        self.source_clip
    }

    pub fn tracks(&self) -> &[Arc<Track>] {
        &self.tracks
    }

    pub fn track(&self, node: NodeId, kind: TrackKind) -> Option<&Track> {
        self.tracks.iter().map(Arc::as_ref).find(|t| t.node == node && t.kind == kind)
    }

    pub fn is_edited(&self) -> bool {
        self.edited
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        size_of::<Self>() + self.name.capacity() + self.tracks.capacity() * size_of::<Arc<Track>>()
            + self.tracks.iter().map(|track| {
                size_of::<Track>() + track.times.capacity() * size_of::<f32>()
                    + track.values.capacity() * size_of::<[f32; 4]>()
            }).sum::<usize>()
    }
}

impl AssetDocument {
    fn animation_base(&self, node: NodeId) -> AssetResult<Transform> {
        self.animation_bases.get(node.0).copied().flatten().ok_or_else(|| {
            AssetError::new(ErrorCode::Unsupported, "У этого узла матричная трансформация")
        })
    }

    pub fn can_animate_node(&self, node: NodeId) -> bool {
        self.animation_bases.get(node.0).is_some_and(Option::is_some)
    }

    pub fn animation_value(
        &self,
        clip: usize,
        node: NodeId,
        kind: TrackKind,
        time: f32,
    ) -> AssetResult<[f32; 4]> {
        let clip = self.animation_clips.get(clip).ok_or_else(|| invalid("Unknown animation clip"))?;
        match clip.track(node, kind) {
            Some(track) => track.sample(time),
            None => Ok(self.animation_base(node)?.get(kind)),
        }
    }

    pub fn prepare_animation_key(
        &self,
        expected: crate::DocumentTag,
        key: AnimationKey,
        cancel: &Cancellation,
    ) -> AssetResult<crate::PreparedRemoval> {
        use crate::indices::budget;

        self.verify_tag(expected, cancel)?;
        if !key.time.is_finite() || key.time < 0. {
            return Err(invalid("Время ключа должно быть конечным и неотрицательным"));
        }
        let clip = self.animation_clips.get(key.clip).ok_or_else(|| invalid("Unknown animation clip"))?;
        let ChannelValue { node, kind, value } = key.channel;
        let base = self.animation_base(node)?;
        let mut tracks = Vec::new();
        tracks.try_reserve_exact(clip.tracks.len() + 1).map_err(|_| budget())?;
        tracks.extend(clip.tracks.iter().cloned());
        let index = tracks.iter().position(|t| t.node == node && t.kind == kind);
        let changed = if let Some(index) = index {
            match tracks[index].edit_key(key, cancel)? {
                TrackEdit::Same => false,
                TrackEdit::Replace(track) => {
                    tracks[index] = Arc::new(track);
                    true
                }
                TrackEdit::Remove => {
                    tracks.remove(index);
                    true
                }
            }
        } else if key.remove {
            false
        } else {
            let mut track = Track::new(
                node,
                base,
                kind,
                Interpolation::Linear,
                vec![key.time],
                vec![normalize_key(kind, value)?],
                cancel,
            )?;
            track.edited = true;
            tracks.push(Arc::new(track));
            true
        };
        if tracks.is_empty() {
            return Err(AssetError::new(ErrorCode::Unsupported, "В клипе должен остаться хотя бы один канал"));
        }
        let replacement = if changed {
            let next = clip.with_tracks(tracks, cancel)?;
            let mut clips = self.copy_clip_list(0)?;
            clips[key.clip] = Arc::new(next);
            Some(clips)
        } else {
            None
        };
        self.animation_plan(expected, replacement, cancel)
    }
}

enum TrackEdit {
    Same,
    Remove,
    Replace(Track),
}

impl Track {
    fn edit_key(&self, key: AnimationKey, cancel: &Cancellation) -> AssetResult<TrackEdit> {
        let search = self.times.binary_search_by(|time| time.partial_cmp(&key.time).expect("finite times"));
        let stride = if self.interpolation == Interpolation::CubicSpline { 3 } else { 1 };
        let mut value = key.channel.value;
        if key.remove {
            if search.is_err() {
                return Ok(TrackEdit::Same);
            }
            if self.times.len() == 1 {
                return Ok(TrackEdit::Remove);
            }
        } else {
            if let Ok(i) = search
                && self.values[i * stride + stride / 2] == value
            {
                return Ok(TrackEdit::Same);
            }
            value = normalize_key(self.kind, value)?;
            if self.kind == TrackKind::Rotation {
                let reference = match search {
                    Ok(i) => self.values[i * stride + stride / 2],
                    Err(i) => self.values[i.saturating_sub(1) * stride + stride / 2],
                };
                if Quat::from_array(reference).dot(Quat::from_array(value)) < 0. {
                    value = value.map(|v| -v);
                }
            }
            if let Ok(i) = search
                && self.values[i * stride + stride / 2] == value
            {
                return Ok(TrackEdit::Same);
            }
        }
        let mut times = Vec::new();
        let mut values = Vec::new();
        times.try_reserve_exact(self.times.len() + 1).map_err(|_| crate::indices::budget())?;
        values.try_reserve_exact(self.values.len() + stride).map_err(|_| crate::indices::budget())?;
        for block in self.times.chunks(4096) {
            cancel.check()?;
            times.extend_from_slice(block);
        }
        for block in self.values.chunks(4096) {
            cancel.check()?;
            values.extend_from_slice(block);
        }
        match search {
            Ok(i) if key.remove => {
                times.remove(i);
                values.drain(i * stride..(i + 1) * stride);
            }
            Ok(i) => values[i * stride + stride / 2] = value,
            Err(i) => {
                times.insert(i, key.time);
                if stride == 3 {
                    values.splice(i * stride..i * stride, [[0.; 4], value, [0.; 4]]);
                } else {
                    values.insert(i, value);
                }
            }
        }
        let interpolation = if times.len() == 1 && stride == 3 {
            let only = values[1];
            values.clear();
            values.push(only);
            Interpolation::Linear
        } else {
            self.interpolation
        };
        Ok(TrackEdit::Replace(Self {
            node: self.node,
            base: self.base,
            kind: self.kind,
            interpolation,
            times,
            values,
            source_channel: self.source_channel,
            edited: true,
        }))
    }
}
