use crate::models::{AppBootstrapState, BatchSummary, SearchResultItem, SettingsSnapshot, TaskSummary};

pub fn sample_bootstrap_state() -> AppBootstrapState {
    AppBootstrapState {
        batches: vec![BatchSummary {
            batch_id: "batch-demo-001".to_string(),
            status: "processing".to_string(),
            total: 3,
            completed: 1,
            failed: 0,
            awaiting_review: 1,
            tasks: vec![
                TaskSummary {
                    task_id: "task-demo-001".to_string(),
                    source_path: r"D:\SoftwareData\rust\Filess\dev_docs\development_plan.md"
                        .to_string(),
                    status: "completed".to_string(),
                    error_message: None,
                },
                TaskSummary {
                    task_id: "task-demo-002".to_string(),
                    source_path: r"D:\SoftwareData\rust\Filess\dev_docs\01_product_contract.md"
                        .to_string(),
                    status: "awaiting_review".to_string(),
                    error_message: Some("建议名称需要人工确认".to_string()),
                },
                TaskSummary {
                    task_id: "task-demo-003".to_string(),
                    source_path: r"D:\SoftwareData\rust\Filess\dev_docs".to_string(),
                    status: "completed".to_string(),
                    error_message: None,
                },
            ],
        }],
        search_results: vec![SearchResultItem {
            item_id: "item-demo-001".to_string(),
            title: "development_plan.md".to_string(),
            category_id: "work/dev-docs".to_string(),
            category_name: "开发文档".to_string(),
            current_path: r"D:\SoftwareData\rust\Filess\dev_docs\development_plan.md".to_string(),
            summary_excerpt: Some("当前工作区中的开发总纲文档，可用于验证路径展示与本地跳转。".to_string()),
            matched_fields: vec!["file_name_text".to_string(), "summary_text".to_string()],
            last_processed_at: "2026-04-22T09:00:00Z".to_string(),
            score: None,
        }],
        settings: SettingsSnapshot {
            paths: serde_json::json!({
                "targetRoot": r"D:\Files",
                "unclassifiedRoot": r"D:\Files\未分类",
                "stagingRoot": r"D:\Files\.staging"
            }),
            classification_rules: serde_json::json!({
                "topLevel": ["财务", "生活", "工作"],
                "folderSamplingEnabled": true
            }),
            ai: serde_json::json!({
                "provider": "siliconflow",
                "chatModel": "to-be-configured",
                "embeddingModel": "to-be-configured"
            }),
            organize_policy: serde_json::json!({
                "lowConfidenceThreshold": 0.8,
                "conflictPolicy": "append_counter",
                "previewFirst": true
            }),
            data_and_logs: serde_json::json!({
                "databasePath": "data/app.db",
                "logLevel": "info",
                "historyRetentionDays": 180
            }),
        },
        recent_results: vec![serde_json::json!({
            "itemId": "item-demo-001",
            "fileName": "development_plan.md",
            "categoryName": "寮€鍙戞枃妗?",
            "currentPath": r"D:\SoftwareData\rust\Filess\dev_docs\development_plan.md",
            "itemType": "file",
            "processedAt": "2026-04-22T09:00:00Z",
            "operationId": "batch-demo-001"
        })],
    }
}
