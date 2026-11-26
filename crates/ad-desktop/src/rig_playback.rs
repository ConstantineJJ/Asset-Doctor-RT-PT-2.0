use ad_commands::Session;
use ad_document::{AssetResult, ChannelValue, GeometrySnapshot, ImportLimits};
use ad_render_vulkan::{context::GpuResult, renderer::Renderer};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    edits::AnimationFrame,
    imports::PreparedAsset,
    worker::{Request, Worker},
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Controls {
    pub clip: Option<usize>,
    pub time: f32,
    pub playing: bool,
    pub looping: bool,
    pub speed: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            clip: None,
            time: 0.,
            playing: false,
            looping: true,
            speed: 1.,
        }
    }
}

impl Controls {
    /// Play / pause (the bottom bar and Space). Without a chosen clip the
    /// first one starts; a finished clip starts again from the beginning.
    /// None when there is nothing to play.
    pub fn toggled(self, durations: &[f32]) -> Option<Self> {
        let clip = self.clip.filter(|&i| i < durations.len()).unwrap_or(0);
        let duration = *durations.get(clip)?;
        if duration <= 0. {
            return None;
        }
        let mut next = self;
        if next.clip != Some(clip) {
            next.clip = Some(clip);
            next.time = 0.;
            next.playing = false;
        }
        if !next.playing && next.time >= duration {
            next.time = 0.;
        }
        next.playing = !next.playing;
        Some(next)
    }
    /// The previous (`step` -1) or next (+1) clip, wrapping around, from its
    /// beginning; playback goes on if it was on.
    pub fn switched(self, clips: usize, step: isize) -> Option<Self> {
        if clips == 0 {
            return None;
        }
        let n = clips as isize;
        let clip = match self.clip {
            Some(i) => (i as isize + step).rem_euclid(n),
            None if step < 0 => n - 1,
            None => 0,
        } as usize;
        Some(Self {
            clip: Some(clip),
            time: 0.,
            ..self
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Controls;

    #[test]
    fn toggle_and_switch_clips() {
        let c = Controls::default();
        assert!(c.toggled(&[]).is_none());
        assert!(c.toggled(&[0.]).is_none(), "an empty clip does not play");
        let on = c.toggled(&[2., 3.]).unwrap();
        assert_eq!((on.clip, on.playing, on.time), (Some(0), true, 0.));
        let off = on.toggled(&[2., 3.]).unwrap();
        assert!(!off.playing);
        let ended = Controls { time: 2., ..off };
        assert_eq!(
            ended.toggled(&[2., 3.]).unwrap().time,
            0.,
            "restart at the end"
        );
        let next = Controls { time: 1., ..on }.switched(2, 1).unwrap();
        assert_eq!((next.clip, next.time, next.playing), (Some(1), 0., true));
        assert_eq!(next.switched(2, 1).unwrap().clip, Some(0), "wraps forward");
        assert_eq!(c.switched(3, -1).unwrap().clip, Some(2), "wraps back");
        assert!(c.switched(0, 1).is_none());
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Sample {
    clip: usize,
    time: f32,
    preview: Option<ChannelValue>,
}

struct PoseRequest {
    session: Arc<Session>,
    sample: Sample,
}

pub(crate) struct Playback {
    pub controls: Controls,
    worker: Worker<PoseRequest, GeometrySnapshot>,
    pending: Option<Sample>,
    shown: Option<Sample>,
    preview: Option<ChannelValue>,
    rest: Option<Arc<GeometrySnapshot>>,
    anchor: Option<(Instant, f32)>,
    next_frame: Instant,
}

impl Playback {
    pub fn new() -> Self {
        Self {
            controls: Controls::default(),
            worker: Worker::new(prepare, "Cannot evaluate the animation pose"),
            pending: None,
            shown: None,
            preview: None,
            rest: None,
            anchor: None,
            next_frame: Instant::now(),
        }
    }

    pub fn reset(&mut self) {
        self.worker.cancel();
        self.controls = Controls::default();
        self.pending = None;
        self.shown = None;
        self.preview = None;
        self.rest = None;
        self.anchor = None;
        self.next_frame = Instant::now();
    }

    pub fn apply(&mut self, controls: Controls) {
        if self.controls.clip != controls.clip
            || self.controls.time != controls.time
            || controls.playing
        {
            self.preview(None);
        }
        self.controls = controls;
        self.anchor = controls.playing.then(|| (Instant::now(), controls.time));
    }

    pub fn preview(&mut self, value: Option<ChannelValue>) {
        self.worker.cancel();
        self.pending = None;
        self.preview = value;
        self.next_frame = Instant::now();
    }

    pub fn pause_for_edit(&mut self) {
        self.controls.playing = false;
        self.anchor = None;
        self.preview(None);
    }

    /// The pose on screen: (clip, time, previewed key) or None for the
    /// authored pose.
    pub fn shown_pose(&self) -> Option<(usize, f32, Option<ChannelValue>)> {
        self.shown.map(|s| (s.clip, s.time, s.preview))
    }

    pub fn is_idle(&self) -> bool {
        self.worker.is_idle()
    }

    pub fn edit_published(&mut self, frame: Option<AnimationFrame>) {
        self.shown = frame.as_ref().map(|frame| Sample {
            clip: frame.clip,
            time: frame.time,
            preview: None,
        });
        self.controls.clip = self.shown.map(|sample| sample.clip);
        self.controls.time = self.shown.map_or(0., |sample| sample.time);
        self.rest = frame.map(|frame| frame.rest);
    }

    pub fn reference_snapshot<'a>(&'a self, asset: &'a PreparedAsset) -> &'a GeometrySnapshot {
        self.rest.as_deref().unwrap_or_else(|| asset.snapshot())
    }

    pub fn stop_after_error(&mut self) {
        self.preview(None);
        self.controls.clip = self.shown.map(|sample| sample.clip);
        self.controls.time = self.shown.map_or(0., |sample| sample.time);
        self.controls.playing = false;
        self.anchor = None;
    }

    pub fn tick(&mut self, asset: &mut PreparedAsset, renderer: &mut Renderer) -> GpuResult<bool> {
        let Some(clip) = self.controls.clip else {
            if self.shown.is_some()
                && let Some(rest) = &self.rest
            {
                renderer.set_scene_pose(rest)?;
                asset.generation.snapshot = rest.clone();
                self.shown = None;
                self.rest = None;
                return Ok(true);
            }
            return Ok(false);
        };
        let now = Instant::now();
        let duration = asset.document().animation_clips()[clip].duration;
        if let Some((start, offset)) = self.anchor {
            let time = f64::from(offset)
                + now.duration_since(start).as_secs_f64() * f64::from(self.controls.speed);
            if self.controls.looping && duration > 0. {
                self.controls.time = (time % f64::from(duration)) as f32;
            } else if time >= f64::from(duration) {
                self.controls.time = duration;
                self.controls.playing = false;
                self.anchor = None;
            } else {
                self.controls.time = time as f32;
            }
        }

        let mut published = false;
        if let Some(completion) = self.worker.poll()
            && let Some(sample) = self.pending.take()
        {
            let pose = completion.result?;
            renderer.set_scene_pose(&pose)?;
            asset.generation.snapshot = Arc::new(pose);
            self.shown = Some(sample);
            published = true;
        }
        let sample = Sample {
            clip,
            time: self.controls.time,
            preview: self.preview,
        };
        if self.pending.is_none() && self.shown != Some(sample) && now >= self.next_frame {
            if self.rest.is_none() {
                self.rest = Some(asset.snapshot().clone());
            }
            self.worker.request(PoseRequest {
                session: asset.session.clone(),
                sample,
            });
            self.pending = Some(sample);
            self.next_frame = now + Duration::from_millis(33);
        }
        Ok(published)
    }
}

fn prepare(request: &Request<PoseRequest>) -> AssetResult<GeometrySnapshot> {
    request.input.session.document().view().animated_snapshot(
        request.input.sample.clip,
        request.input.sample.time,
        request.input.sample.preview,
        ImportLimits::default(),
        &request.cancel,
    )
}
