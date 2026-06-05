"""RPC handler implementations — the actual AI pipeline logic."""
from __future__ import annotations

import json
import mimetypes
import os
import struct
from pathlib import Path
from typing import Any, Callable

from sidecar.adapters.siliconflow import SiliconFlowClient, SiliconFlowConfig

RpcHandler = Callable[[dict[str, Any]], dict[str, Any]]

# ── Module-level client (lazily initialised via configure) ────
_client: SiliconFlowClient | None = None


def configure(params: dict[str, Any]) -> dict[str, Any]:
    """Initialise the SiliconFlow client with API key + model config."""
    global _client
    _client = SiliconFlowClient(SiliconFlowConfig(
        api_key=params["api_key"],
        chat_model=params.get("chat_model", "Qwen/Qwen2.5-7B-Instruct"),
        embedding_model=params.get("embedding_model", "BAAI/bge-m3"),
        request_timeout_seconds=int(params.get("request_timeout_seconds", 30)),
        max_retries=int(params.get("max_retries", 2)),
        max_excerpt_chars=int(params.get("max_excerpt_chars", 1200)),
    ))
    return {"configured": True}


def _get_client() -> SiliconFlowClient:
    if _client is None:
        raise RuntimeError("Sidecar not configured — call 'configure' first")
    return _client


# ── parse_item ────────────────────────────────────────────────
_TEXT_EXTS = {".txt", ".md", ".csv", ".json", ".xml", ".yml", ".yaml",
              ".toml", ".ini", ".cfg", ".log", ".py", ".rs", ".ts",
              ".tsx", ".js", ".jsx", ".html", ".css", ".sql", ".sh",
              ".bat", ".ps1", ".c", ".cpp", ".h", ".java", ".go"}


def _read_text_excerpt(path: Path, max_chars: int = 1200) -> str | None:
    """Try reading first N chars of text-like files."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read(max_chars)
    except Exception:
        return None


def _folder_sample(path: Path, depth: int = 1, max_items: int = 30) -> list[str]:
    """Shallow listing of a directory's contents."""
    items: list[str] = []
    try:
        for entry in sorted(path.iterdir()):
            rel = entry.name + ("/" if entry.is_dir() else "")
            items.append(rel)
            if len(items) >= max_items:
                break
    except PermissionError:
        pass
    return items


def parse_item(params: dict[str, Any]) -> dict[str, Any]:
    """Parse a file/folder and extract metadata + content excerpt."""
    source = Path(str(params["source_path"]))
    is_dir = source.is_dir()

    stat = source.stat() if source.exists() else None

    content_excerpt: str | None = None
    folder_sample: list[str] | None = None

    if is_dir:
        folder_sample = _folder_sample(source)
    elif source.suffix.lower() in _TEXT_EXTS:
        content_excerpt = _read_text_excerpt(source)

    mime, _ = mimetypes.guess_type(str(source))

    return {
        "item_type": "folder" if is_dir else "file",
        "name": source.name,
        "extension": source.suffix.lower() or None,
        "mime_type": mime,
        "size_bytes": stat.st_size if stat and not is_dir else 0,
        "created_at": stat.st_ctime if stat else None,
        "modified_at": stat.st_mtime if stat else None,
        "content_excerpt": content_excerpt,
        "folder_sample": folder_sample,
    }


# ── build_features ────────────────────────────────────────────
def build_features(params: dict[str, Any]) -> dict[str, Any]:
    """Build a feature text string for the LLM from parsed item data."""
    item = params["parsed_item"]
    parts: list[str] = []

    name = item.get("name", "")
    parts.append(f"文件名: {name}")

    ext = item.get("extension")
    if ext:
        parts.append(f"扩展名: {ext}")

    mime = item.get("mime_type")
    if mime:
        parts.append(f"MIME: {mime}")

    size = item.get("size_bytes", 0)
    if size:
        if size < 1024:
            parts.append(f"大小: {size}B")
        elif size < 1024 * 1024:
            parts.append(f"大小: {size // 1024}KB")
        else:
            parts.append(f"大小: {size // (1024 * 1024)}MB")

    excerpt = item.get("content_excerpt")
    if excerpt:
        parts.append(f"内容摘要:\n{excerpt[:800]}")

    folder_items = item.get("folder_sample")
    if folder_items:
        listing = "\n".join(f"  {f}" for f in folder_items[:20])
        parts.append(f"文件夹内容:\n{listing}")

    feature_text = "\n".join(parts)
    summary = f"{name} ({ext or '文件夹'})" if ext else f"{name} (文件夹)"
    return {
        "feature_text": feature_text,
        "content_summary": summary,
    }


# ── classify_item ─────────────────────────────────────────────
_CLASSIFY_SYSTEM = """你是一个文件归档助手。请把用户提供的文件归到合适的**分类路径**。

# 已有顶级分类（强烈优先复用）
{existing_top_level}

# 当前整个目录结构（最多 3 层，你应该把文件归到与现有分支语义最贴合的位置）
{directory_tree}

# 顶级分类新建规则
{top_level_rule}

# 已用过的子分类（优先复用，避免语义重复 —— 例如不要同时建"项目A"和"projectA"）
{known_subcategories}

# 分类规则

1. **必须至少输出二级路径**（顶级/子级）。顶级分类太宽，不利于以后查找。
   - ✓ "学习/4级英语"、"工作/Filess项目"、"开发/Rust"、"财务/发票/2024"
   - ✗ "学习"、"工作"（仅顶级，禁止）

2. **新子分类命名要求**（只在已有子分类都不合适时才新建）：
   - 简短（2~6 个汉字）、语义通用
   - 不要加时间/版本后缀（除非本来就是时序归档，如 "发票/2024"）
   - 不要用具体文件名作分类名

3. 层级控制在 2-3 级为宜，避免过度嵌套。

4. 如果信息不足无法判断，返回 `category_id: "unclassified"`、`confidence: 0`。

# 命名启发（很重要，帮你判断"用途场景"而不只是"内容主题"）

- 文件名包含**人名前缀/后缀**（如 "张三_报告.docx"、"李雷-提案.pdf"、"王芳的简历.doc"）→ 几乎必然是**工作场景的他人交付物**，应归 "工作/" 而不是按内容主题归（比如 "数学分析报告" 不是数学学习资料，是工作分析报告）
- 文件名是**教材/章节/讲次格式**（如 "高数第3章.pdf"、"4级第8讲.pdf"、"第二节练习.docx"）→ 学习资料，归 "学习/"
- 文件名包含**项目代号 + 日期/版本**（如 "ProjectX-v2.pdf"、"OKR_Q3.xlsx"）→ 工作
- 文件名是**发票/合同/账单/对账**字样 → 财务
- 文件名为**截图/IMG/DSC/VID 等设备命名** → 媒体
- 文件名为**code/类名/源码扩展名**（.py/.rs/.ts 等）→ 开发

判断顺序: **先看命名特征 → 再看内容主题**。命名特征往往比内容更准确反映用途。

# 输出格式（严格 JSON，不要额外文字）
{{
  "category_id": "顶级/子级[/子子级]",
  "suggested_name": "建议文件名（保留原扩展名）",
  "confidence": 0.0到1.0,
  "reason": "一句话分类理由（说明你看到了哪个关键信号）"
}}
"""

_CLASSIFY_USER = """请对以下文件进行分类：

{feature_text}"""

_CLASSIFY_HINT_BLOCK = """

# 用户提示（**最高优先级**，必须遵从）
{hint}
"""


def classify_item(params: dict[str, Any]) -> dict[str, Any]:
    """Call the LLM to classify a file based on its features."""
    client = _get_client()
    feature_text = params["feature_text"]
    # existing_top_level wins when provided; falls back to the seed `categories`
    # list for very first run when no items exist yet.
    existing_top: list[str] = params.get("existing_top_level") or []
    seed_categories = params.get("categories", ["财务", "工作", "生活", "学习", "媒体", "开发"])
    if not existing_top:
        existing_top = list(seed_categories)

    directory_tree: str = (params.get("directory_tree") or "").strip()
    can_create: bool = bool(params.get("can_create_top_level", True))
    max_top: int = int(params.get("max_top_level", 30))
    known_subs: list[str] = params.get("known_subcategories") or []
    hint: str = (params.get("hint") or "").strip()

    existing_top_str = "\n".join(f"- {c}" for c in existing_top) or "(暂无)"
    tree_str = directory_tree if directory_tree else "(暂无，目录为空)"
    if known_subs:
        known_str = "\n".join(f"  - {c}" for c in known_subs[:40])
    else:
        known_str = "  (暂无，可以新建)"

    if can_create:
        top_level_rule = (
            "如果\"已有顶级\"列表中都没有合适的分类，你**可以**新建一个新的顶级分类。\n"
            "新建时要求：2~4 个汉字、语义通用、避免与已有顶级重复（如 \"工作\" 和 \"职场\"）。"
        )
    else:
        top_level_rule = (
            f"**禁止新建顶级分类**（当前已达用户设置的上限 {max_top} 个）。\n"
            "必须从\"已有顶级\"列表中选一个；若所有已有顶级都确实不合适，返回 "
            "`category_id: \"unclassified\"`。"
        )

    system = _CLASSIFY_SYSTEM.format(
        existing_top_level=existing_top_str,
        directory_tree=tree_str,
        top_level_rule=top_level_rule,
        known_subcategories=known_str,
    )
    user = _CLASSIFY_USER.format(feature_text=feature_text)
    if hint:
        user += _CLASSIFY_HINT_BLOCK.format(hint=hint)

    # temperature=0 → greedy decoding; same input always produces the same
    # category, so users get reproducible classification across runs.
    raw = client.chat(system, user, temperature=0.0)

    try:
        result = json.loads(raw)
    except json.JSONDecodeError:
        return {
            "category_id": "unclassified",
            "suggested_name": None,
            "confidence": 0.0,
            "reason": f"LLM 返回了非法 JSON: {raw[:200]}",
            "need_human_review": True,
        }

    confidence = float(result.get("confidence", 0))
    threshold = float(params.get("low_confidence_threshold", 0.8))

    return {
        "category_id": result.get("category_id", "unclassified"),
        "suggested_name": result.get("suggested_name"),
        "confidence": confidence,
        "reason": result.get("reason", ""),
        "need_human_review": confidence < threshold,
    }


# ── retrieve_context (placeholder for RAG) ────────────────────
def retrieve_context(params: dict[str, Any]) -> dict[str, Any]:
    """Retrieve similar items from vector store. Placeholder for now."""
    _ = params
    return {"matches": []}


# ── embed_sample (legacy placeholder) ─────────────────────────
def embed_sample(params: dict[str, Any]) -> dict[str, Any]:
    """Generate embedding for a text and return placeholder vector ID."""
    return {"vector_id": str(params.get("sample_id", "none"))}


# ── embed_text ────────────────────────────────────────────────
# Truncate input to keep embedding requests within model context.
# bge-m3 supports up to 8192 tokens; we conservatively cap by characters
# to avoid worst-case overruns on CJK text.
_EMBED_MAX_CHARS = 6000


def embed_text(params: dict[str, Any]) -> dict[str, Any]:
    """Compute an embedding vector for arbitrary text via SiliconFlow.

    Returns ``{"embedding": [float, ...], "model": str, "dim": int}``.
    Input is truncated to a safe character budget; empty input returns an
    empty vector so callers can detect and skip storage.
    """
    text = (params.get("text") or "").strip()
    if not text:
        return {"embedding": [], "model": "", "dim": 0}

    if len(text) > _EMBED_MAX_CHARS:
        text = text[:_EMBED_MAX_CHARS]

    client = _get_client()
    vector = client.embed(text)
    return {
        "embedding": vector,
        "model": client._cfg.embedding_model,  # noqa: SLF001 — internal field, intentional
        "dim": len(vector),
    }


# ── write_rag_sample ──────────────────────────────────────────
def write_rag_sample(params: dict[str, Any]) -> dict[str, Any]:
    _ = params
    return {"written": True}


# ── health_check ──────────────────────────────────────────────
def health_check(params: dict[str, Any]) -> dict[str, Any]:
    """Check if the sidecar is alive and configured."""
    _ = params
    configured = _client is not None
    return {
        "status": "ready" if configured else "unconfigured",
        "configured": configured,
    }


# ── Handler registry ─────────────────────────────────────────
HANDLERS: dict[str, RpcHandler] = {
    "configure": configure,
    "health_check": health_check,
    "parse_item": parse_item,
    "build_features": build_features,
    "retrieve_context": retrieve_context,
    "classify_item": classify_item,
    "embed_sample": embed_sample,
    "embed_text": embed_text,
    "write_rag_sample": write_rag_sample,
}
