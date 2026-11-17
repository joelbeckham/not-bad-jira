fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // GPUI's nested form rendering needs headroom on the UI thread, especially
        // in unoptimized builds. This reserves address space, not committed RAM.
        println!("cargo:rustc-link-arg-bin=not-bad-jira=/STACK:8388608");
        embed_resource::compile("assets/app.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
