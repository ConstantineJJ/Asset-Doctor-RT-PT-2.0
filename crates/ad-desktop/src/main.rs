#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native;

use ad_render_vulkan::{
    SurfaceSize,
    context::GpuResult,
    renderer::{FrameResult, Renderer},
};
use std::{
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Icon, Window, WindowId},
};

fn icon() -> GpuResult<Icon> {
    let decoder = png::Decoder::new(Cursor::new(include_bytes!(
        "../../../assets/window-icon.png"
    )));
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("Icon output size overflow")?
    ];
    let info = reader.next_frame(&mut bytes)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("Bundled icon must be 8-bit RGBA PNG".into());
    }
    bytes.truncate(info.buffer_size());
    Ok(Icon::from_rgba(bytes, info.width, info.height)?)
}

#[derive(Debug)]
enum SmokeStage {
    Initial,
    Resizing(u64),
    Minimizing(Instant),
    Restoring(u64),
    Closing,
}

struct App {
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    error: Option<String>,
    validation: bool,
    smoke: Option<SmokeStage>,
    started: Instant,
    resize_events: u32,
    saw_resize: bool,
    saw_minimized: bool,
    saw_restore: bool,
    minimized_frame_baseline: Option<u64>,
    close_requested: bool,
    suspended: bool,
    closed: bool,
}

impl App {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Some(renderer) = self.renderer.take() {
            let context = renderer.context();
            let frames = renderer.frames;
            drop(renderer); // Retire ALL Vulkan objects while native window is alive.
            let errors = context.validation_errors();
            eprintln!(
                "native_shutdown frames={frames} resize_events={} minimized={} restored={} validation_errors={errors}",
                self.resize_events, self.saw_minimized, self.saw_restore
            );
            if errors > 0 {
                self.error
                    .get_or_insert(format!("{errors} Vulkan validation errors"));
            }
            if self.smoke.is_some() && self.error.is_none() {
                if !self.saw_resize
                    || !self.saw_minimized
                    || !self.saw_restore
                    || !self.close_requested
                    || frames < 15
                {
                    self.error = Some(
                        "Native smoke did not observe every required lifecycle transition".into(),
                    );
                } else {
                    eprintln!(
                        "SMOKE_PASS frames={frames} resize=true minimize=true restore=true close_requested=true validation_enabled={} validation_errors=0",
                        context.validation
                    );
                }
            }
        }
        event_loop.exit();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        self.finish(event_loop);
    }

    fn create(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Asset Doctor RT-PT 2.0 | Vulkan clear-frame prototype")
                    .with_inner_size(PhysicalSize::new(1200, 760))
                    .with_min_inner_size(PhysicalSize::new(320, 240))
                    .with_window_icon(Some(icon()?)),
            )?,
        );
        let size = window.inner_size();
        let renderer = Renderer::new(
            window.clone(),
            SurfaceSize {
                width: size.width,
                height: size.height,
            },
            self.validation,
        )?;
        eprintln!(
            "native_window_ready size={}x{} icon=owner_asset",
            size.width, size.height
        );
        self.window = Some(window);
        self.renderer = Some(renderer);
        Ok(())
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        if self.closed {
            return Ok(());
        }
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        let minimized = window.is_minimized().unwrap_or(false);
        if minimized && !self.suspended {
            renderer.recreate(SurfaceSize {
                width: 0,
                height: 0,
            })?;
            self.suspended = true;
            self.saw_minimized = true;
            self.minimized_frame_baseline = Some(renderer.frames);
            eprintln!("native_minimized frames={}", renderer.frames);
        } else if !minimized && self.suspended {
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                renderer.recreate(SurfaceSize {
                    width: size.width,
                    height: size.height,
                })?;
                self.suspended = false;
                self.saw_restore = true;
                eprintln!("native_restored frames={}", renderer.frames);
            }
        }
        if let Some(stage) = self.smoke.as_mut() {
            if self.started.elapsed() > Duration::from_secs(20) {
                return Err(format!("Native smoke timed out at {stage:?}").into());
            }
            let frames = renderer.frames;
            match stage {
                SmokeStage::Initial if frames >= 5 => {
                    let _ = window.request_inner_size(PhysicalSize::new(960, 600));
                    *stage = SmokeStage::Resizing(frames);
                    eprintln!("smoke_request_resize");
                }
                SmokeStage::Resizing(before) if self.saw_resize && frames >= *before + 5 => {
                    window.set_minimized(true);
                    *stage = SmokeStage::Minimizing(Instant::now());
                    eprintln!("smoke_request_minimize");
                }
                SmokeStage::Minimizing(at)
                    if self.saw_minimized && at.elapsed() > Duration::from_millis(500) =>
                {
                    if self.minimized_frame_baseline != Some(frames) {
                        return Err("Frames were submitted while minimized".into());
                    }
                    window.set_minimized(false);
                    *stage = SmokeStage::Restoring(frames);
                    eprintln!("smoke_request_restore");
                }
                SmokeStage::Restoring(before) if self.saw_restore && frames >= *before + 5 => {
                    native::request_close(window)?;
                    *stage = SmokeStage::Closing;
                    eprintln!("smoke_post_native_close");
                }
                _ => (),
            }
        }
        if !self.suspended {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.create(event_loop)
        {
            self.fail(event_loop, error);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let result: GpuResult<()> = match event {
            WindowEvent::CloseRequested => {
                self.close_requested = true;
                eprintln!("native_close_requested");
                self.finish(event_loop);
                Ok(())
            }
            WindowEvent::Resized(size) => {
                self.resize_events += 1;
                self.saw_resize |= size == PhysicalSize::new(960, 600);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.recreate(SurfaceSize {
                        width: size.width,
                        height: size.height,
                    })
                } else {
                    Ok(())
                }
            }
            WindowEvent::RedrawRequested
                if !self.suspended
                    && self
                        .window
                        .as_ref()
                        .is_some_and(|window| window.is_minimized() != Some(true)) =>
            {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.draw().and_then(|result| {
                        if result == FrameResult::Recreate {
                            let size = self
                                .window
                                .as_ref()
                                .expect("Renderer has a live window")
                                .inner_size();
                            renderer.recreate(SurfaceSize {
                                width: size.width,
                                height: size.height,
                            })?;
                        }
                        Ok(())
                    })
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.tick(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.finish(event_loop);
    }
}

fn run() -> GpuResult<()> {
    let mut validation = false;
    let mut smoke = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--require-validation" => validation = true,
            "--smoke" => {
                smoke = true;
                validation = true;
            }
            "--help" => {
                println!("AssetDoctorRTPT [--require-validation] [--smoke]");
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let event_loop = EventLoop::new()?;
    let mut app = App {
        renderer: None,
        window: None,
        error: None,
        validation,
        smoke: smoke.then_some(SmokeStage::Initial),
        started: Instant::now(),
        resize_events: 0,
        saw_resize: false,
        saw_minimized: false,
        saw_restore: false,
        minimized_frame_baseline: None,
        close_requested: false,
        suspended: false,
        closed: false,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Asset Doctor startup/runtime error: {error}");
        if !std::env::args().any(|arg| arg == "--smoke") {
            native::show_error(&error.to_string());
        }
        std::process::exit(1);
    }
}
