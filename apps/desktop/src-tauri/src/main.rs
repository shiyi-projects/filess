#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod services;
mod state;

use services::orchestrator::{AppState, OrganizerConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

/// Resolve the project root by walking up from `CARGO_MANIFEST_DIR`
/// (`apps/desktop/src-tauri`). This works in dev mode regardless of OS.
/// At runtime in a packaged build the manifest dir constant is the build-time
/// path, which is fine for sidecar bundling discussions but not used in
/// production yet — release packaging is a separate task.
fn dev_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // apps/desktop/src-tauri  →  ../../..
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or(manifest_dir)
}

/// Try to locate a bundled Python runtime inside Tauri's resource directory.
/// Returns `Some((python_exe, sidecar_main))` when both the interpreter and
/// the sidecar entry-point exist; `None` otherwise (dev mode fallback).
fn resolve_bundled_python(app: &tauri::AppHandle) -> Option<(String, String)> {
    let resource_dir = app.path().resource_dir().ok()?;
    let runtime_dir = resource_dir.join("python-runtime");

    #[cfg(target_os = "windows")]
    let python_exe = runtime_dir.join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let python_exe = runtime_dir.join("bin").join("python3");

    let sidecar_main = runtime_dir.join("sidecar").join("main.py");

    if python_exe.exists() && sidecar_main.exists() {
        // macOS: ensure the bundled Python binary has execute permission.
        // Tauri resource extraction may strip the +x bit.
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&python_exe) {
                let mode = meta.permissions().mode();
                if mode & 0o111 == 0 {
                    println!("[python] fixing execute permission on {}", python_exe.display());
                    let mut perms = meta.permissions();
                    perms.set_mode(mode | 0o755);
                    let _ = std::fs::set_permissions(&python_exe, perms);
                }
            }
        }

        println!("[python] bundled runtime found: {}", python_exe.display());
        Some((
            python_exe.to_string_lossy().to_string(),
            sidecar_main.to_string_lossy().to_string(),
        ))
    } else {
        println!("[python] no bundled runtime — using system Python");
        println!("[python]   tried: {}", python_exe.display());
        println!("[python]   tried: {}", sidecar_main.display());
        None
    }
}

/// DB path priority:
///   1. $FILESS_DB env var (manual override)
///   2. <workspace_root>/data/db.sqlite in dev (easy to inspect with any tool)
///   3. Tauri's per-OS `app_data_dir` if the workspace path doesn't exist
///      (covers packaged builds; also Mac/Linux when source isn't writable)
fn resolve_db_path(workspace_root: &Path, app_data_fallback: Option<PathBuf>) -> PathBuf {
    if let Ok(s) = std::env::var("FILESS_DB") {
        let p = PathBuf::from(s);
        println!("[storage] using $FILESS_DB override: {}", p.display());
        return p;
    }
    let dev_path = workspace_root.join("data").join("db.sqlite");
    if let Some(parent) = dev_path.parent() {
        if parent.exists() || std::fs::create_dir_all(parent).is_ok() {
            println!("[storage] dev path resolved to: {}", dev_path.display());
            return dev_path;
        }
    }
    if let Some(fallback) = app_data_fallback {
        let p = fallback.join("db.sqlite");
        println!("[storage] using app_data_dir: {}", p.display());
        return p;
    }
    println!("[storage] last-resort cwd path: {}", dev_path.display());
    dev_path
}

fn main() {
    // Resolve workspace root from compile-time manifest dir — works on every OS
    // without hard-coding any drive letters.
    let workspace_root = dev_workspace_root();

    let sidecar_path = workspace_root
        .join("services")
        .join("sidecar")
        .join("src")
        .join("sidecar")
        .join("main.py");

    // API key is intentionally NOT hard-coded here. It starts empty and is
    // loaded at runtime from the persisted `app_settings` table (see
    // `apply_persisted_settings`) or set by the user in the settings UI.
    let config = OrganizerConfig {
        sidecar_entrypoint: sidecar_path.to_string_lossy().to_string(),
        ..OrganizerConfig::default()
    };

    // We need the Tauri AppHandle to resolve app_data_dir, so we defer DB path
    // selection into `setup` below. Stash workspace_root for the closure.
    let workspace_root_cloned = workspace_root.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(AppState::new(config)))
        .setup(move |app| {
            let state = app.state::<Arc<AppState>>();
            let app_data_fallback = app.path().app_data_dir().ok();
            let db_path = resolve_db_path(&workspace_root_cloned, app_data_fallback);
            println!("[storage] db_path = {}", db_path.display());

            match state.set_database(&db_path, 200) {
                Ok(()) => {
                    println!("[storage] initialised at {}", db_path.display());
                    if let Err(e) = state.apply_persisted_settings() {
                        eprintln!("[storage] failed to load app_settings: {}", e);
                    }
                }
                Err(e) => eprintln!(
                    "[storage] init FAILED at {}: {} — items will NOT persist",
                    db_path.display(),
                    e
                ),
            }

            // ── Resolve Python runtime ─────────────────────────────
            // Prefer bundled python-runtime/ inside the resource dir
            // (production builds). Falls back to the dev workspace
            // paths already set in OrganizerConfig.
            if let Some((py, sidecar)) = resolve_bundled_python(app.handle()) {
                let mut cfg = state.config.lock();
                cfg.python_exe = py;
                cfg.sidecar_entrypoint = sidecar;
            }

            if let Some(window) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::DragDrop(drag) = event {
                        match drag {
                            DragDropEvent::Enter { paths, position } => {
                                println!(
                                    "[drag-drop] kind=enter count={} pos=({},{}) first={:?}",
                                    paths.len(),
                                    position.x,
                                    position.y,
                                    paths.first()
                                );
                            }
                            DragDropEvent::Over { position } => {
                                // Throttle: skip over-logs to avoid noise
                                let _ = position;
                            }
                            DragDropEvent::Drop { paths, position } => {
                                println!(
                                    "[drag-drop] kind=drop count={} pos=({},{}) first={:?}",
                                    paths.len(),
                                    position.x,
                                    position.y,
                                    paths.first()
                                );
                                // Secondary channel: re-emit for front-end as a belt-and-suspenders path
                                let payload: Vec<String> = paths
                                    .iter()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .collect();
                                if let Err(e) = handle.emit("drag-drop-fallback", payload) {
                                    eprintln!("[drag-drop] fallback emit failed: {}", e);
                                }
                            }
                            DragDropEvent::Leave => {
                                println!("[drag-drop] kind=leave");
                            }
                            _ => {}
                        }
                    }
                });
            } else {
                eprintln!("[drag-drop] warning: main window not found at setup time");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap_app_state,
            commands::get_batch_status,
            commands::search_files,
            commands::search_by_filename,
            commands::search_semantic,
            commands::list_all_items,
            commands::get_settings,
            commands::enqueue_items,
            commands::get_organize_policy,
            commands::update_settings,
            commands::get_app_settings,
            commands::update_app_settings,
            commands::get_cpu_info,
            commands::undo_operation,
            commands::reclassify_item,
            commands::open_file,
            commands::reveal_in_folder,
            commands::copy_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
