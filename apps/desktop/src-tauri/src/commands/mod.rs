use std::process::Command;
use std::sync::Arc;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::models::{
    AppBootstrapState, AppSettings, BatchSummary, CpuInfo, EnqueueResult, OrganizePolicy,
    SearchResultItem, SearchResults, SettingsSnapshot,
};
use crate::services::orchestrator::AppState;
use crate::services::storage;
use serde_json::json;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn bootstrap_app_state(state: State<'_, Arc<AppState>>) -> AppBootstrapState {
    let batches = state.batches.lock().clone();
    let results = state.results.lock().clone();
    let cfg = state.config.lock().clone();

    AppBootstrapState {
        batches,
        search_results: vec![],
        settings: SettingsSnapshot {
            paths: serde_json::json!({
                "target_root": cfg.target_root,
                "unclassified_root": cfg.unclassified_root,
            }),
            classification_rules: serde_json::json!({
                "categories": cfg.categories,
            }),
            ai: serde_json::json!({
                "provider": "siliconflow",
                "chat_model": "Qwen/Qwen2.5-7B-Instruct",
            }),
            organize_policy: serde_json::json!({
                "low_confidence_threshold": cfg.low_confidence_threshold,
                "conflict_policy": "append_counter",
            }),
            data_and_logs: serde_json::json!({
                "log_level": "info",
            }),
        },
        recent_results: results
            .iter()
            .map(|r| serde_json::json!({
                "itemId": r.item_id,
                "fileName": r.file_name,
                "categoryName": r.category_name,
                "currentPath": r.current_path,
                "itemType": r.item_type,
                "processedAt": r.processed_at,
                "operationId": r.operation_id,
            }))
            .collect(),
    }
}

#[tauri::command]
pub fn get_batch_status(batch_id: String, state: State<'_, Arc<AppState>>) -> Result<BatchSummary, String> {
    state
        .batches
        .lock()
        .iter()
        .find(|b| b.batch_id == batch_id)
        .cloned()
        .ok_or_else(|| "batch_not_found".to_string())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPayload {
    pub query: String,
    pub filters: Option<serde_json::Value>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Kept for backward compatibility; runs both channels sequentially. Prefer
/// `search_by_filename` + `search_semantic` from the front-end to get
/// incremental results (filename lands in milliseconds; semantic after the
/// embedding round-trip).
#[tauri::command]
pub fn search_files(
    payload: SearchPayload,
    state: State<'_, Arc<AppState>>,
) -> Result<SearchResults, String> {
    let query = payload.query.trim().to_string();
    let limit = payload.limit.unwrap_or(20).max(1);

    if query.is_empty() {
        return Ok(SearchResults {
            semantic: Vec::new(),
            filename: Vec::new(),
        });
    }

    let filename = search_filename_impl(&state, &query, limit)?;
    let semantic = match search_semantic_impl(&state, &query, limit) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[search] semantic disabled: {e}");
            Vec::new()
        }
    };
    Ok(SearchResults { semantic, filename })
}

/// Fast path: SQL `LIKE` against file_name / category_name / current_path.
/// Returns in a few milliseconds even with tens of thousands of rows.
#[tauri::command]
pub fn search_by_filename(
    payload: SearchPayload,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SearchResultItem>, String> {
    let query = payload.query.trim().to_string();
    let limit = payload.limit.unwrap_or(20).max(1);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    search_filename_impl(&state, &query, limit)
}

/// Slow path: query embedding + cosine over in-memory vectors.
/// Network round-trip dominates; the vector comparison itself is sub-ms.
#[tauri::command]
pub fn search_semantic(
    payload: SearchPayload,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SearchResultItem>, String> {
    let query = payload.query.trim().to_string();
    let limit = payload.limit.unwrap_or(20).max(1);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    search_semantic_impl(&state, &query, limit)
}

fn search_filename_impl(
    state: &State<'_, Arc<AppState>>,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResultItem>, String> {
    let filename_records = {
        let db = state.db.lock();
        match db.as_ref() {
            Some(conn) => storage::search_by_text(conn, query, limit)?,
            None => Vec::new(),
        }
    };
    Ok(filename_records
        .iter()
        .map(|r| SearchResultItem {
            item_id: r.id.clone(),
            title: r.file_name.clone(),
            category_id: r.category_name.clone(),
            category_name: r.category_name.clone(),
            current_path: r.current_path.clone(),
            summary_excerpt: None,
            matched_fields: vec!["file_name".into()],
            last_processed_at: r.processed_at.clone(),
            score: None,
        })
        .collect())
}

fn search_semantic_impl(
    state: &State<'_, Arc<AppState>>,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResultItem>, String> {
    // Try query cache first — repeat searches are instant.
    let query_vec: Vec<f32> = if let Some(v) = state.query_cache_get(query) {
        v
    } else {
        let pool = state
            .sidecar
            .lock()
            .as_ref()
            .ok_or("sidecar pool not initialised")?
            .clone();
        let mut lease = pool.acquire();
        let resp = lease.call("embed_text", json!({ "text": query }))?;
        drop(lease);
        let arr = resp["embedding"]
            .as_array()
            .ok_or("embed_text returned no embedding array")?;
        let v: Vec<f32> = arr
            .iter()
            .filter_map(|x| x.as_f64().map(|f| f as f32))
            .collect();
        if !v.is_empty() {
            state.query_cache_put(query.to_string(), v.clone());
        }
        v
    };

    if query_vec.is_empty() {
        return Ok(Vec::new());
    }

    // Score against the in-memory cache — no DB blob reads.
    let mut scored: Vec<(String, f32)> = {
        let emb = state.embeddings_cache.lock();
        emb.iter()
            .map(|(id, vec)| (id.clone(), storage::cosine(&query_vec, vec)))
            .filter(|(_, s)| *s > 0.0)
            .collect()
    };
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    if scored.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = scored.iter().map(|(id, _)| id.clone()).collect();
    let records = {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::find_items_by_ids(conn, &ids)?
    };

    // Re-order records to match score order
    let mut by_id: std::collections::HashMap<String, _> =
        records.into_iter().map(|r| (r.id.clone(), r)).collect();
    let mut out = Vec::with_capacity(scored.len());
    for (id, score) in scored {
        if let Some(r) = by_id.remove(&id) {
            out.push(SearchResultItem {
                item_id: r.id,
                title: r.file_name,
                category_id: r.category_name.clone(),
                category_name: r.category_name,
                current_path: r.current_path,
                summary_excerpt: r
                    .feature_text
                    .as_ref()
                    .map(|t| t.chars().take(180).collect::<String>()),
                matched_fields: vec!["semantic".into()],
                last_processed_at: r.processed_at,
                score: Some(score as f64),
            });
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn list_all_items(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<serde_json::Value>, String> {
    let db = state.db.lock();
    let conn = db.as_ref().ok_or("database not initialised")?;
    let items = storage::all_items_brief(conn)?;
    Ok(items
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "itemId": r.id,
                "fileName": r.file_name,
                "categoryName": r.category_name,
                "currentPath": r.current_path,
                "itemType": r.item_type,
                "processedAt": r.processed_at,
                "operationId": r.operation_id,
            })
        })
        .collect())
}

#[tauri::command]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> SettingsSnapshot {
    let cfg = state.config.lock().clone();
    SettingsSnapshot {
        paths: serde_json::json!({
            "target_root": cfg.target_root,
            "unclassified_root": cfg.unclassified_root,
        }),
        classification_rules: serde_json::json!({
            "categories": cfg.categories,
        }),
        ai: serde_json::json!({
            "provider": "siliconflow",
            "chat_model": "Qwen/Qwen2.5-7B-Instruct",
        }),
        organize_policy: serde_json::json!({
            "low_confidence_threshold": cfg.low_confidence_threshold,
        }),
        data_and_logs: serde_json::json!({
            "log_level": "info",
        }),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueuePayload {
    pub paths: Vec<String>,
}

#[tauri::command]
pub fn enqueue_items(
    payload: EnqueuePayload,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<EnqueueResult, String> {
    // The full dedup + hash + sidecar-startup + worker-spawn sequence can
    // take seconds (hashing folders, launching 3 Python processes first time,
    // SQL lookups per file). Running it synchronously would freeze the UI
    // until it finishes, which is exactly what caused "drop 2 GB of video
    // → app unresponsive" reports.
    //
    // Solution: fire-and-forget. The command returns instantly; the real
    // work runs on a background thread and emits `batch-updated` /
    // `duplicates-bumped` events so the front-end reacts asynchronously.
    let state_arc = Arc::clone(state.inner());
    let app_clone = app.clone();
    let paths = payload.paths;
    std::thread::spawn(move || {
        if let Err(e) = state_arc.enqueue_async(paths, app_clone, None, false) {
            eprintln!("[enqueue] background enqueue failed: {e}");
        }
    });
    Ok(EnqueueResult {
        batch_id: None,
        queued_count: 0,
        skipped: Vec::new(),
    })
}

#[tauri::command]
pub fn get_organize_policy(state: State<'_, Arc<AppState>>) -> OrganizePolicy {
    let cfg = state.config.lock();
    OrganizePolicy {
        file_operation_mode: cfg.file_operation_mode,
        source_disposition: cfg.source_disposition,
        auto_unclassify_low_confidence: cfg.auto_unclassify_low_confidence,
    }
}

#[tauri::command]
pub fn update_settings(
    patch: OrganizePolicy,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Persist to DB first; only mutate in-memory config if the write succeeded.
    {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::save_setting(
            conn,
            "file_operation_mode",
            &serde_json::to_string(&patch.file_operation_mode).map_err(|e| e.to_string())?,
        )?;
        storage::save_setting(
            conn,
            "source_disposition",
            &serde_json::to_string(&patch.source_disposition).map_err(|e| e.to_string())?,
        )?;
        storage::save_setting(
            conn,
            "auto_unclassify_low_confidence",
            &serde_json::to_string(&patch.auto_unclassify_low_confidence).map_err(|e| e.to_string())?,
        )?;
    }
    {
        let mut cfg = state.config.lock();
        cfg.file_operation_mode = patch.file_operation_mode;
        cfg.source_disposition = patch.source_disposition;
        cfg.auto_unclassify_low_confidence = patch.auto_unclassify_low_confidence;
    }
    let _ = app.emit("settings-updated", &patch);
    Ok(())
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, Arc<AppState>>) -> AppSettings {
    let cfg = state.config.lock();
    AppSettings {
        api_key: cfg.api_key.clone(),
        chat_model: cfg.chat_model.clone(),
        embedding_model: cfg.embedding_model.clone(),
        target_root: cfg.target_root.clone(),
        unclassified_root: cfg.unclassified_root.clone(),
        low_confidence_threshold: cfg.low_confidence_threshold,
        categories: cfg.categories.clone(),
        file_operation_mode: cfg.file_operation_mode,
        source_disposition: cfg.source_disposition,
        auto_unclassify_low_confidence: cfg.auto_unclassify_low_confidence,
        max_concurrent_workers: cfg.max_concurrent_workers,
        search_hotkey: cfg.search_hotkey.clone(),
        max_top_level_categories: cfg.max_top_level_categories,
    }
}

#[tauri::command]
pub fn get_cpu_info() -> CpuInfo {
    use crate::services::orchestrator::{logical_cpu_count, recommended_workers};
    CpuInfo {
        logical: logical_cpu_count(),
        recommended: recommended_workers(),
    }
}

#[tauri::command]
pub fn update_app_settings(
    patch: AppSettings,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Detect changes that require sidecar workers to be reconfigured / resized.
    let (needs_reconfigure, needs_resize, old_workers) = {
        let cur = state.config.lock();
        (
            cur.api_key != patch.api_key
                || cur.chat_model != patch.chat_model
                || cur.embedding_model != patch.embedding_model,
            cur.max_concurrent_workers != patch.max_concurrent_workers,
            cur.max_concurrent_workers,
        )
    };
    let _ = old_workers;

    // Persist every key — failure on any aborts the apply.
    {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        let to_save: [(&str, String); 13] = [
            ("api_key", serde_json::to_string(&patch.api_key).map_err(|e| e.to_string())?),
            ("chat_model", serde_json::to_string(&patch.chat_model).map_err(|e| e.to_string())?),
            ("embedding_model", serde_json::to_string(&patch.embedding_model).map_err(|e| e.to_string())?),
            ("target_root", serde_json::to_string(&patch.target_root).map_err(|e| e.to_string())?),
            ("unclassified_root", serde_json::to_string(&patch.unclassified_root).map_err(|e| e.to_string())?),
            ("low_confidence_threshold", serde_json::to_string(&patch.low_confidence_threshold).map_err(|e| e.to_string())?),
            ("categories", serde_json::to_string(&patch.categories).map_err(|e| e.to_string())?),
            ("file_operation_mode", serde_json::to_string(&patch.file_operation_mode).map_err(|e| e.to_string())?),
            ("source_disposition", serde_json::to_string(&patch.source_disposition).map_err(|e| e.to_string())?),
            ("auto_unclassify_low_confidence", serde_json::to_string(&patch.auto_unclassify_low_confidence).map_err(|e| e.to_string())?),
            ("max_concurrent_workers", serde_json::to_string(&patch.max_concurrent_workers).map_err(|e| e.to_string())?),
            ("search_hotkey", serde_json::to_string(&patch.search_hotkey).map_err(|e| e.to_string())?),
            ("max_top_level_categories", serde_json::to_string(&patch.max_top_level_categories).map_err(|e| e.to_string())?),
        ];
        for (k, v) in &to_save {
            storage::save_setting(conn, k, v)?;
        }
    }

    // Apply to in-memory config.
    {
        let mut cur = state.config.lock();
        cur.api_key = patch.api_key.clone();
        cur.chat_model = patch.chat_model.clone();
        cur.embedding_model = patch.embedding_model.clone();
        cur.target_root = patch.target_root.clone();
        cur.unclassified_root = patch.unclassified_root.clone();
        cur.low_confidence_threshold = patch.low_confidence_threshold.clamp(0.0, 1.0);
        if !patch.categories.is_empty() {
            cur.categories = patch.categories.clone();
        }
        cur.file_operation_mode = patch.file_operation_mode;
        cur.source_disposition = patch.source_disposition;
        cur.auto_unclassify_low_confidence = patch.auto_unclassify_low_confidence;
        cur.max_concurrent_workers = patch.max_concurrent_workers.clamp(
            1,
            crate::services::orchestrator::logical_cpu_count(),
        );
        if !patch.search_hotkey.trim().is_empty() {
            cur.search_hotkey = patch.search_hotkey.clone();
        }
        cur.max_top_level_categories = patch.max_top_level_categories.clamp(5, 100);
    }

    // Reconfigure sidecar workers if model/api changed AND a pool exists.
    let pool_opt = state.sidecar.lock().clone();
    if let Some(ref pool) = pool_opt {
        if needs_reconfigure {
            if let Err(e) = pool.reconfigure(
                &patch.api_key,
                &patch.chat_model,
                &patch.embedding_model,
            ) {
                eprintln!("[settings] sidecar reconfigure failed: {e}");
                return Err(format!("应用设置成功,但 sidecar 重新配置失败: {e}"));
            }
        }
        if needs_resize {
            if let Err(e) = pool.resize(patch.max_concurrent_workers) {
                eprintln!("[settings] sidecar resize failed: {e}");
                return Err(format!("应用设置成功,但 worker 数调整失败: {e}"));
            }
        }
    }

    let _ = app.emit("settings-updated", &patch);
    Ok(())
}

/// Undo a previously-organized item. Looks up the item by id, reads the
/// original `source_path` from the items row, and moves the file at
/// `current_path` back to where it came from.
///
/// We deliberately do NOT use a separate operations table — every items row
/// already carries both the original and current path, so the items table
/// alone is the source of truth.
/// Re-run the AI pipeline on a file we've already classified — used when the
/// user thinks the model picked the wrong category. The current location is
/// used as the new source: we drop the existing item record (so the hash
/// dedupe doesn't bounce us back to the same row), then re-enqueue the file.
/// The next move will land it wherever the LLM picks this time around.
#[tauri::command]
pub fn reclassify_item(
    item_id: String,
    hint: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<EnqueueResult, String> {
    let item = {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::find_item_by_id(conn, &item_id)?
            .ok_or_else(|| format!("item_not_found: {}", item_id))?
    };

    if !Path::new(&item.current_path).exists() {
        return Err(format!(
            "文件已不在记录位置: {} (可能已被手动移走或删除)",
            item.current_path
        ));
    }

    // Drop the existing record + embedding so:
    //   1. enqueue_async's hash dedupe pass won't recognise the file as
    //      "already known" and skip the work.
    //   2. Sidebar / Result list won't show two rows for the same file
    //      while the new pipeline run is in flight.
    {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::delete_item(conn, &item.id)?;
    }
    state.remove_embedding(&item.id);
    {
        let mut cache = state.results.lock();
        cache.retain(|r| r.item_id != item.id);
    }

    // Re-enqueue using the file's current location as the new source.
    // The optional hint reaches the classifier as a high-priority directive.
    let path = item.current_path.clone();
    let hint_clean = hint.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    // Reclassify deliberately feeds current_path (inside target_root) back
    // into the pipeline, so we MUST skip the path-prefix filter here or it
    // would reject this path the way it rejects normal already-organised drops.
    //
    // Fire-and-forget (same reasoning as enqueue_items): hashing a 2 GB video
    // blocks the invoke thread — keep the UI responsive by doing the heavy
    // lifting on a background thread.
    //
    // We deleted the old record above (required so the hash-dedupe pass
    // doesn't skip the file). If the synchronous part of enqueue_async fails
    // — e.g. the sidecar can't start — the file would otherwise be left
    // sitting inside target_root with NO record and NO way back into the UI
    // (the path-prefix filter rejects re-dropping it). So we keep the old
    // record around and restore it on failure.
    let restore = item.clone();
    let state_arc = Arc::clone(state.inner());
    let app_clone = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = state_arc.clone().enqueue_async(vec![path], app_clone.clone(), hint_clean, true) {
            eprintln!("[reclassify] background enqueue failed: {e} — restoring previous record");
            state_arc.restore_item(restore);
            let _ = app_clone.emit("batch-updated", "reclassify-failed");
        }
    });
    Ok(EnqueueResult {
        batch_id: None,
        queued_count: 0,
        skipped: Vec::new(),
    })
}

/// Outcome of an undo request — lets the UI choose between "silent refresh"
/// and "show a notice that the record was just orphaned".
#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum UndoOutcome {
    /// File was moved back to its original location.
    Restored,
    /// The recorded path didn't exist on disk (user had moved / deleted it).
    /// We cleaned up the stale DB row instead.
    StaleCleanup,
}

#[tauri::command]
pub fn undo_operation(
    item_id: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UndoOutcome, String> {
    let item = {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::find_item_by_id(conn, &item_id)?
            .ok_or_else(|| format!("item_not_found: {}", item_id))?
    };

    let source_path = item
        .source_path
        .clone()
        .ok_or_else(|| "未记录原始路径,无法撤销".to_string())?;
    let current = Path::new(&item.current_path);

    // If the file/folder is already gone (user moved it manually, deleted it,
    // cleaned up the tree themselves...) the DB record is just stale. Don't
    // error — remove the orphan record so the UI reflects reality.
    if !current.exists() {
        eprintln!(
            "[undo] {} already gone — removing stale record {}",
            item.current_path, item.id
        );
        {
            let db = state.db.lock();
            let conn = db.as_ref().ok_or("database not initialised")?;
            storage::delete_item(conn, &item.id)?;
        }
        state.remove_embedding(&item.id);
        {
            let mut cache = state.results.lock();
            cache.retain(|r| r.item_id != item.id);
        }
        let _ = app.emit("batch-updated", "undo-stale-cleanup");
        return Ok(UndoOutcome::StaleCleanup);
    }

    // If the original location is now occupied by something else, fall back
    // to "<stem>_undo_N<ext>" in the same folder rather than overwriting.
    let dest = resolve_undo_destination(&source_path);

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all({}) failed: {e}", parent.display()))?;
    }

    // Reverse the original move. Files can use `fs::copy`; **directories
    // can't** (fs::copy returns "access denied" on folders on Windows).
    // Folders must be either renamed (atomic, same-disk) or walked with
    // `copy_dir_recursive` — same strategy the forward move uses.
    if current.is_dir() {
        // Same-disk atomic move first; cross-disk falls back to recursive copy.
        if fs::rename(current, &dest).is_err() {
            crate::services::orchestrator::copy_dir_recursive(current, &dest)
                .map_err(|e| format!("undo copy (folder) failed: {}", e))?;
            if let Err(e) = trash::delete(current) {
                eprintln!(
                    "[undo] folder recursive-copy ok but trash failed for {}: {} — old folder kept",
                    item.current_path, e
                );
            }
        }
    } else {
        fs::copy(current, &dest).map_err(|e| format!("undo copy failed: {}", e))?;
        if let Err(e) = trash::delete(current) {
            eprintln!(
                "[undo] copy ok but trash failed for {}: {} — file may remain at both locations",
                item.current_path, e
            );
        }
    }

    // Drop DB row + memory caches in lock-step.
    {
        let db = state.db.lock();
        let conn = db.as_ref().ok_or("database not initialised")?;
        storage::delete_item(conn, &item.id)?;
    }
    state.remove_embedding(&item.id);
    {
        let mut cache = state.results.lock();
        cache.retain(|r| r.item_id != item.id);
    }

    let _ = app.emit("batch-updated", "undo");
    Ok(UndoOutcome::Restored)
}

fn resolve_undo_destination(original: &str) -> std::path::PathBuf {
    let original_path = std::path::PathBuf::from(original);
    if !original_path.exists() {
        return original_path;
    }
    let parent = original_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = original_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "restored".into());
    let ext = original_path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut counter = 1;
    loop {
        let candidate = parent.join(format!("{stem}_undo_{counter}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn resolve_path(item_id: &str, state: &State<'_, Arc<AppState>>) -> Result<String, String> {
    let results = state.results.lock();
    results
        .iter()
        .find(|r| r.item_id == item_id)
        .map(|r| r.current_path.clone())
        .ok_or_else(|| "item_not_found".to_string())
}

#[tauri::command]
pub fn open_file(item_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let path = resolve_path(&item_id, &state)?;

    #[cfg(target_os = "windows")]
    {
        // explorer.exe ShellExecutes the default handler for files and opens
        // the folder for directories. CreateProcessW carries UTF-16 args, so
        // Unicode paths survive intact.
        Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open_file failed for {path}: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        // `open` is the universal "open with default app" tool on macOS.
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open_file failed for {path}: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open_file failed for {path}: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub fn reveal_in_folder(item_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let path = resolve_path(&item_id, &state)?;

    #[cfg(target_os = "windows")]
    {
        // explorer's `/select,<path>` syntax is finicky — the comma binds
        // `/select` to the path, and the path must be quoted when it contains
        // spaces. Use raw_arg to bypass Rust's default quote-the-whole-arg.
        let raw = format!("/select,\"{}\"", path);
        Command::new("explorer.exe")
            .raw_arg(&raw)
            .spawn()
            .map_err(|e| format!("reveal_in_folder failed for {path}: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        // Finder: `open -R <path>` reveals and selects the file.
        Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("reveal_in_folder failed for {path}: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Linux: most file managers don't support a "select this file" arg
        // portably — we open the parent directory and let the user spot it.
        let parent = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        Command::new("xdg-open")
            .arg(&parent)
            .spawn()
            .map_err(|e| format!("reveal_in_folder failed for {parent}: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub fn copy_path(item_id: String, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    // Writing to the clipboard is delegated to the front-end (navigator.clipboard.writeText),
    // which handles Unicode natively. The previous clip.exe pipeline forwarded
    // UTF-8 bytes into an ANSI-code-page-aware tool, producing mojibake
    // (e.g. `学习` → `瀛︿範`) on Chinese Windows systems.
    resolve_path(&item_id, &state)
}
