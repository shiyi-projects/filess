//! Orchestrates the file organization pipeline:
//! enqueue → parse → features → classify → (review?) → execute → done

use crate::models::{BatchSummary, EnqueueResult, SkippedItem, TaskSummary};
use crate::services::sidecar::{SidecarLease, SidecarPool};
use crate::services::storage::{self, ItemRecord};
use parking_lot::Mutex;
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::time::UNIX_EPOCH;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// Number of recent query embeddings to cache in memory.
const QUERY_CACHE_CAPACITY: usize = 64;

/// Logical CPU count, capped at 16 so the slider never offers absurd values
/// (a 64-core workstation doesn't translate to 64x more LLM throughput).
pub fn logical_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16)
}

/// Recommended default — small enough for free-tier API rate limits but
/// large enough to overlap network latency on a multi-core machine.
pub fn recommended_workers() -> usize {
    logical_cpu_count().min(3).max(1)
}

/// Snapshot of the current category tree, built once per batch. Shared by
/// all workers in that batch so concurrent tasks see a consistent picture;
/// the next batch will get a fresh snapshot that reflects items inserted
/// by the previous batch (via the `category_paths` trigger).
#[derive(Debug, Clone)]
pub struct CategoryIndex {
    pub top_level: Vec<String>,
    pub tree_text: String,
}

impl CategoryIndex {
    /// Build an index from a flat list of full category paths (e.g.
    /// `["工作/项目A/资料", "学习/4级"]`). Paths are split by `/`, top-level
    /// names are de-duplicated, and the whole thing is rendered into a
    /// tree-shaped string capped at `max_nodes` entries to avoid blowing
    /// up the LLM prompt.
    pub fn from_paths(paths: Vec<String>, max_nodes: usize) -> Self {
        let mut root = CategoryNode::default();
        let mut seen_top: std::collections::BTreeSet<String> = Default::default();

        for path in &paths {
            let mut node = &mut root;
            let mut first = true;
            for seg in path.split('/').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                if first {
                    seen_top.insert(seg.to_string());
                    first = false;
                }
                node = node.children.entry(seg.to_string()).or_default();
            }
        }

        let mut text = String::new();
        let mut rendered = 0usize;
        let mut truncated = 0usize;
        render_tree(&root, 0, max_nodes, &mut rendered, &mut truncated, &mut text);
        if truncated > 0 {
            text.push_str(&format!("- (更多 {} 项...)\n", truncated));
        }

        let top_level: Vec<String> = seen_top.into_iter().collect();
        Self { top_level, tree_text: text }
    }

    pub fn top_level_count(&self) -> usize {
        self.top_level.len()
    }
}

fn render_tree(
    node: &impl TreeNodeView,
    depth: usize,
    cap: usize,
    rendered: &mut usize,
    truncated: &mut usize,
    out: &mut String,
) {
    for (name, child) in node.children_sorted() {
        if *rendered >= cap {
            *truncated += 1 + child.descendant_count();
            continue;
        }
        *rendered += 1;
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str("- ");
        out.push_str(&name);
        out.push('\n');
        render_tree(child, depth + 1, cap, rendered, truncated, out);
    }
}

// Lightweight trait so we can pass the private Node type around.
trait TreeNodeView {
    fn children_sorted(&self) -> Vec<(String, &Self)>;
    fn descendant_count(&self) -> usize;
}

// Implement for the local Node type inside `from_paths`. Because that Node
// is defined inside the method, we re-declare a parallel impl helper here.
// To keep things simple we inline the impl on a public-ish struct instead:

/// Public node type used by the CategoryIndex builder. Lives here (not in
/// the method) so `TreeNodeView` can be implemented for it.
#[derive(Default)]
pub struct CategoryNode {
    children: std::collections::BTreeMap<String, CategoryNode>,
}

impl TreeNodeView for CategoryNode {
    fn children_sorted(&self) -> Vec<(String, &Self)> {
        self.children.iter().map(|(k, v)| (k.clone(), v)).collect()
    }
    fn descendant_count(&self) -> usize {
        1 + self
            .children
            .values()
            .map(|c| c.descendant_count())
            .sum::<usize>()
    }
}

/// Sanitise a `category_id` that came back from the LLM. We reject anything
/// that looks like a path-traversal attack, absolute path, or dangerous
/// Windows char. Returns the cleaned path, or `None` if the LLM's answer
/// is unusable — caller should fall back to "unclassified".
pub fn sanitize_category_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "unclassified" {
        return Some(trimmed.to_string());
    }
    // Reject absolute / drive-letter / leading-slash
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.chars().nth(1) == Some(':'))
    {
        return None;
    }
    let segments: Vec<&str> = trimmed
        .split('/')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() || segments.len() > 4 {
        return None;
    }
    const BAD_CHARS: &[char] = &['\\', '*', '?', '<', '>', '|', ':', '\n', '\r', '\t'];
    for seg in &segments {
        if seg.chars().count() > 24 || seg.chars().count() == 0 {
            return None;
        }
        if *seg == "." || *seg == ".." || seg.contains("..") {
            return None;
        }
        if seg.chars().any(|c| BAD_CHARS.contains(&c)) {
            return None;
        }
    }
    Some(segments.join("/"))
}

/// Shared application state accessible from Tauri commands.
pub struct AppState {
    /// Pool of Python sidecar workers — N parallel processes for concurrent
    /// LLM/embedding calls. `None` until `ensure_sidecar` first succeeds.
    pub sidecar: Mutex<Option<SidecarPool>>,
    pub batches: Mutex<Vec<BatchSummary>>,
    pub results: Mutex<Vec<CompletedResult>>,
    pub config: Mutex<OrganizerConfig>,
    /// Persistent SQLite store. `None` until `set_database` is called from
    /// `tauri::Builder::setup`, because the app-data-dir is only knowable
    /// through the Tauri `AppHandle`.
    pub db: Mutex<Option<Connection>>,
    /// In-memory copy of all `(item_id, embedding)` pairs, loaded once on
    /// startup and kept in sync as new items are classified. Avoids
    /// re-reading and re-decoding multi-MB of blob data on every search.
    pub embeddings_cache: Mutex<Vec<(String, Vec<f32>)>>,
    /// Small LRU of recent query embeddings. Keyed by the raw query string.
    /// First tuple element is insertion order for eviction.
    pub query_cache: Mutex<(VecDeque<String>, std::collections::HashMap<String, Vec<f32>>)>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedResult {
    pub item_id: String,
    pub file_name: String,
    pub category_name: String,
    pub current_path: String,
    pub item_type: String,
    pub processed_at: String,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperationMode {
    Move,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDisposition {
    RecycleBin,
    Delete,
}

#[derive(Debug, Clone)]
pub struct OrganizerConfig {
    pub api_key: String,
    pub target_root: String,
    pub unclassified_root: String,
    pub categories: Vec<String>,
    pub low_confidence_threshold: f64,
    pub python_exe: String,
    pub sidecar_entrypoint: String,
    pub file_operation_mode: FileOperationMode,
    pub source_disposition: SourceDisposition,
    pub auto_unclassify_low_confidence: bool,
    pub chat_model: String,
    pub embedding_model: String,
    pub max_concurrent_workers: usize,
    /// Keyboard shortcut for opening the quick search palette.
    /// Stored as a string like "Ctrl+K", "Alt+Shift+F", parsed by the
    /// front-end against `KeyboardEvent`.
    pub search_hotkey: String,
    /// Hard cap on top-level category count. While the count is below this
    /// value the LLM is allowed to invent new top-levels; once reached it's
    /// forced to pick from existing ones (or unclassified).
    pub max_top_level_categories: usize,
}

/// Resolve a sensible "Filess" library directory for the current OS.
/// - macOS  → ~/Documents/Filess
/// - Linux  → ~/Documents/Filess (or $XDG_DOCUMENTS_DIR)
/// - Windows → C:\Users\<user>\Documents\Filess
fn default_target_root() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Filess")
}

/// Pick a sensible Python invocation per platform. On Windows the canonical
/// command is `python` (or `py`); on macOS / Linux it's `python3`.
fn default_python_exe() -> &'static str {
    #[cfg(target_os = "windows")]
    { "python" }
    #[cfg(not(target_os = "windows"))]
    { "python3" }
}

impl Default for OrganizerConfig {
    fn default() -> Self {
        let target = default_target_root();
        let unclassified = target.join("未分类");
        Self {
            api_key: String::new(),
            target_root: target.to_string_lossy().into_owned(),
            unclassified_root: unclassified.to_string_lossy().into_owned(),
            categories: vec![
                "财务".into(), "工作".into(), "生活".into(),
                "学习".into(), "媒体".into(), "开发".into(),
            ],
            low_confidence_threshold: 0.8,
            python_exe: default_python_exe().to_string(),
            // Filled in by main.rs at startup with an absolute path.
            sidecar_entrypoint: "services/sidecar/src/sidecar/main.py".to_string(),
            file_operation_mode: FileOperationMode::Move,
            source_disposition: SourceDisposition::RecycleBin,
            auto_unclassify_low_confidence: true,
            chat_model: "Qwen/Qwen2.5-7B-Instruct".to_string(),
            embedding_model: "BAAI/bge-m3".to_string(),
            max_concurrent_workers: recommended_workers(),
            search_hotkey: "Ctrl+K".to_string(),
            max_top_level_categories: 30,
        }
    }
}

impl AppState {
    pub fn new(config: OrganizerConfig) -> Self {
        Self {
            sidecar: Mutex::new(None),
            batches: Mutex::new(Vec::new()),
            results: Mutex::new(Vec::new()),
            config: Mutex::new(config),
            db: Mutex::new(None),
            embeddings_cache: Mutex::new(Vec::new()),
            query_cache: Mutex::new((
                VecDeque::with_capacity(QUERY_CACHE_CAPACITY),
                std::collections::HashMap::with_capacity(QUERY_CACHE_CAPACITY),
            )),
        }
    }

    /// Initialise the SQLite store at the given path and hydrate:
    /// - the in-memory `results` cache (recent N items for UI)
    /// - the `embeddings_cache` (all vectors, for fast semantic search)
    pub fn set_database(&self, path: &Path, recent_limit: usize) -> Result<(), String> {
        let conn = storage::open(path)?;
        let recent = storage::recent_items(&conn, recent_limit)?;
        {
            let mut cache = self.results.lock();
            cache.clear();
            for rec in &recent {
                cache.push(CompletedResult::from(rec));
            }
        }
        let all_embeds = storage::all_embeddings(&conn)?;
        {
            let mut emb = self.embeddings_cache.lock();
            *emb = all_embeds;
        }
        *self.db.lock() = Some(conn);
        Ok(())
    }

    /// Push a newly-classified item's embedding into the in-memory cache so
    /// subsequent searches see it without re-scanning the DB. Replaces any
    /// existing entry for the same id — re-processing the same path (upsert)
    /// would otherwise leave stale duplicate vectors that inflate scores and
    /// grow the cache unboundedly.
    pub fn cache_embedding(&self, item_id: String, embedding: Vec<f32>) {
        let mut emb = self.embeddings_cache.lock();
        emb.retain(|(id, _)| id != &item_id);
        emb.push((item_id, embedding));
    }

    /// Drop a single embedding from the in-memory cache (used by undo).
    pub fn remove_embedding(&self, item_id: &str) {
        let mut emb = self.embeddings_cache.lock();
        emb.retain(|(id, _)| id != item_id);
    }

    /// Re-insert a previously-removed item record and rehydrate its in-memory
    /// caches. Used to roll back `reclassify_item` when the background
    /// re-enqueue fails, so the file never ends up orphaned (sitting inside
    /// target_root with no DB row and no UI entry).
    pub fn restore_item(&self, record: ItemRecord) {
        if let Some(conn) = self.db.lock().as_ref() {
            if let Err(e) = storage::upsert_item(conn, &record) {
                eprintln!("[orchestrator] restore_item DB upsert failed: {e}");
            }
        }
        if let Some(vec) = &record.embedding {
            self.cache_embedding(record.id.clone(), vec.clone());
        }
        let result = CompletedResult::from(&record);
        let mut cache = self.results.lock();
        cache.retain(|r| r.item_id != result.item_id);
        cache.insert(0, result);
    }

    /// Apply persisted settings on top of the default config. Called once at
    /// startup after `set_database`. Unknown keys are ignored so newer DBs
    /// stay forward-compatible with older binaries.
    pub fn apply_persisted_settings(&self) -> Result<(), String> {
        let stored = {
            let db = self.db.lock();
            match db.as_ref() {
                Some(c) => storage::load_settings(c)?,
                None => return Ok(()),
            }
        };
        let mut cfg = self.config.lock();
        // Existing toggles
        if let Some(v) = stored.get("file_operation_mode") {
            if let Ok(m) = serde_json::from_str::<FileOperationMode>(v) {
                cfg.file_operation_mode = m;
            }
        }
        if let Some(v) = stored.get("source_disposition") {
            if let Ok(d) = serde_json::from_str::<SourceDisposition>(v) {
                cfg.source_disposition = d;
            }
        }
        if let Some(v) = stored.get("auto_unclassify_low_confidence") {
            if let Ok(b) = serde_json::from_str::<bool>(v) {
                cfg.auto_unclassify_low_confidence = b;
            }
        }
        // Extended app settings
        if let Some(v) = stored.get("api_key") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.api_key = s;
                }
            }
        }
        if let Some(v) = stored.get("chat_model") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.chat_model = s;
                }
            }
        }
        if let Some(v) = stored.get("embedding_model") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.embedding_model = s;
                }
            }
        }
        if let Some(v) = stored.get("target_root") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.target_root = s;
                }
            }
        }
        if let Some(v) = stored.get("unclassified_root") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.unclassified_root = s;
                }
            }
        }
        if let Some(v) = stored.get("low_confidence_threshold") {
            if let Ok(f) = serde_json::from_str::<f64>(v) {
                cfg.low_confidence_threshold = f.clamp(0.0, 1.0);
            }
        }
        if let Some(v) = stored.get("categories") {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(v) {
                if !list.is_empty() {
                    cfg.categories = list;
                }
            }
        }
        if let Some(v) = stored.get("max_concurrent_workers") {
            if let Ok(n) = serde_json::from_str::<usize>(v) {
                cfg.max_concurrent_workers = n.clamp(1, logical_cpu_count());
            }
        }
        if let Some(v) = stored.get("search_hotkey") {
            if let Ok(s) = serde_json::from_str::<String>(v) {
                if !s.trim().is_empty() {
                    cfg.search_hotkey = s;
                }
            }
        }
        if let Some(v) = stored.get("max_top_level_categories") {
            if let Ok(n) = serde_json::from_str::<usize>(v) {
                cfg.max_top_level_categories = n.clamp(5, 100);
            }
        }
        Ok(())
    }

    /// LRU-get a previously-embedded query. Returns None if not cached.
    pub fn query_cache_get(&self, q: &str) -> Option<Vec<f32>> {
        self.query_cache.lock().1.get(q).cloned()
    }

    /// Insert a query embedding, evicting the oldest if over capacity.
    pub fn query_cache_put(&self, q: String, v: Vec<f32>) {
        let mut guard = self.query_cache.lock();
        let (order, map) = &mut *guard;
        if map.contains_key(&q) {
            return;
        }
        if order.len() >= QUERY_CACHE_CAPACITY {
            if let Some(oldest) = order.pop_front() {
                map.remove(&oldest);
            }
        }
        order.push_back(q.clone());
        map.insert(q, v);
    }

    /// Ensure a sidecar pool is running and configured. Idempotent — once
    /// the pool exists it's reused for all subsequent calls.
    fn ensure_sidecar(&self) -> Result<SidecarPool, String> {
        {
            let guard = self.sidecar.lock();
            if let Some(pool) = guard.as_ref() {
                return Ok(pool.clone());
            }
        }
        let cfg = self.config.lock().clone();
        let pool = SidecarPool::spawn(
            &cfg.python_exe,
            &cfg.sidecar_entrypoint,
            &cfg.api_key,
            &cfg.chat_model,
            &cfg.embedding_model,
            cfg.max_concurrent_workers,
        )?;
        let mut guard = self.sidecar.lock();
        // Re-check in case another thread won the race.
        if guard.is_none() {
            *guard = Some(pool.clone());
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    /// Enqueue paths for processing.
    ///
    /// **Dedupe is performed up front, before any task is created.** Files
    /// whose SHA-256 hash already exists in the DB are reported back to the
    /// caller as `skipped` and never enter the queue / progress UI.
    ///
    /// Returns immediately once the sidecar handshake succeeds and skipped
    /// items have been computed; remaining files are processed on a background
    /// thread that emits `batch-updated` events.
    pub fn enqueue_async(
        self: Arc<Self>,
        paths: Vec<String>,
        app: AppHandle,
        hint: Option<String>,
        bypass_filter: bool,
    ) -> Result<EnqueueResult, String> {
        let pool = self.ensure_sidecar()?;

        // ── Hash + dedupe pass ────────────────────────────────────────
        // For each file we hash and consult the DB. Cache hits are recorded
        // in `skipped` and never become tasks. Folders, broken paths, and
        // files where hashing failed fall through to the queue.
        let mut to_process: Vec<String> = Vec::new();
        let mut skipped: Vec<SkippedItem> = Vec::new();

        // ── Path-prefix filter (first line of defence) ────────────────
        // If the user drags a file that's already inside target_root or
        // unclassified_root, the system would re-classify it and generate
        // a duplicate items row. Reject up front unless the caller
        // explicitly bypasses (reclassify_item does this — it feeds
        // current_path back in as a new source by design).
        let filter_roots: Vec<PathBuf> = if bypass_filter {
            Vec::new()
        } else {
            let cfg_for_filter = self.config.lock();
            vec![
                PathBuf::from(&cfg_for_filter.target_root),
                PathBuf::from(&cfg_for_filter.unclassified_root),
            ]
        };

        for path in paths {
            // 0) Reject already-organized paths
            if !bypass_filter {
                let candidate = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| PathBuf::from(&path));
                let hit_root = filter_roots.iter().find(|r| {
                    let rc = std::fs::canonicalize(r)
                        .unwrap_or_else(|_| r.to_path_buf());
                    candidate.starts_with(&rc)
                });
                if let Some(r) = hit_root {
                    println!("[orchestrator] skip already-organized path: {path}");
                    skipped.push(SkippedItem {
                        source_path: path.clone(),
                        existing_path: path.clone(),
                        existing_id: String::new(),
                        category_name: format!("(已在 {})", r.display()),
                    });
                    continue;
                }
            }

            let p = Path::new(&path);
            // Files use a content SHA-256 (prefix "f:"); folders use a
            // structural fingerprint over (relative_path, size) pairs
            // (prefix "d:"). Either way we ask the DB for an existing
            // item with the same hash before paying for the AI pipeline.
            if let Some(hash) = compute_path_hash(p) {
                let cached = {
                    let db = self.db.lock();
                    db.as_ref()
                        .and_then(|c| storage::find_item_by_hash(c, &hash).ok().flatten())
                };
                if let Some(rec) = cached {
                    println!(
                        "[orchestrator] skip duplicate (hash={}…): {} → already at {}",
                        &hash[..hash.len().min(14)],
                        path,
                        rec.current_path
                    );
                    let cached_result = CompletedResult::from(&rec);
                    {
                        let mut cache = self.results.lock();
                        cache.retain(|r| r.item_id != cached_result.item_id);
                        cache.insert(0, cached_result);
                    }
                    skipped.push(SkippedItem {
                        source_path: path,
                        existing_path: rec.current_path,
                        existing_id: rec.id,
                        category_name: rec.category_name,
                    });
                    continue;
                }
            }
            to_process.push(path);
        }

        if to_process.is_empty() {
            println!(
                "[orchestrator] all {} dropped paths already known, no batch created",
                skipped.len()
            );
            // Trigger a UI refresh so the bumped duplicates show up at the top.
            let _ = app.emit("batch-updated", "duplicates-bumped");
            // The Tauri command returns instantly (fire-and-forget), so the
            // real result is delivered to the front-end via this event instead
            // of the command's return value.
            let result = EnqueueResult {
                batch_id: None,
                queued_count: 0,
                skipped,
            };
            let _ = app.emit("enqueue-result", &result);
            return Ok(result);
        }

        // ── Build batch + tasks for the items that actually need work ──
        let batch_id = format!("batch-{}", &Uuid::new_v4().to_string()[..8]);
        let cfg = self.config.lock().clone();

        let tasks: Vec<TaskSummary> = to_process
            .iter()
            .enumerate()
            .map(|(i, p)| TaskSummary {
                task_id: format!("{}-task-{}", batch_id, i),
                source_path: p.clone(),
                status: "queued".to_string(),
                error_message: None,
            })
            .collect();
        let queued_count = tasks.len();

        {
            let mut batches = self.batches.lock();
            batches.push(BatchSummary {
                batch_id: batch_id.clone(),
                status: "processing".to_string(),
                total: tasks.len(),
                completed: 0,
                failed: 0,
                awaiting_review: 0,
                tasks: tasks.clone(),
            });
        }

        let _ = app.emit("batch-updated", &batch_id);

        // Build one category-tree snapshot for this entire batch. Workers
        // spawned below all share a reference; next batch will rebuild and
        // thereby see any categories inserted by this batch (via the
        // SQLite trigger on `items`).
        let category_snapshot: Arc<CategoryIndex> = {
            let db = self.db.lock();
            let paths = db
                .as_ref()
                .and_then(|c| storage::list_category_paths(c).ok())
                .unwrap_or_default();
            let node_count = paths.len();
            let idx = CategoryIndex::from_paths(paths, 200);
            println!(
                "[category-index] built: {} top-level, {} paths in DB",
                idx.top_level_count(),
                node_count
            );
            Arc::new(idx)
        };

        // Concurrent worker pool: spawn N OS threads (= sidecar pool capacity)
        // that all pull from the same job queue. Each call to
        // `process_single_task` will acquire its own sidecar lease internally,
        // so multiple LLM/embedding calls run truly in parallel.
        let job_queue: Arc<Mutex<VecDeque<(usize, String)>>> = Arc::new(Mutex::new(
            to_process.into_iter().enumerate().collect(),
        ));
        let worker_count = pool.capacity();

        for _ in 0..worker_count {
            let self_clone = Arc::clone(&self);
            let app_thread = app.clone();
            let cfg_thread = cfg.clone();
            let batch_id_thread = batch_id.clone();
            let queue = Arc::clone(&job_queue);
            let hint_thread = hint.clone();
            let snapshot_thread = Arc::clone(&category_snapshot);
            std::thread::spawn(move || loop {
                let job = {
                    let mut q = queue.lock();
                    q.pop_front()
                };
                let (idx, source_path) = match job {
                    Some(j) => j,
                    None => break, // queue drained
                };
                let task_id = format!("{}-task-{}", batch_id_thread, idx);
                let result = self_clone.process_single_task(
                    &task_id,
                    &source_path,
                    &cfg_thread,
                    &app_thread,
                    hint_thread.as_deref(),
                    &snapshot_thread,
                );

                {
                    let mut batches = self_clone.batches.lock();
                    if let Some(batch) = batches.iter_mut().find(|b| b.batch_id == batch_id_thread)
                    {
                        if let Some(task) = batch.tasks.iter_mut().find(|t| t.task_id == task_id) {
                            match &result {
                                Ok(_) => {
                                    task.status = "completed".to_string();
                                    batch.completed += 1;
                                }
                                Err(e) => {
                                    task.status = "failed".to_string();
                                    task.error_message = Some(e.clone());
                                    batch.failed += 1;
                                    eprintln!("[orchestrator] task {} failed: {}", task_id, e);
                                }
                            }
                        }
                        if batch.completed + batch.failed == batch.total {
                            batch.status = "done".to_string();
                        }
                    }
                }
                let _ = app_thread.emit("batch-updated", &batch_id_thread);
            });
        }

        // Deliver the real enqueue outcome (batch id + skipped duplicates) to
        // the front-end via event — the command itself returned an empty
        // placeholder the moment this work was handed to a background thread.
        let result = EnqueueResult {
            batch_id: Some(batch_id),
            queued_count,
            skipped,
        };
        let _ = app.emit("enqueue-result", &result);
        Ok(result)
    }

    fn process_single_task(
        &self,
        task_id: &str,
        source_path: &str,
        cfg: &OrganizerConfig,
        app: &AppHandle,
        hint: Option<&str>,
        category_index: &CategoryIndex,
    ) -> Result<(), String> {
        // Hash here is for persistence — dedup already happened in enqueue_async.
        // Files get a content hash, folders get a structural fingerprint;
        // either way the DB row gets a stable key to dedupe against next time.
        self.set_status(task_id, "sniffing", app);
        let src_check = Path::new(source_path);
        let file_hash: Option<String> = compute_path_hash(src_check);
        if file_hash.is_none() {
            eprintln!(
                "[orchestrator] hash failed for {source_path} — future drops won't dedupe this item"
            );
        }

        // Acquire one worker from the pool — held for this task's full
        // duration. Other tasks using sibling workers run truly in parallel.
        let pool = {
            self.sidecar
                .lock()
                .as_ref()
                .ok_or("Sidecar pool not initialised")?
                .clone()
        };
        let mut lease: SidecarLease = pool.acquire();

        // 1. Parse item
        self.set_status(task_id, "parsing", app);
        let parsed = lease.call("parse_item", json!({ "source_path": source_path }))?;

        // 2. Build features
        let features = lease.call("build_features", json!({ "parsed_item": parsed }))?;

        let feature_text = features["feature_text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // 3. Classify
        self.set_status(task_id, "calling_model", app);
        // Seed the prompt with the most-used sub-categories so the LLM re-uses
        // established paths instead of inventing parallel ones.
        let known_subcategories: Vec<String> = {
            let db = self.db.lock();
            db.as_ref()
                .and_then(|c| storage::recent_subcategories(c, 40).ok())
                .unwrap_or_default()
        };
        let can_create_top_level =
            category_index.top_level_count() < cfg.max_top_level_categories;
        let classification = lease.call(
            "classify_item",
            json!({
                "feature_text": feature_text,
                "categories": cfg.categories,                  // legacy seed fallback
                "existing_top_level": category_index.top_level,
                "directory_tree": category_index.tree_text,
                "can_create_top_level": can_create_top_level,
                "max_top_level": cfg.max_top_level_categories,
                "low_confidence_threshold": cfg.low_confidence_threshold,
                "known_subcategories": known_subcategories,
                "hint": hint.unwrap_or(""),
            }),
        )?;

        // Raw output from the LLM — must be sanitised before we treat it as
        // a path component. `sanitize_category_id` rejects path traversal,
        // absolute paths, Windows-illegal chars, and caps segment length/count.
        let raw_category = classification["category_id"]
            .as_str()
            .unwrap_or("unclassified");
        let category_id = match sanitize_category_id(raw_category) {
            Some(clean) => {
                // Enforce top-level cap: if the model invented a new top-level
                // but we're at the ceiling, reject and fall back to unclassified.
                if !can_create_top_level {
                    let top = clean.split('/').next().unwrap_or("").to_string();
                    if !top.is_empty()
                        && top != "unclassified"
                        && !category_index.top_level.contains(&top)
                    {
                        eprintln!(
                            "[orchestrator] top-level cap ({}) reached, rejecting new top '{}' → unclassified",
                            cfg.max_top_level_categories, top
                        );
                        "unclassified".to_string()
                    } else {
                        clean
                    }
                } else {
                    clean
                }
            }
            None => {
                eprintln!(
                    "[orchestrator] LLM returned unsafe category_id {:?} — falling back to unclassified",
                    raw_category
                );
                "unclassified".to_string()
            }
        };

        let suggested_name = classification["suggested_name"]
            .as_str()
            .map(|s| s.to_string());

        let confidence = classification["confidence"].as_f64().unwrap_or(0.0);
        let need_review = classification["need_human_review"].as_bool().unwrap_or(true);

        // 4. If review needed and confidence too low:
        //    - auto_unclassify_low_confidence = true → fall through to unclassified
        //    - auto_unclassify_low_confidence = false → stop here, await human input
        let route_to_unclassified = need_review && confidence < cfg.low_confidence_threshold;
        if route_to_unclassified && !cfg.auto_unclassify_low_confidence {
            self.set_status(task_id, "awaiting_review", app);
            // No file operation; record stays in DB only after the user resolves the review.
            // Lease drops automatically and returns the worker to the pool.
            return Ok(());
        }

        // 5. Execute the file operation
        self.set_status(task_id, "executing", app);

        let src = Path::new(source_path);
        // Folders keep their original name regardless of what the LLM
        // suggested — a "项目X" folder shouldn't be renamed to "项目X_分析".
        // Files get the suggested rewrite as today.
        let src_original_name = src.file_name().unwrap_or_default().to_str().unwrap_or("unknown");
        let file_name: String = if src.is_dir() {
            src_original_name.to_string()
        } else {
            suggested_name.clone().unwrap_or_else(|| src_original_name.to_string())
        };

        // Decide the **effective** category. If we route to unclassified
        // (either because the LLM said "unclassified" or because confidence
        // is too low and auto_unclassify is on), the recorded category MUST
        // match the actual destination — otherwise the UI tag shows the
        // LLM's wishful suggestion while the file lives in 未分类/, which
        // is exactly what the bug report shows.
        let goes_to_unclassified = route_to_unclassified || category_id == "unclassified";
        let effective_category: String = if goes_to_unclassified {
            "未分类".to_string()
        } else {
            category_id.clone()
        };

        let target_dir = if goes_to_unclassified {
            PathBuf::from(&cfg.unclassified_root)
        } else {
            PathBuf::from(&cfg.target_root)
                .join(effective_category.replace('/', std::path::MAIN_SEPARATOR_STR))
        };

        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create directory {}: {e}", target_dir.display()))?;

        // Resolve conflicts: append _1, _2, ... if target exists
        let mut target_path = target_dir.join(&file_name);
        if target_path.exists() && target_path != src {
            let stem = target_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let ext = target_path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let mut counter = 1;
            loop {
                target_path = target_dir.join(format!("{stem}_{counter}{ext}"));
                if !target_path.exists() {
                    break;
                }
                counter += 1;
            }
        }

        // Snapshot the source kind BEFORE any filesystem mutation. After the
        // Move branch trashes/deletes the source, `src.is_file()` would return
        // false and we'd silently skip writing the undo log — which is exactly
        // why earlier records produced "operation_not_found" on undo.
        let src_was_file = src.is_file();
        let src_was_dir = src.is_dir();

        // Guard against operating on the file in place. `reclassify_item`
        // feeds `current_path` back in as the source; if the LLM returns the
        // same category + suggested name, `target_path` resolves to `src`.
        // `fs::copy(src, src)` truncates the file to zero on Unix (then the
        // Move branch would trash the now-empty file → data loss) and errors
        // on Windows. Either way there's nothing to move: the file already
        // lives at the destination, so just (re)record it.
        let same_location = target_path == src;

        // Move or copy according to current policy. We always go through
        // copy + dispose for Move (rather than fs::rename) so the source
        // disposition (recycle vs delete) is honoured uniformly across same-
        // and cross-disk targets.
        if same_location {
            // Nothing to do on disk — fall through to the DB/record update.
        } else if src_was_file {
            fs::copy(src, &target_path)
                .map_err(|e| format!("Failed to copy file: {e}"))?;
            if matches!(cfg.file_operation_mode, FileOperationMode::Move) {
                match cfg.source_disposition {
                    SourceDisposition::RecycleBin => {
                        if let Err(e) = trash::delete(src) {
                            // Soft-failure: copy succeeded; user keeps both copies.
                            eprintln!(
                                "[orchestrator] move ok but trash failed for {}: {} — source kept",
                                source_path, e
                            );
                        }
                    }
                    SourceDisposition::Delete => {
                        if let Err(e) = fs::remove_file(src) {
                            eprintln!(
                                "[orchestrator] move ok but delete failed for {}: {} — source kept",
                                source_path, e
                            );
                        }
                    }
                }
            }
        } else if src_was_dir {
            // Move/copy the entire folder tree. We always try fs::rename
            // first — it's atomic on same-disk paths and completes in
            // microseconds even for multi-GB trees. If it fails (typically
            // cross-disk: ERROR_NOT_SAME_DEVICE / EXDEV) we fall back to a
            // recursive copy + dispose the source.
            match cfg.file_operation_mode {
                FileOperationMode::Move => {
                    if fs::rename(src, &target_path).is_err() {
                        // Fallback: cross-disk or rename refused. Do the
                        // expensive thing: recursive copy, then handle source.
                        copy_dir_recursive(src, &target_path)
                            .map_err(|e| format!("Failed to copy folder: {e}"))?;
                        match cfg.source_disposition {
                            SourceDisposition::RecycleBin => {
                                if let Err(e) = trash::delete(src) {
                                    eprintln!(
                                        "[orchestrator] folder copied but trash failed for {}: {} — source kept",
                                        source_path, e
                                    );
                                }
                            }
                            SourceDisposition::Delete => {
                                if let Err(e) = fs::remove_dir_all(src) {
                                    eprintln!(
                                        "[orchestrator] folder copied but delete failed for {}: {} — source kept",
                                        source_path, e
                                    );
                                }
                            }
                        }
                    }
                }
                FileOperationMode::Copy => {
                    copy_dir_recursive(src, &target_path)
                        .map_err(|e| format!("Failed to copy folder: {e}"))?;
                }
            }
        }

        // 6. Embed the feature_text for semantic search.
        // Failures here must not abort the task — the file is already moved.
        let embedding: Option<Vec<f32>> = match lease.call("embed_text", json!({ "text": &feature_text })) {
            Ok(resp) => resp["embedding"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect()),
            Err(e) => {
                eprintln!("[orchestrator] embed_text failed for {task_id}: {e}");
                None
            }
        };
        // Release the worker back to the pool before touching the DB.
        drop(lease);

        // Record result
        let item_type = if src.is_dir() { "folder" } else { "file" };
        let now = chrono_now_iso();
        let item_id = format!("item-{}", &Uuid::new_v4().to_string()[..8]);
        let operation_id = format!("op-{}", &Uuid::new_v4().to_string()[..8]);
        let current_path = target_path.to_string_lossy().to_string();

        let result = CompletedResult {
            item_id: item_id.clone(),
            file_name: file_name.to_string(),
            category_name: effective_category.clone(),
            current_path: current_path.clone(),
            item_type: item_type.to_string(),
            processed_at: now.clone(),
            operation_id: Some(operation_id.clone()),
        };

        // Persist to SQLite (best-effort — failure logs but doesn't fail the task)
        let record = ItemRecord {
            id: item_id.clone(),
            file_name: file_name.to_string(),
            category_name: effective_category.clone(),
            current_path: current_path.clone(),
            source_path: Some(source_path.to_string()),
            item_type: item_type.to_string(),
            feature_text: Some(feature_text.clone()),
            embedding,
            confidence: Some(confidence),
            processed_at: now.clone(),
            operation_id: Some(operation_id.clone()),
            file_hash: file_hash.clone(),
        };
        // Undo relies solely on the items row (which carries both source_path
        // and current_path); there is no separate operations table. See
        // `undo_operation` in commands.
        if let Some(conn) = self.db.lock().as_ref() {
            if let Err(e) = storage::upsert_item(conn, &record) {
                eprintln!("[orchestrator] DB upsert failed for {task_id}: {e}");
            }
        }

        // Keep the embeddings cache in sync so the next search sees this item.
        if let Some(vec) = &record.embedding {
            self.cache_embedding(record.id.clone(), vec.clone());
        }

        self.results.lock().insert(0, result);

        Ok(())
    }

    fn set_status(&self, task_id: &str, status: &str, app: &AppHandle) {
        let mut batch_id_for_event: Option<String> = None;
        {
            let mut batches = self.batches.lock();
            for batch in batches.iter_mut() {
                if let Some(task) = batch.tasks.iter_mut().find(|t| t.task_id == task_id) {
                    task.status = status.to_string();
                    batch_id_for_event = Some(batch.batch_id.clone());
                    break;
                }
            }
        }
        if let Some(bid) = batch_id_for_event {
            let _ = app.emit("batch-updated", bid);
        }
    }
}

fn chrono_now_iso() -> String {
    // RFC3339 in UTC, e.g. "2026-04-23T08:30:00.123456Z".
    // The previous hand-rolled version used 365 days/year and 30 days/month,
    // which drifted ~14 days vs reality (one day per leap year since 1970)
    // and produced "X seconds ago" values in the past.
    chrono::Utc::now().to_rfc3339()
}

/// Above this size, hashing the whole file would block the UI long enough to
/// trigger "app unresponsive" prompts (a 2 GB video takes 20-30s on SSD).
/// Large files get a **sampled fingerprint** instead — size + mtime + the
/// first 64 KB + the last 64 KB. Collisions are astronomically unlikely for
/// real-world videos / archives / installers.
const HASH_FULL_MAX_BYTES: u64 = 64 * 1024 * 1024; // 64 MB
const HASH_SAMPLE_SIZE: u64 = 64 * 1024;           // 64 KB head / tail

/// SHA-256 hash of a file, hex-encoded with `f:` prefix.
///
/// - Files ≤ 64 MB: full content hash (byte-for-byte dedup).
/// - Files  > 64 MB: sampled fingerprint (size + mtime + head + tail).
///
/// Both share the same `f:` prefix so they live in the same `items.file_hash`
/// column; two files only compare "equal" when they were hashed with the
/// same strategy, which is always consistent because strategy is a pure
/// function of file size.
fn compute_file_hash(path: &Path) -> std::io::Result<String> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();

    let digest = if size <= HASH_FULL_MAX_BYTES {
        hash_full(path)?
    } else {
        hash_sampled(path, &metadata, size)?
    };

    let mut hex = String::with_capacity(2 + 64);
    hex.push_str("f:");
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    Ok(hex)
}

/// Full streaming SHA-256 for small/medium files.
fn hash_full(path: &Path) -> std::io::Result<[u8; 32]> {
    let f = fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, f);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

/// Sampled fingerprint for very large files — runs in constant time
/// regardless of file size (only 128 KB of disk IO plus metadata).
fn hash_sampled(path: &Path, metadata: &std::fs::Metadata, size: u64) -> std::io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"sampled-v1:");
    hasher.update(&size.to_le_bytes());

    // mtime is part of the fingerprint so re-encoded / touched files get
    // a different hash even if head+tail happen to match.
    if let Ok(mt) = metadata.modified() {
        if let Ok(d) = mt.duration_since(UNIX_EPOCH) {
            hasher.update(&d.as_secs().to_le_bytes());
            hasher.update(&d.subsec_nanos().to_le_bytes());
        }
    }

    let mut f = fs::File::open(path)?;
    let mut buf = vec![0u8; HASH_SAMPLE_SIZE as usize];

    // Head
    let head_n = f.read(&mut buf)?;
    hasher.update(&buf[..head_n]);

    // Tail (only if the file is large enough that head and tail don't overlap)
    if size > HASH_SAMPLE_SIZE * 2 {
        f.seek(SeekFrom::End(-(HASH_SAMPLE_SIZE as i64)))?;
        let tail_n = f.read(&mut buf)?;
        hasher.update(&buf[..tail_n]);
    }

    Ok(hasher.finalize().into())
}

/// Structural fingerprint of a folder.
///
/// Recursively walks `root`, collects every regular file as
/// `(relative_path, size)` (no symlink follow), sorts the list, then SHA-256s
/// the serialized form. Returns a hex digest with `d:` prefix so it never
/// collides with a file hash.
///
/// Properties:
/// - **Stable**: same folder → same hash regardless of OS path separator
///   (we normalise to forward slashes)
/// - **Order-independent**: directory iteration order doesn't affect the result
/// - **Cheap**: only reads file metadata, never file contents — completes
///   in tens of milliseconds even for tens of thousands of files
/// - **False positives possible** if files are replaced with same-size content
///   under the same names — acceptable for de-dup of "did the user drop this
///   folder twice in a row?"
fn compute_folder_hash(root: &Path) -> std::io::Result<String> {
    let mut entries: Vec<(String, u64)> = Vec::new();
    walk_collect(root, root, &mut entries, 0)?;
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    // Hash the file count up front so an empty folder ≠ a folder of one
    // empty file.
    hasher.update(&(entries.len() as u64).to_le_bytes());
    for (rel, size) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(&size.to_le_bytes());
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(2 + digest.len() * 2);
    hex.push_str("d:");
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    Ok(hex)
}

/// Recursive helper that doesn't follow symlinks (uses `symlink_metadata`)
/// and bails out at depth 32 to defend against pathological structures.
fn walk_collect(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, u64)>,
    depth: usize,
) -> std::io::Result<()> {
    if depth > 32 {
        return Ok(()); // give up on absurd nesting
    }
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return Ok(()), // permission denied / vanished — skip silently
    };
    for entry in read.flatten() {
        let p = entry.path();
        let metadata = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = metadata.file_type();
        if ft.is_symlink() {
            // Don't follow — would risk cycles.
            continue;
        }
        if ft.is_dir() {
            let _ = walk_collect(root, &p, out, depth + 1);
        } else if ft.is_file() {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, metadata.len()));
        }
    }
    Ok(())
}

/// Recursively copy `src` folder into `dst`, creating `dst` if missing.
/// Does NOT follow symlinks (avoids cycles). Files inside are copied via
/// `fs::copy`, preserving contents but not extended attributes / ACLs.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let metadata = match fs::symlink_metadata(&src_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = metadata.file_type();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if ft.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Pick the right hashing strategy based on whether `path` is a file or a
/// directory. Returns `None` for paths that don't exist or where hashing
/// fails — callers treat that as "no dedup, just process it".
fn compute_path_hash(path: &Path) -> Option<String> {
    if path.is_file() {
        compute_file_hash(path).ok()
    } else if path.is_dir() {
        compute_folder_hash(path).ok()
    } else {
        None
    }
}
