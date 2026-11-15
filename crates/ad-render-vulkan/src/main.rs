use ad_render_vulkan::context::{Context, GpuResult};

fn main() {
    if let Err(error) = run() {
        eprintln!("GPU experiment failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> GpuResult<()> {
    let require_validation = std::env::args().any(|arg| arg == "--require-validation");
    let context = Context::new(&[], require_validation)?;
    context.describe_devices()?;
    if context.validation_errors() > 0 {
        return Err("Vulkan validation errors occurred".into());
    }
    Ok(())
}
