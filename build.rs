// build.rs — embedding Windows resources (the icon).
fn main() {
    // Re-run (and re-embed) whenever the icon changes.
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // A missing icon must not break the build.
            println!("cargo:warning=winres failed: {e}");
        }
    }
}
