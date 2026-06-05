//! SQLite-backed persistence for batches, tasks, and classified items.
//!
//! Embeddings are stored as little-endian f32 blobs in `items.embedding`.
//! Semantic search is done in Rust via linear cosine similarity over all
//! loaded vectors — sufficient for desktop-scale corpora (< 100k items).

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

use crate::services::orchestrator::CompletedResult;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS items (
    id             TEXT PRIMARY KEY,
    file_name      TEXT NOT NULL,
    category_name  TEXT NOT NULL,
    current_path   TEXT NOT NULL UNIQUE,
    source_path    TEXT,
    item_type      TEXT NOT NULL,
    feature_text   TEXT,
    embedding      BLOB,
    confidence     REAL,
    processed_at   TEXT NOT NULL,
    operation_id   TEXT,
    file_hash      TEXT
);

CREATE INDEX IF NOT EXISTS idx_items_category     ON items(category_name);
CREATE INDEX IF NOT EXISTS idx_items_processed_at ON items(processed_at DESC);
CREATE INDEX IF NOT EXISTS idx_items_file_name    ON items(file_name);
CREATE INDEX IF NOT EXISTS idx_items_file_hash    ON items(file_hash);

-- Trigram FTS5 index for substring search across file_name / current_path /
-- category_name. Trigram tokenizer is byte-level, so it works equally well
-- for CJK text (each Chinese char = 3 UTF-8 bytes ≈ 1 trigram).
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
    item_id UNINDEXED,
    file_name,
    current_path,
    category_name,
    tokenize = 'trigram'
);

-- Keep FTS in sync via triggers. ON CONFLICT DO UPDATE on items fires the
-- AFTER UPDATE trigger, so upsert paths are covered.
CREATE TRIGGER IF NOT EXISTS items_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(item_id, file_name, current_path, category_name)
    VALUES (NEW.id, NEW.file_name, NEW.current_path, NEW.category_name);
END;

CREATE TRIGGER IF NOT EXISTS items_au AFTER UPDATE ON items BEGIN
    DELETE FROM items_fts WHERE item_id = OLD.id;
    INSERT INTO items_fts(item_id, file_name, current_path, category_name)
    VALUES (NEW.id, NEW.file_name, NEW.current_path, NEW.category_name);
END;

CREATE TRIGGER IF NOT EXISTS items_ad AFTER DELETE ON items BEGIN
    DELETE FROM items_fts WHERE item_id = OLD.id;
END;

-- Per-key string store for user preferences (file_operation_mode, etc.).
-- Values are JSON-encoded so we can hold enums / numbers / bools uniformly.
CREATE TABLE IF NOT EXISTS app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Category tree. Each row is one full path like "工作/项目A/资料".
-- Kept in sync by triggers on `items` — no manual maintenance required.
-- We don't prune on DELETE (empty-category residue is harmless) to avoid
-- counting rows in a hot path.
CREATE TABLE IF NOT EXISTS category_paths (
    path      TEXT PRIMARY KEY,
    last_seen TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_category_paths_last_seen
    ON category_paths(last_seen DESC);

CREATE TRIGGER IF NOT EXISTS items_category_sync_ai
AFTER INSERT ON items
BEGIN
    INSERT INTO category_paths(path, last_seen)
    VALUES (NEW.category_name, NEW.processed_at)
    ON CONFLICT(path) DO UPDATE SET last_seen = excluded.last_seen;
END;

CREATE TRIGGER IF NOT EXISTS items_category_sync_au
AFTER UPDATE OF category_name ON items
BEGIN
    INSERT INTO category_paths(path, last_seen)
    VALUES (NEW.category_name, NEW.processed_at)
    ON CONFLICT(path) DO UPDATE SET last_seen = excluded.last_seen;
END;
"#;

#[derive(Debug, Clone)]
pub struct ItemRecord {
    pub id: String,
    pub file_name: String,
    pub category_name: String,
    pub current_path: String,
    pub source_path: Option<String>,
    pub item_type: String,
    pub feature_text: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub confidence: Option<f64>,
    pub processed_at: String,
    pub operation_id: Option<String>,
    pub file_hash: Option<String>,
}

impl From<&ItemRecord> for CompletedResult {
    fn from(r: &ItemRecord) -> Self {
        CompletedResult {
            item_id: r.id.clone(),
            file_name: r.file_name.clone(),
            category_name: r.category_name.clone(),
            current_path: r.current_path.clone(),
            item_type: r.item_type.clone(),
            processed_at: r.processed_at.clone(),
            operation_id: r.operation_id.clone(),
        }
    }
}

pub fn open(db_path: &Path) -> Result<Connection, String> {
    if let Some(dir) = db_path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create_dir_all({}) failed: {}", dir.display(), e))?;
    }
    let conn = Connection::open(db_path)
        .map_err(|e| format!("SQLite open({}) failed: {}", db_path.display(), e))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("SQLite pragma failed: {}", e))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| format!("SQLite schema failed: {}", e))?;
    migrate(&conn)?;
    Ok(conn)
}

/// Best-effort additive migrations for older DB files. Each step is wrapped
/// in its own transaction and ignores "duplicate column" errors so reruns
/// are safe.
fn migrate(conn: &Connection) -> Result<(), String> {
    // Drop the legacy `operations` table. Undo was reworked to rely solely on
    // the items row, so this table only accumulated orphan rows. Safe no-op on
    // DBs that never had it.
    let _ = conn.execute_batch("DROP TABLE IF EXISTS operations;");

    let has_hash_col = column_exists(conn, "items", "file_hash")?;
    if !has_hash_col {
        conn.execute_batch(
            "ALTER TABLE items ADD COLUMN file_hash TEXT;
             CREATE INDEX IF NOT EXISTS idx_items_file_hash ON items(file_hash);",
        )
        .map_err(|e| format!("migration add file_hash failed: {}", e))?;
    }

    // Backfill category_paths for DBs that existed before the trigger was added.
    // Trigger keeps it in sync going forward; this one-shot query covers history.
    let cat_path_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM category_paths", [], |r| r.get(0))
        .unwrap_or(0);
    if cat_path_count == 0 {
        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO category_paths(path, last_seen)
                 SELECT category_name, COALESCE(MAX(processed_at), '') FROM items
                 GROUP BY category_name",
                [],
            )
            .unwrap_or(0);
        if inserted > 0 {
            println!("[storage] backfilled category_paths with {} rows", inserted);
        }
    }

    // Backfill FTS index if it's empty but `items` has rows. Triggers will
    // keep it current going forward, so this only runs on the first launch
    // after upgrading to the FTS-enabled schema.
    let items_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
        .map_err(|e| format!("count items: {}", e))?;
    let fts_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM items_fts", [], |r| r.get(0))
        .map_err(|e| format!("count items_fts: {}", e))?;
    if items_count > 0 && fts_count < items_count {
        conn.execute("DELETE FROM items_fts", [])
            .map_err(|e| format!("clear items_fts: {}", e))?;
        let inserted = conn
            .execute(
                "INSERT INTO items_fts(item_id, file_name, current_path, category_name)
                 SELECT id, file_name, current_path, category_name FROM items",
                [],
            )
            .map_err(|e| format!("backfill items_fts: {}", e))?;
        println!("[storage] backfilled FTS5 index with {} rows", inserted);
    }

    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("table_info prepare: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("table_info query: {}", e))?;
    for r in rows {
        if r.map_err(|e| e.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn upsert_item(conn: &Connection, item: &ItemRecord) -> Result<(), String> {
    let embedding_bytes: Option<Vec<u8>> = item.embedding.as_ref().map(embedding_to_bytes);
    conn.execute(
        "INSERT INTO items
            (id, file_name, category_name, current_path, source_path, item_type,
             feature_text, embedding, confidence, processed_at, operation_id, file_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(current_path) DO UPDATE SET
            file_name     = excluded.file_name,
            category_name = excluded.category_name,
            source_path   = excluded.source_path,
            item_type     = excluded.item_type,
            feature_text  = excluded.feature_text,
            embedding     = excluded.embedding,
            confidence    = excluded.confidence,
            processed_at  = excluded.processed_at,
            operation_id  = excluded.operation_id,
            file_hash     = excluded.file_hash",
        params![
            item.id,
            item.file_name,
            item.category_name,
            item.current_path,
            item.source_path,
            item.item_type,
            item.feature_text,
            embedding_bytes,
            item.confidence,
            item.processed_at,
            item.operation_id,
            item.file_hash,
        ],
    )
    .map_err(|e| format!("upsert_item failed: {}", e))?;
    Ok(())
}

/// Look up an item by its file content hash. Used to short-circuit re-processing
/// of files we've already seen (even if renamed or copied to a new path).
pub fn find_item_by_hash(
    conn: &Connection,
    hash: &str,
) -> Result<Option<ItemRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, category_name, current_path, source_path, item_type,
                    feature_text, embedding, confidence, processed_at, operation_id, file_hash
             FROM items WHERE file_hash = ?1
             ORDER BY processed_at DESC
             LIMIT 1",
        )
        .map_err(|e| format!("prepare find_item_by_hash: {}", e))?;
    stmt.query_row(params![hash], row_to_item)
        .optional()
        .map_err(|e| format!("query find_item_by_hash: {}", e))
}

/// Lightweight projection used by the sidebar's category tree — skips
/// `feature_text` and `embedding` blobs which can be many KB per item.
#[derive(Debug, Clone)]
pub struct BriefItem {
    pub id: String,
    pub file_name: String,
    pub category_name: String,
    pub current_path: String,
    pub item_type: String,
    pub processed_at: String,
    pub operation_id: Option<String>,
}

/// List every distinct category path currently known to the DB. Used by the
/// classifier to build a directory-tree snapshot for the LLM prompt. Rows
/// are ordered by `last_seen DESC` so freshly-used paths appear first.
pub fn list_category_paths(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT path FROM category_paths ORDER BY last_seen DESC",
        )
        .map_err(|e| format!("prepare list_category_paths: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query list_category_paths: {}", e))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row list_category_paths: {}", e))?);
    }
    Ok(out)
}

/// Returns the most-used multi-level categories (those with at least one `/`),
/// ordered by frequency descending. Used to seed the LLM classifier so it
/// re-uses existing sub-categories instead of inventing semantically-overlapping
/// new ones (e.g. avoiding both "工作" and "职场").
pub fn recent_subcategories(conn: &Connection, limit: usize) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT category_name, COUNT(*) AS cnt, MAX(processed_at) AS last_seen
             FROM items
             WHERE category_name LIKE '%/%'
             GROUP BY category_name
             ORDER BY cnt DESC, last_seen DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("prepare recent_subcategories: {}", e))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query recent_subcategories: {}", e))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row recent_subcategories: {}", e))?);
    }
    Ok(out)
}

pub fn all_items_brief(conn: &Connection) -> Result<Vec<BriefItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, category_name, current_path, item_type,
                    processed_at, operation_id
             FROM items
             ORDER BY processed_at DESC",
        )
        .map_err(|e| format!("prepare all_items_brief: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(BriefItem {
                id: row.get(0)?,
                file_name: row.get(1)?,
                category_name: row.get(2)?,
                current_path: row.get(3)?,
                item_type: row.get(4)?,
                processed_at: row.get(5)?,
                operation_id: row.get(6)?,
            })
        })
        .map_err(|e| format!("query all_items_brief: {}", e))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row all_items_brief: {}", e))?);
    }
    Ok(out)
}

pub fn recent_items(conn: &Connection, limit: usize) -> Result<Vec<ItemRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, category_name, current_path, source_path, item_type,
                    feature_text, embedding, confidence, processed_at, operation_id, file_hash
             FROM items
             ORDER BY processed_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("prepare recent_items: {}", e))?;
    let rows = stmt
        .query_map(params![limit as i64], row_to_item)
        .map_err(|e| format!("query_map recent_items: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("row recent_items: {}", e))?);
    }
    Ok(out)
}

pub fn find_item_by_id(conn: &Connection, id: &str) -> Result<Option<ItemRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, category_name, current_path, source_path, item_type,
                    feature_text, embedding, confidence, processed_at, operation_id, file_hash
             FROM items WHERE id = ?1",
        )
        .map_err(|e| format!("prepare find_item: {}", e))?;
    stmt.query_row(params![id], row_to_item)
        .optional()
        .map_err(|e| format!("query find_item: {}", e))
}

/// Fetch (id, embedding) pairs for every item with a non-null embedding.
/// Used by semantic search — kept lightweight (no text payloads).
pub fn all_embeddings(conn: &Connection) -> Result<Vec<(String, Vec<f32>)>, String> {
    let mut stmt = conn
        .prepare("SELECT id, embedding FROM items WHERE embedding IS NOT NULL")
        .map_err(|e| format!("prepare all_embeddings: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((id, bytes))
        })
        .map_err(|e| format!("query_map all_embeddings: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, bytes) = row.map_err(|e| format!("row all_embeddings: {}", e))?;
        if let Some(vec) = bytes_to_embedding(&bytes) {
            out.push((id, vec));
        }
    }
    Ok(out)
}

pub fn find_items_by_ids(
    conn: &Connection,
    ids: &[String],
) -> Result<Vec<ItemRecord>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("?{}", i))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, file_name, category_name, current_path, source_path, item_type,
                feature_text, embedding, confidence, processed_at, operation_id, file_hash
         FROM items WHERE id IN ({})",
        placeholders
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare find_items_by_ids: {}", e))?;
    let params_iter: Vec<&dyn rusqlite::ToSql> =
        ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt
        .query_map(params_iter.as_slice(), row_to_item)
        .map_err(|e| format!("query_map find_items_by_ids: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("row find_items_by_ids: {}", e))?);
    }
    Ok(out)
}

/// Full-text substring search via FTS5 trigram index. Wraps the user's input
/// as a phrase ("…") so query syntax characters (`-`, `*`, `AND`, `OR`,
/// parentheses, etc.) are treated literally rather than as FTS operators.
/// Embedded double-quotes are doubled to escape them inside the phrase.
pub fn search_by_text(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<ItemRecord>, String> {
    let escaped = query.replace('"', "\"\"");
    let match_expr = format!("\"{}\"", escaped);
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.file_name, i.category_name, i.current_path, i.source_path,
                    i.item_type, i.feature_text, i.embedding, i.confidence,
                    i.processed_at, i.operation_id, i.file_hash
             FROM items_fts f
             JOIN items i ON i.id = f.item_id
             WHERE items_fts MATCH ?1
             ORDER BY bm25(items_fts), i.processed_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("prepare search_by_text: {}", e))?;
    let rows = stmt
        .query_map(params![match_expr, limit as i64], row_to_item)
        .map_err(|e| format!("query search_by_text: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("row search_by_text: {}", e))?);
    }
    Ok(out)
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRecord> {
    let embedding_bytes: Option<Vec<u8>> = row.get(7)?;
    Ok(ItemRecord {
        id: row.get(0)?,
        file_name: row.get(1)?,
        category_name: row.get(2)?,
        current_path: row.get(3)?,
        source_path: row.get(4)?,
        item_type: row.get(5)?,
        feature_text: row.get(6)?,
        embedding: embedding_bytes.as_deref().and_then(bytes_to_embedding),
        confidence: row.get(8)?,
        processed_at: row.get(9)?,
        operation_id: row.get(10)?,
        file_hash: row.get::<_, Option<String>>(11).unwrap_or(None),
    })
}

fn embedding_to_bytes(vec: &Vec<f32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * 4);
    for &f in vec {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn bytes_to_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(out)
}

// ── Settings ─────────────────────────────────────────────────

pub fn load_settings(conn: &Connection) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_settings")
        .map_err(|e| format!("prepare load_settings: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let k: String = row.get(0)?;
            let v: String = row.get(1)?;
            Ok((k, v))
        })
        .map_err(|e| format!("query load_settings: {}", e))?;
    let mut out = HashMap::new();
    for r in rows {
        let (k, v) = r.map_err(|e| format!("row load_settings: {}", e))?;
        out.insert(k, v);
    }
    Ok(out)
}

pub fn save_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| format!("save_setting failed: {}", e))?;
    Ok(())
}

pub fn delete_item(conn: &Connection, item_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM items WHERE id = ?1", params![item_id])
        .map_err(|e| format!("delete_item failed: {}", e))?;
    Ok(())
}

/// Cosine similarity. Returns 0.0 if either vector is zero-length or has
/// mismatched dimensions.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
