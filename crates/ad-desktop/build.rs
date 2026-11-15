fn main() {
    println!("cargo:rerun-if-changed=../../assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/app.ico")
            .set("ProductName", "Asset Doctor RT-PT 2.0")
            .set(
                "FileDescription",
                "Asset Doctor RT-PT 2.0 — Vulkan foundation",
            )
            .set("OriginalFilename", "AssetDoctorRTPT.exe")
            .compile()
            .expect("Windows icon/version resource compilation failed");
    }
}
