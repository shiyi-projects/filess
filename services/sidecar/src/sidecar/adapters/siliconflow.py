"""SiliconFlow API client — stdlib only, no external dependencies."""
from __future__ import annotations

import json
import urllib.request
import urllib.error
from dataclasses import dataclass, field
from typing import Any


@dataclass(slots=True)
class SiliconFlowConfig:
    api_key: str
    api_base_url: str = "https://api.siliconflow.cn/v1"
    chat_model: str = "Qwen/Qwen2.5-7B-Instruct"
    embedding_model: str = "BAAI/bge-m3"
    request_timeout_seconds: int = 30
    max_retries: int = 2
    max_excerpt_chars: int = 1200


class SiliconFlowClient:
    """Minimal OpenAI-compatible client for SiliconFlow cloud API."""

    def __init__(self, config: SiliconFlowConfig) -> None:
        self._cfg = config

    # ── Chat Completion ──────────────────────────────────────
    def chat(
        self,
        system_prompt: str,
        user_prompt: str,
        temperature: float = 0.0,
        max_tokens: int = 1024,
    ) -> str:
        """Send a chat completion request, return the assistant message text."""
        body = {
            "model": self._cfg.chat_model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt},
            ],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "response_format": {"type": "json_object"},
        }
        data = self._post("/chat/completions", body)
        return data["choices"][0]["message"]["content"]

    # ── Embedding ────────────────────────────────────────────
    def embed(self, text: str) -> list[float]:
        """Get embedding vector for a text string."""
        body = {
            "model": self._cfg.embedding_model,
            "input": text,
            "encoding_format": "float",
        }
        data = self._post("/embeddings", body)
        return data["data"][0]["embedding"]

    # ── Internal ─────────────────────────────────────────────
    def _post(self, path: str, body: dict[str, Any]) -> dict[str, Any]:
        url = self._cfg.api_base_url.rstrip("/") + path
        payload = json.dumps(body, ensure_ascii=False).encode("utf-8")
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self._cfg.api_key}",
        }

        last_err: Exception | None = None
        for _ in range(self._cfg.max_retries + 1):
            try:
                req = urllib.request.Request(url, data=payload, headers=headers, method="POST")
                with urllib.request.urlopen(req, timeout=self._cfg.request_timeout_seconds) as resp:
                    return json.loads(resp.read().decode("utf-8"))
            except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as exc:
                last_err = exc
                continue

        raise RuntimeError(f"SiliconFlow API failed after retries: {last_err}")
