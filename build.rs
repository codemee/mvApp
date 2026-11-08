fn main() {
    // 嵌入 manifest（PerMonitorV2 DPI 感知）
    let _ = embed_resource::compile("app.rc", embed_resource::NONE);
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
