use ad_commands::Session;
use ad_document::{AssetResult, ChannelValue, GeometrySnapshot, ImportLimits, SkinLayout};
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
    /// ADR-0066: ask for matrices (GPU skinning) instead of a snapshot; the
    /// layout when one is already on the GPU.
    gpu: bool,
    layout: Option<Arc<SkinLayout>>,
}

/// A computed pose: a full CPU snapshot, or the matrices for the GPU
/// skinning pass (with the layout when it was built for this request).
pub(crate) enum Pose {
    Snapshot(Box<GeometrySnapshot>),
    Matrices {
        layout: Option<Arc<SkinLayout>>,
        matrices: Vec<[[f32; 4]; 4]>,
    },
}

pub(crate) struct Playback {
    pub controls: Controls,
    worker: Worker<PoseRequest, Pose>,
    pending: Option<(Sample, bool)>,
    shown: Option<Sample>,
    /// ADR-0066: the shown pose was skinned on the GPU (no CPU snapshot of
    /// it; one follows when playback stops).
    shown_gpu: bool,
    /// The GPU skinning layout of the current scene, and scenes (document,
    /// revision) that cannot be skinned on the GPU.
    layout: Option<Arc<SkinLayout>>,
    gpu_off: Option<(u64, u64)>,
    pub gpu_frames: u64,
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
            shown_gpu: false,
            layout: None,
            gpu_off: None,
            gpu_frames: 0,
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
        self.shown_gpu = false;
        self.layout = None;
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

    /// A preview that changes while dragging (joint gizmo, key fields): the
    /// pose being computed is finished and shown, then the newest value is
    /// computed. Cancelling on every pointer move (as `preview` does) left
    /// the pose frozen until the pointer stopped when one pose took longer
    /// than a frame.
    pub fn preview_live(&mut self, value: Option<ChannelValue>) {
        if self.preview != value {
            self.preview = value;
            self.next_frame = Instant::now();
        }
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
        // A new scene revision: its GPU layout is built again on demand.
        self.shown_gpu = false;
        self.layout = None;
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

    /// The pose on screen was skinned on the GPU (ADR-0066).
    pub fn shown_on_gpu(&self) -> Option<(usize, f32)> {
        self.shown
            .filter(|_| self.shown_gpu)
            .map(|s| (s.clip, s.time))
    }

    pub fn tick(&mut self, asset: &mut PreparedAsset, renderer: &mut Renderer) -> GpuResult<bool> {
        let tag = {
            let document = asset.document();
            (document.id().0, document.revision())
        };
        if self
            .layout
            .as_ref()
            .is_some_and(|l| (l.document_id.0, l.revision) != tag)
        {
            self.layout = None;
        }
        let Some(clip) = self.controls.clip else {
            if self.shown.is_some()
                && let Some(rest) = &self.rest
            {
                renderer.set_scene_pose(rest)?;
                asset.generation.snapshot = rest.clone();
                self.shown = None;
                self.shown_gpu = false;
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
            && let Some((sample, asked_gpu)) = self.pending.take()
        {
            match completion.result? {
                Pose::Snapshot(pose) => {
                    if asked_gpu {
                        // This scene is not skinned on the GPU (morphs).
                        self.gpu_off = Some(tag);
                    }
                    renderer.set_scene_pose(&pose)?;
                    asset.generation.snapshot = Arc::new(*pose);
                    self.shown_gpu = false;
                }
                Pose::Matrices { layout, matrices } => {
                    if let Some(layout) = layout {
                        match renderer.set_scene_skin(&layout) {
                            Ok(()) => self.layout = Some(layout),
                            Err(error) => {
                                eprintln!("gpu_skinning_off reason={error}");
                                self.gpu_off = Some(tag);
                            }
                        }
                    }
                    if self.layout.is_some() {
                        renderer.set_skin_pose(asset.document().id(), tag.1, &matrices)?;
                        self.shown_gpu = true;
                        self.gpu_frames += 1;
                    }
                }
            }
            self.shown = Some(sample);
            published = true;
        }
        let sample = Sample {
            clip,
            time: self.controls.time,
            preview: self.preview,
        };
        // ADR-0066: while playing, the GPU skins; paused, scrubbing and key
        // previews keep the exact CPU pose (picking uses it too).
        let gpu = self.controls.playing
            && self.preview.is_none()
            && renderer.gpu_skinning()
            && self.gpu_off != Some(tag);
        let stale = self.shown != Some(sample) || (self.shown_gpu && !gpu);
        if self.pending.is_none() && stale && now >= self.next_frame {
            if self.rest.is_none() {
                self.rest = Some(asset.snapshot().clone());
            }
            self.worker.request(PoseRequest {
                session: asset.session.clone(),
                sample,
                gpu,
                layout: self.layout.clone(),
            });
            self.pending = Some((sample, gpu));
            // At most one pose in flight; up to 60 per second.
            self.next_frame = now + Duration::from_millis(16);
        }
        Ok(published)
    }
}

fn prepare(request: &Request<PoseRequest>) -> AssetResult<Pose> {
    let input = &request.input;
    let view = input.session.document().view();
    let sample = input.sample;
    if input.gpu {
        let (layout, fresh) = match &input.layout {
            Some(layout) => (Some(layout.clone()), false),
            None => (
                view.skin_layout(ImportLimits::default(), &request.cancel)?
                    .map(Arc::new),
                true,
            ),
        };
        if let Some(layout) = layout {
            let matrices = view.pose_matrices(
                &layout,
                Some((sample.clip, sample.time, sample.preview)),
                &request.cancel,
            )?;
            return Ok(Pose::Matrices {
                layout: fresh.then_some(layout),
                matrices,
            });
        }
    }
    view.animated_snapshot(
        sample.clip,
        sample.time,
        sample.preview,
        ImportLimits::default(),
        &request.cancel,
    )
    .map(|pose| Pose::Snapshot(Box::new(pose)))
}
