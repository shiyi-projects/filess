use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub task_id: String,
    pub source_path: String,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub batch_id: String,
    pub status: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub awaiting_review: usize,
    pub tasks: Vec<TaskSummary>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultItem {
    pub item_id: String,
    pub title: String,
    pub category_id: String,
    pub category_name: String,
    pub current_path: String,
    pub summary_excerpt: Option<String>,
    pub matched_fields: Vec<String>,
    pub last_processed_at: String,
    pub score: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub semantic: Vec<SearchResultItem>,
    pub filename: Vec<SearchResultItem>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub paths: serde_json::Value,
    pub classification_rules: serde_json::Value,
    pub ai: serde_json::Value,
    pub organize_policy: serde_json::Value,
    pub data_and_logs: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrapState {
    pub batches: Vec<BatchSummary>,
    pub search_results: Vec<SearchResultItem>,
    pub settings: SettingsSnapshot,
    pub recent_results: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkippedItem {
    /// The path the user dropped — already classified previously.
    pub source_path: String,
    /// Where the existing copy lives now.
    pub existing_path: String,
    /// item_id of the existing record, so the front-end can navigate to it.
    pub existing_id: String,
    pub category_name: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    /// `None` when every input was a duplicate — no batch was created.
    pub batch_id: Option<String>,
    pub queued_count: usize,
    pub skipped: Vec<SkippedItem>,
}

/// User-tunable knobs surfaced in the QuickConfigBar above the drop zone.
/// Mirrors the three corresponding fields on `OrganizerConfig`.
#[derive(Debug, Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePolicy {
    pub file_operation_mode: crate::services::orchestrator::FileOperationMode,
    pub source_disposition: crate::services::orchestrator::SourceDisposition,
    pub auto_unclassify_low_confidence: bool,
}

/// Full editable app configuration surfaced in the SettingsDrawer.
/// Same shape used for both `get_app_settings` (full read) and the
/// "save changes" path; absent / empty fields fall back to the previous
/// in-memory value on the backend.
#[derive(Debug, Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub api_key: String,
    pub chat_model: String,
    pub embedding_model: String,
    pub target_root: String,
    pub unclassified_root: String,
    pub low_confidence_threshold: f64,
    pub categories: Vec<String>,
    pub file_operation_mode: crate::services::orchestrator::FileOperationMode,
    pub source_disposition: crate::services::orchestrator::SourceDisposition,
    pub auto_unclassify_low_confidence: bool,
    pub max_concurrent_workers: usize,
    pub search_hotkey: String,
    pub max_top_level_categories: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    pub logical: usize,
    pub recommended: usize,
}

