// Embed app icon on Windows builds.
// Invoked automatically by Cargo via build.rs.
//
// Expects windows/app.ico to exist.  Run scripts/generate-icons.sh to
// create it from icons/logo.svg, or let CI generate it.
#[cfg(target_os = "windows")]
fn main() {
    let ico = std::path::PathBuf::from("windows/app.ico");
    if ico.exists() {
        winres::WindowsResource::new()
            .set_icon(ico.to_str().unwrap())
            .compile()
            .expect("failed to compile Windows resources");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {}
