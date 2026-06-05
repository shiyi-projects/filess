fn main() {
    // Ensure `python-runtime/` exists so the `resources` glob in
    // tauri.conf.json doesn't error during `cargo check` / dev builds.
    // The real content is populated by `scripts/bundle_python.ps1`
    // before `tauri build` (production).
    let runtime_dir = std::path::Path::new("python-runtime");
    if !runtime_dir.exists() {
        std::fs::create_dir_all(runtime_dir).ok();
        // Create a placeholder so the glob matches at least one file.
        std::fs::write(runtime_dir.join(".gitkeep"), b"").ok();
    }

    tauri_build::build()
}
