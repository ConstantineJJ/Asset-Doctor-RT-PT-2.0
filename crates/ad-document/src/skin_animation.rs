use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, ErrorCode, GeometrySnapshot, ImportLimits,
    NodeId,
};
use glam::{Mat4, Quat, Vec3, Vec4};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
        })
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

pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    tracks: Vec<Track>,
}

impl AnimationClip {
    pub fn new(name: String, tracks: Vec<Track>, cancel: &Cancellation) -> AssetResult<Self> {
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
        })
    }
}

impl AssetDocument {
    pub fn animation_clips(&self) -> &[AnimationClip] {
        &self.animation_clips
    }

    pub fn with_animation_clips(mut self, clips: Vec<AnimationClip>) -> AssetResult<Self> {
        if self.revision() != 0
            || !self.animation_clips.is_empty()
            || clips
                .iter()
                .flat_map(|clip| &clip.tracks)
                .any(|track| track.node.0 >= self.nodes().len())
        {
            return Err(invalid("Animation does not match the loaded document"));
        }
        self.animation_clips = clips;
        Ok(self)
    }

    pub fn is_read_only(&self) -> bool {
        self.rig().is_some() || !self.animation_clips.is_empty()
    }

    pub fn animated_snapshot(
        &self,
        clip: usize,
        time: f32,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<GeometrySnapshot> {
        let clip = self
            .animation_clips
            .get(clip)
            .ok_or_else(|| invalid("Unknown animation clip"))?;
        if !time.is_finite() {
            return Err(invalid("Animation time must be finite"));
        }
        let mut pose = vec![None; self.nodes().len()];
        for track in &clip.tracks {
            cancel.check()?;
            let value = track.sample(time.clamp(0., clip.duration))?;
            let transform = pose[track.node.0].get_or_insert(track.base);
            let xyz = [value[0], value[1], value[2]];
            match track.kind {
                TrackKind::Translation => transform.translation = xyz,
                TrackKind::Rotation => transform.rotation = value,
                TrackKind::Scale => transform.scale = xyz,
            }
        }
        let locals: Vec<_> = pose
            .into_iter()
            .map(|transform| transform.map(Transform::matrix))
            .collect();
        self.view().snapshot_with_pose(Some(&locals), limits, cancel)
    }
}
