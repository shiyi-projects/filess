# 接口契约

## 1. 目标

本文档定义前端、Rust、Python Sidecar 之间的稳定接口。后续实现不得私自新增字段、改变语义或修改错误码含义。

## 2. 通用约定

### 2.1 数据编码

1. 所有接口使用 UTF-8。
2. 时间统一为 UTC ISO-8601 字符串。
3. 主键统一为 UUID v7 字符串。

### 2.2 错误结构

所有命令和 Sidecar 调用统一返回如下错误结构：

```json
{
  "code": "string",
  "message": "string",
  "details": {}
}
```

固定错误码：

1. `invalid_input`
2. `not_found`
3. `validation_failed`
4. `path_policy_blocked`
5. `sidecar_unavailable`
6. `sidecar_timeout`
7. `model_request_failed`
8. `storage_failed`
9. `filesystem_failed`
10. `conflict_requires_review`

### 2.3 超时语义

1. Tauri 命令默认超时为 `30s`，文件操作命令允许 `60s`。
2. Sidecar 解析默认超时为 `15s`。
3. Sidecar 调用硅基流动聊天模型默认超时为 `30s`。
4. Sidecar 调用 embedding 默认超时为 `20s`。
5. 超时必须返回结构化错误，不得静默吞掉。

## 3. Tauri 命令

### 3.1 `enqueue_items`

用途：
创建批次并将拖入路径加入任务队列。

请求：

```json
{
  "paths": ["C:\\\\Users\\\\Alice\\\\Downloads\\\\invoice.pdf"]
}
```

响应：

```json
{
  "batch_id": "uuid",
  "task_ids": ["uuid"],
  "status": "queued"
}
```

约束：

1. `paths` 不能为空。
2. 路径必须存在，否则返回 `invalid_input`。

### 3.2 `get_batch_status`

用途：
获取批次概览和任务列表。

请求：

```json
{
  "batch_id": "uuid"
}
```

响应：

```json
{
  "batch_id": "uuid",
  "status": "processing",
  "summary": {
    "total": 10,
    "completed": 4,
    "failed": 1,
    "awaiting_review": 2
  },
  "tasks": [
    {
      "task_id": "uuid",
      "source_path": "C:\\\\in\\\\a.txt",
      "status": "parsing",
      "error_message": null
    }
  ]
}
```

### 3.3 `search_files`

用途：
在已纳入管理的文件中搜索。

请求：

```json
{
  "query": "发票 供应商",
  "filters": {
    "category_id": null,
    "item_type": null
  },
  "limit": 20,
  "offset": 0
}
```

响应：

```json
{
  "total": 1,
  "items": [
    {
      "item_id": "uuid",
      "title": "2026-03 供应商发票.pdf",
      "category_id": "finance/invoice",
      "category_name": "发票",
      "current_path": "D:\\\\Files\\\\财务\\\\发票\\\\2026-03 供应商发票.pdf",
      "summary_excerpt": "包含供应商、金额、日期等摘要",
      "matched_fields": ["file_name_text", "summary_text"],
      "last_processed_at": "2026-04-22T09:00:00Z"
    }
  ]
}
```

约束：

1. 仅查询 `search_index` 与 `managed_items`。
2. 不扫描整个磁盘。

### 3.4 `review_task`

用途：
提交人工确认结果。

请求：

```json
{
  "task_id": "uuid",
  "action": "approve",
  "payload": {
    "category_id": "finance/invoice",
    "new_name": "2026-03 供应商发票.pdf"
  }
}
```

`action` 允许值：

1. `approve`
2. `rename`
3. `reroute_unclassified`
4. `skip`

响应：

```json
{
  "task_id": "uuid",
  "status": "executing"
}
```

### 3.5 `undo_operation`

用途：
撤销一次成功操作。

请求：

```json
{
  "operation_id": "uuid"
}
```

响应：

```json
{
  "operation_id": "uuid",
  "task_id": "uuid",
  "status": "rolled_back"
}
```

### 3.6 `open_file`

用途：
用系统默认程序打开文件。

请求：

```json
{
  "item_id": "uuid"
}
```

响应：

```json
{
  "item_id": "uuid",
  "opened": true
}
```

### 3.7 `reveal_in_folder`

用途：
在系统资源管理器中定位文件。

请求：

```json
{
  "item_id": "uuid"
}
```

响应：

```json
{
  "item_id": "uuid",
  "revealed": true
}
```

### 3.8 `copy_path`

用途：
复制文件当前路径到剪贴板。

请求：

```json
{
  "item_id": "uuid"
}
```

响应：

```json
{
  "item_id": "uuid",
  "current_path": "D:\\\\Files\\\\财务\\\\发票\\\\2026-03 供应商发票.pdf",
  "copied": true
}
```

### 3.9 `get_settings`

用途：
获取设置页需要的完整设置快照。

请求：

```json
{}
```

响应：

```json
{
  "paths": {},
  "classification_rules": {},
  "ai": {},
  "organize_policy": {},
  "data_and_logs": {}
}
```

### 3.10 `save_settings`

用途：
保存设置页内容。

请求：

```json
{
  "paths": {},
  "classification_rules": {},
  "ai": {},
  "organize_policy": {},
  "data_and_logs": {}
}
```

响应：

```json
{
  "saved": true,
  "updated_at": "2026-04-22T09:00:00Z"
}
```

## 4. 前后端核心数据结构

### 4.1 `TaskSummary`

```json
{
  "task_id": "string",
  "source_path": "string",
  "status": "queued|sniffing|parsing|retrieving_context|calling_model|awaiting_review|executing|completed|failed|rolled_back|skipped",
  "error_message": "string|null"
}
```

### 4.2 `ReviewPayload`

```json
{
  "category_id": "string|null",
  "new_name": "string|null",
  "notes": "string|null"
}
```

### 4.3 `SearchResultItem`

```json
{
  "item_id": "string",
  "title": "string",
  "category_id": "string",
  "category_name": "string",
  "current_path": "string",
  "summary_excerpt": "string|null",
  "matched_fields": ["string"],
  "last_processed_at": "string"
}
```

## 5. Python Sidecar 契约

### 5.1 协议

Rust 与 Sidecar 使用 `JSON-RPC over stdio`，消息格式固定为：

请求：

```json
{
  "jsonrpc": "2.0",
  "id": "uuid",
  "method": "parse_item",
  "params": {}
}
```

成功响应：

```json
{
  "jsonrpc": "2.0",
  "id": "uuid",
  "result": {}
}
```

失败响应：

```json
{
  "jsonrpc": "2.0",
  "id": "uuid",
  "error": {
    "code": "string",
    "message": "string",
    "details": {}
  }
}
```

### 5.2 `parse_item`

输入：

```json
{
  "source_path": "C:\\\\Users\\\\Alice\\\\Downloads\\\\invoice.pdf"
}
```

输出：

```json
{
  "item_type": "file",
  "name": "invoice.pdf",
  "extension": ".pdf",
  "mime_type": "application/pdf",
  "size_bytes": 12034,
  "created_at": "2026-04-20T01:00:00Z",
  "modified_at": "2026-04-21T01:00:00Z",
  "content_excerpt": "前几段文本",
  "folder_sample": null
}
```

### 5.3 `build_features`

输入：

```json
{
  "parsed_item": {}
}
```

输出：

```json
{
  "feature_text": "文件名、扩展名、摘要、目录语义等拼接后的标准化文本",
  "content_summary": "用于 UI 与搜索的摘要文本"
}
```

### 5.4 `retrieve_context`

输入：

```json
{
  "feature_text": "string",
  "top_k": 3
}
```

输出：

```json
{
  "matches": [
    {
      "sample_id": "uuid",
      "final_category_id": "finance/invoice",
      "final_name": "2026-03 供应商发票.pdf",
      "score": 0.91
    }
  ]
}
```

### 5.5 `classify_item`

输入：

```json
{
  "feature_text": "string",
  "content_summary": "string",
  "context_examples": [],
  "category_tree": [],
  "policy": {
    "allow_cloud_excerpt": true,
    "max_excerpt_chars": 1200
  }
}
```

输出：

```json
{
  "category_id": "finance/invoice",
  "suggested_name": "2026-03 供应商发票.pdf",
  "confidence": 0.92,
  "reason": "文件名与内容均指向发票场景",
  "need_human_review": false
}
```

规则：

1. Sidecar 不返回物理路径。
2. `category_id` 必须来自输入的分类树。

### 5.6 `embed_sample`

输入：

```json
{
  "sample_id": "uuid",
  "feature_text": "string"
}
```

输出：

```json
{
  "vector_id": "uuid"
}
```

### 5.7 `write_rag_sample`

输入：

```json
{
  "sample_id": "uuid",
  "vector_id": "uuid",
  "item_id": "uuid",
  "feature_text": "string",
  "final_category_id": "finance/invoice",
  "confirmed_by_user": 1,
  "was_rolled_back": 0,
  "quality_tier": "confirmed"
}
```

输出：

```json
{
  "written": true
}
```

## 6. 硅基流动适配层

Sidecar 的模型配置固定从 `settings.ai` 读取以下字段：

1. `api_base_url`
2. `api_key`
3. `chat_model`
4. `embedding_model`
5. `request_timeout_seconds`
6. `max_retries`
7. `max_excerpt_chars`

固定约束：

1. 首版仅接硅基流动。
2. 聊天模型与 embedding 模型作为配置项，不在代码中硬编码常量值。
3. Sidecar 必须对非 JSON 响应、超时、限流和鉴权失败做结构化错误映射。
