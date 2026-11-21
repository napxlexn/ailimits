// build.rs — embedding Windows resources (the icon).
fn main() {
    // Re-run (and re-embed) whenever the icon changes.
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        // The version resource in full: what Explorer, Task Manager and
        // antivirus scanners read about the file (a file that says little
        // about itself scores worse with machine-learning scanners).
        res.set("ProductName", "AI Limits")
            .set("FileDescription", "AI Limits")
            .set("CompanyName", "napxlexn")
            .set(
                "LegalCopyright",
                "Copyright (C) 2026 napxlexn. GPL-3.0-or-later.",
            )
            .set("Comments", "https://github.com/napxlexn/ailimits");
        if let Err(e) = res.compile() {
            // A missing icon must not break the build.
            println!("cargo:warning=winres failed: {e}");
        }
    }
}
