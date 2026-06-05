from __future__ import annotations

import json
import sys
from typing import Any

from sidecar.rpc.dispatcher import HANDLERS


def _success(request_id: str, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _error(request_id: str | None, code: str, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message, "details": {}},
    }


def handle_line(line: str) -> dict[str, Any]:
    try:
        payload = json.loads(line)
    except json.JSONDecodeError:
        return _error(None, "invalid_input", "invalid json payload")

    request_id = payload.get("id")
    method = payload.get("method")
    params = payload.get("params", {})

    if method not in HANDLERS:
        return _error(request_id, "not_found", f"unknown method: {method}")

    try:
        return _success(str(request_id), HANDLERS[method](params))
    except Exception as exc:  # pragma: no cover - scaffold fallback
        return _error(str(request_id), "sidecar_failed", str(exc))


def serve() -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        response = handle_line(line)
        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(serve())
