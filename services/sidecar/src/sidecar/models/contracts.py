from dataclasses import dataclass
from typing import Any


@dataclass(slots=True)
class RpcRequest:
    request_id: str
    method: str
    params: dict[str, Any]


@dataclass(slots=True)
class RpcError:
    code: str
    message: str
    details: dict[str, Any] | None = None


@dataclass(slots=True)
class RpcResponse:
    request_id: str
    result: dict[str, Any] | None = None
    error: RpcError | None = None

