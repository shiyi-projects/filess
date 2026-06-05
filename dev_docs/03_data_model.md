# 数据模型

## 1. 目标

本文档定义首版数据模型，覆盖 SQLite 关系数据和 Chroma 向量数据。本文档达到 DDL 级别规范，后续实现必须与本文件一致。

## 2. 通用约定

### 2.1 主键与时间字段

1. 所有主键使用 `TEXT`，值为应用侧生成的 UUID v7。
2. 所有时间使用 UTC ISO-8601 字符串，字段类型为 `TEXT`。
3. 布尔值使用 `INTEGER`，仅允许 `0` 或 `1`。

### 2.2 SQLite PRAGMA

初始化数据库时必须执行：

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
```

### 2.3 命名规则

1. 表名使用复数小写下划线风格。
2. 枚举值使用小写蛇形字符串。
3. 所有外键列与引用主键同名风格。

## 3. SQLite 表结构

### 3.1 `batches`

用途：
记录一次拖拽导入产生的批次。

```sql
CREATE TABLE batches (
  batch_id TEXT PRIMARY KEY,
  source_count INTEGER NOT NULL CHECK (source_count >= 0),
  status TEXT NOT NULL CHECK (
    status IN (
      'queued',
      'processing',
      'awaiting_review',
      'completed',
      'completed_with_failures',
      'failed',
      'cancelled'
    )
  ),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE INDEX idx_batches_status ON batches(status);
CREATE INDEX idx_batches_created_at ON batches(created_at);
```

写入时机：

1. 批次创建时插入。
2. 任一任务状态变化导致批次聚合状态变化时更新。
3. 批次结束时回写 `completed_at`。

### 3.2 `tasks`

用途：
记录每个文件或文件夹的处理任务。

```sql
CREATE TABLE tasks (
  task_id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL,
  source_path TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('file', 'folder')),
  status TEXT NOT NULL CHECK (
    status IN (
      'queued',
      'sniffing',
      'parsing',
      'retrieving_context',
      'calling_model',
      'awaiting_review',
      'executing',
      'completed',
      'failed',
      'rolled_back',
      'skipped'
    )
  ),
  error_code TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (batch_id) REFERENCES batches(batch_id) ON DELETE CASCADE
);

CREATE INDEX idx_tasks_batch_id ON tasks(batch_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_source_path ON tasks(source_path);
```

写入时机：

1. 每个输入对象创建一条任务。
2. 每次阶段推进时更新状态。
3. 失败、跳过、回滚时更新终态。

### 3.3 `managed_items`

用途：
记录受管对象当前生效状态。搜索和 UI 展示路径必须以本表为准。

```sql
CREATE TABLE managed_items (
  item_id TEXT PRIMARY KEY,
  latest_task_id TEXT NOT NULL,
  source_path TEXT NOT NULL,
  current_path TEXT NOT NULL,
  current_name TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('file', 'folder')),
  extension TEXT,
  mime_type TEXT,
  category_id TEXT NOT NULL,
  content_summary TEXT,
  is_managed INTEGER NOT NULL DEFAULT 1 CHECK (is_managed IN (0, 1)),
  is_rolled_back INTEGER NOT NULL DEFAULT 0 CHECK (is_rolled_back IN (0, 1)),
  file_size_bytes INTEGER,
  modified_at TEXT,
  last_processed_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (latest_task_id) REFERENCES tasks(task_id),
  FOREIGN KEY (category_id) REFERENCES category_tree(category_id)
);

CREATE UNIQUE INDEX uq_managed_items_current_path ON managed_items(current_path);
CREATE INDEX idx_managed_items_category_id ON managed_items(category_id);
CREATE INDEX idx_managed_items_is_managed ON managed_items(is_managed);
CREATE INDEX idx_managed_items_last_processed_at ON managed_items(last_processed_at);
```

关键字段说明：

1. `source_path`
   首次导入时的原始路径，不因后续移动变化。
2. `current_path`
   当前真实路径。UI 展示和系统跳转只读本字段。
3. `current_name`
   当前文件或文件夹名称，不含父路径。
4. `content_summary`
   用于搜索展示与摘要匹配的文本摘要。
5. `is_managed`
   受管对象是否仍在工具管理范围内。
6. `is_rolled_back`
   最近一次有效操作是否被回滚。

写入时机：

1. 第一次成功整理时插入。
2. 后续重命名、重分类、回滚后更新。
3. 搜索读取当前状态时以本表为单一来源。

### 3.4 `operations`

用途：
记录实际文件系统操作和回滚行为。

```sql
CREATE TABLE operations (
  operation_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  item_id TEXT,
  operation_type TEXT NOT NULL CHECK (
    operation_type IN (
      'move',
      'rename',
      'move_and_rename',
      'rollback_move',
      'rollback_rename',
      'rollback_move_and_rename'
    )
  ),
  source_path TEXT NOT NULL,
  target_path TEXT NOT NULL,
  final_name TEXT NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('success', 'failed', 'rolled_back')),
  file_size INTEGER,
  modified_at TEXT,
  content_hash_optional TEXT,
  error_code TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE,
  FOREIGN KEY (item_id) REFERENCES managed_items(item_id)
);

CREATE INDEX idx_operations_task_id ON operations(task_id);
CREATE INDEX idx_operations_item_id ON operations(item_id);
CREATE INDEX idx_operations_result ON operations(result);
CREATE INDEX idx_operations_created_at ON operations(created_at);
```

关键字段说明：

1. `source_path`
   本次操作开始前路径。
2. `target_path`
   本次操作落地后路径。
3. `content_hash_optional`
   可选一致性校验值，首版允许为空。
4. `result`
   仅记录本条操作结果，不反推任务整体状态。

写入时机：

1. 每次移动、重命名或回滚都插入一条记录。
2. 执行失败时也要插入失败记录。

### 3.5 `reviews`

用途：
记录人工确认和修正结果。

```sql
CREATE TABLE reviews (
  review_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  review_type TEXT NOT NULL CHECK (
    review_type IN (
      'classification',
      'rename',
      'conflict',
      'unknown_type',
      'path_validation'
    )
  ),
  user_decision TEXT NOT NULL CHECK (
    user_decision IN (
      'approve',
      'rename',
      'reroute_unclassified',
      'skip'
    )
  ),
  review_payload TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE
);

CREATE INDEX idx_reviews_task_id ON reviews(task_id);
CREATE INDEX idx_reviews_review_type ON reviews(review_type);
```

说明：

1. `review_payload` 使用 JSON 字符串存储用户输入的新名称、人工选择分类、冲突解决结果等。

写入时机：

1. 每次人工确认提交时插入。

### 3.6 `category_tree`

用途：
记录分类树定义，供本地路径拼装与 UI 分类入口使用。

```sql
CREATE TABLE category_tree (
  category_id TEXT PRIMARY KEY,
  parent_id TEXT,
  display_name TEXT NOT NULL,
  path_segment TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (parent_id) REFERENCES category_tree(category_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX uq_category_tree_parent_segment
  ON category_tree(parent_id, path_segment);
CREATE INDEX idx_category_tree_parent_id ON category_tree(parent_id);
CREATE INDEX idx_category_tree_enabled ON category_tree(is_enabled);
```

说明：

1. `category_id` 是逻辑 ID，例如 `finance/invoice`。
2. `path_segment` 是单层目录名，不包含路径分隔符。
3. 不允许删除正在被 `managed_items` 使用的分类。

### 3.7 `search_index`

用途：
为搜索框提供全文检索能力。首版固定使用 SQLite FTS5。

```sql
CREATE VIRTUAL TABLE search_index USING fts5(
  item_id UNINDEXED,
  file_name_text,
  category_text,
  path_text,
  summary_text,
  tokenize = 'unicode61 remove_diacritics 2'
);
```

字段约束：

1. `item_id`
   对应 `managed_items.item_id`，由应用层维护一致性。
2. `file_name_text`
   来自 `managed_items.current_name`。
3. `category_text`
   来自分类展示名或分类路径文本。
4. `path_text`
   来自 `managed_items.current_path`。
5. `summary_text`
   来自 `managed_items.content_summary`。

写入时机：

1. `managed_items` 首次插入时写入。
2. 名称、分类、路径或摘要任一变化时重建对应索引行。
3. 回滚后同步刷新索引。

### 3.8 `settings`

用途：
存储本地设置项，供设置页与运行时配置读取。

```sql
CREATE TABLE settings (
  setting_key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

固定键：

1. `paths`
2. `classification_rules`
3. `ai`
4. `organize_policy`
5. `data_and_logs`

说明：

1. `value_json` 存储对应分组完整 JSON。
2. 不额外建快照表，首版只保留最新值。

### 3.9 `rag_samples`

用途：
记录允许进入向量检索的高质量样本元数据。

```sql
CREATE TABLE rag_samples (
  sample_id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL,
  source_task_id TEXT NOT NULL,
  vector_id TEXT NOT NULL UNIQUE,
  feature_text TEXT NOT NULL,
  final_category_id TEXT NOT NULL,
  final_name TEXT NOT NULL,
  confirmed_by_user INTEGER NOT NULL CHECK (confirmed_by_user IN (0, 1)),
  was_rolled_back INTEGER NOT NULL DEFAULT 0 CHECK (was_rolled_back IN (0, 1)),
  quality_tier TEXT NOT NULL CHECK (quality_tier IN ('confirmed', 'high_confidence')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (item_id) REFERENCES managed_items(item_id),
  FOREIGN KEY (source_task_id) REFERENCES tasks(task_id),
  FOREIGN KEY (final_category_id) REFERENCES category_tree(category_id)
);

CREATE INDEX idx_rag_samples_item_id ON rag_samples(item_id);
CREATE INDEX idx_rag_samples_category_id ON rag_samples(final_category_id);
CREATE INDEX idx_rag_samples_quality_tier ON rag_samples(quality_tier);
CREATE INDEX idx_rag_samples_was_rolled_back ON rag_samples(was_rolled_back);
```

写入门槛：

1. 人工确认成功样本
2. 高置信度且未撤销样本

回滚规则：

1. 一旦对应整理结果被回滚，必须将 `was_rolled_back` 更新为 `1`。
2. `was_rolled_back = 1` 的样本不得再参与未来检索。

## 4. Chroma 数据模型

集合名称固定为：`rag_samples_v1`

### 4.1 向量 ID

1. 使用 `rag_samples.vector_id`
2. 与 SQLite 中 `sample_id` 一一对应

### 4.2 存储文本

1. 文本特征来自 `rag_samples.feature_text`
2. 不存原始整文件内容

### 4.3 Chroma 元数据字段

1. `sample_id`
2. `item_id`
3. `final_category_id`
4. `confirmed_by_user`
5. `was_rolled_back`
6. `quality_tier`

### 4.4 写入规则

1. 只写入通过质量门槛的样本
2. 回滚样本在 Chroma 中必须删除或标记不可检索
3. 检索时过滤 `was_rolled_back = 0`

## 5. 状态枚举

### 5.1 批次状态

1. `queued`
2. `processing`
3. `awaiting_review`
4. `completed`
5. `completed_with_failures`
6. `failed`
7. `cancelled`

### 5.2 任务状态

1. `queued`
2. `sniffing`
3. `parsing`
4. `retrieving_context`
5. `calling_model`
6. `awaiting_review`
7. `executing`
8. `completed`
9. `failed`
10. `rolled_back`
11. `skipped`

### 5.3 操作类型

1. `move`
2. `rename`
3. `move_and_rename`
4. `rollback_move`
5. `rollback_rename`
6. `rollback_move_and_rename`

### 5.4 Review 类型

1. `classification`
2. `rename`
3. `conflict`
4. `unknown_type`
5. `path_validation`

## 6. 状态流转表

| 对象 | 起始状态 | 目标状态 | 触发条件 |
| --- | --- | --- | --- |
| `tasks` | `queued` | `sniffing` | Rust 开始识别输入对象 |
| `tasks` | `sniffing` | `parsing` | Sidecar 开始提取内容或浅采样 |
| `tasks` | `parsing` | `retrieving_context` | 特征提取成功，进入 RAG 检索 |
| `tasks` | `retrieving_context` | `calling_model` | 相似案例已返回 |
| `tasks` | `calling_model` | `awaiting_review` | 低置信度、冲突、未知类型或规则校验失败 |
| `tasks` | `calling_model` | `executing` | 建议可直接执行 |
| `tasks` | `awaiting_review` | `executing` | 用户确认后允许执行 |
| `tasks` | `executing` | `completed` | 文件系统操作与数据库写入成功 |
| `tasks` | 任意处理中状态 | `failed` | 不可恢复错误或达到重试上限 |
| `tasks` | `completed` | `rolled_back` | 用户成功执行撤销 |

## 7. 与功能的对应关系

1. UI 展示路径
   读取 `managed_items.current_path`
2. 搜索框匹配
   查询 `search_index`
3. 撤销可行性
   读取 `operations` 与 `managed_items`
4. 分类入口
   读取 `category_tree`
5. RAG 检索
   读取 `rag_samples` 与 Chroma
6. 设置页
   读取和写入 `settings`

## 8. 数据一致性规则

1. 任一成功的物理移动或重命名，必须同时更新 `operations`、`managed_items`、`search_index`。
2. 任一成功回滚，必须同时更新 `tasks`、`operations`、`managed_items`、`rag_samples`、`search_index`。
3. `managed_items.current_path` 必须与最后一个成功操作后的真实路径一致。
4. `category_tree` 中被禁用或删除的分类，不得出现在新的 AI 结果落地路径中。
