# SPDX-License-Identifier: AGPL-3.0-only
"""Built-in Ollama LLM providers."""

from __future__ import annotations

import base64
from dataclasses import dataclass
from math import ceil
from pathlib import Path
from typing import Any, Literal
from urllib.parse import urlparse, urlunparse

from scriptscore.contracts import ErrorCategory, LlmTokenUsage, ScriptscoreError
from scriptscore.providers.constants import PROVIDER_INTERFACE_VERSION
from scriptscore.providers.interfaces import LlmProvider, LlmRequest, LlmResponse

_DEFAULT_TIMEOUT_SECONDS = 30
_OLLAMA_NATIVE_BASE_URL = "http://127.0.0.1:11434"
_OLLAMA_CLOUD_BASE_URL = "https://ollama.com"


def _provider_error(
    *, code: str, message: str, details: dict[str, Any], retryable: bool = False
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=ErrorCategory.PROVIDER,
        retryable=retryable,
        details=details,
    )


def _external_error(
    *,
    code: str,
    message: str,
    details: dict[str, Any],
    retryable: bool,
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=ErrorCategory.EXTERNAL_DEPENDENCY,
        retryable=retryable,
        details=details,
    )


def _base64_image(path: str) -> str:
    return base64.b64encode(Path(path).read_bytes()).decode("ascii")


def _ollama_format(
    response_mode: str, response_contract: dict[str, Any] | None
) -> dict[str, Any] | None:
    if response_mode != "json_schema" or response_contract is None:
        return None
    if response_contract.get("type") != "json_schema":
        return response_contract
    schema = dict(response_contract)
    schema["type"] = "object"
    return schema


def _provider_payload_options(
    execution_options: dict[str, Any],
) -> tuple[dict[str, Any], str | None, bool | None]:
    options = dict(execution_options)
    keep_alive_value = options.pop("keep_alive", None)
    think_value = options.pop("think", None)
    keep_alive = None if keep_alive_value is None else str(keep_alive_value)
    think = None if think_value is None else bool(think_value)
    return options, keep_alive, think


def _response_dict(
    *,
    response: Any,
    provider_name: str,
    base_url: str,
    model: str,
    operation: str,
) -> dict[str, Any]:
    if isinstance(response, dict):
        parsed = response
    elif hasattr(response, "model_dump"):
        parsed = response.model_dump(exclude_none=True)
    elif hasattr(response, "dict"):
        parsed = response.dict(exclude_none=True)
    else:
        raise _external_error(
            code="ollama_response_invalid",
            message=f"Ollama {operation} returned an unsupported response payload.",
            details={
                "provider": provider_name,
                "base_url": base_url,
                "model": model,
                "operation": operation,
                "response_type": type(response).__name__,
            },
            retryable=False,
        )
    if not isinstance(parsed, dict):
        raise _external_error(
            code="ollama_response_invalid",
            message=f"Ollama {operation} returned a non-object payload.",
            details={
                "provider": provider_name,
                "base_url": base_url,
                "model": model,
                "operation": operation,
                "response": parsed,
            },
            retryable=False,
        )
    return parsed


def _client_factory(
    *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
) -> Any:
    try:
        from ollama import Client
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ollama_client_unavailable",
            message="The ollama Python client is not installed.",
            details={"host": host},
            retryable=False,
        ) from exc
    return Client(
        host=host,
        headers=headers,
        timeout=timeout_seconds or _DEFAULT_TIMEOUT_SECONDS,
    )


def _client_show(client: Any, *, model: str) -> Any:
    return client.show(model=model)


def _client_generate(client: Any, **payload: Any) -> Any:
    return client.generate(**payload)


def _token_estimate(value: str) -> int:
    if not value:
        return 0
    return ceil(len(value) / 4)


def _nonnegative_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int) and value >= 0:
        return value
    return None


def _llm_usage_from_generate(
    *, request: LlmRequest, generate_response: dict[str, Any], raw_text: str
) -> LlmTokenUsage:
    input_tokens = _nonnegative_int(generate_response.get("prompt_eval_count"))
    output_tokens = _nonnegative_int(generate_response.get("eval_count"))
    image_input_count = len(request.file_inputs)
    if input_tokens is not None and output_tokens is not None:
        return LlmTokenUsage(
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            total_tokens=input_tokens + output_tokens,
            count_source="exact",
            image_input_count=image_input_count,
        )
    estimated_input_tokens = _token_estimate(request.rendered_text)
    estimated_output_tokens = _token_estimate(raw_text)
    return LlmTokenUsage(
        input_tokens=estimated_input_tokens,
        output_tokens=estimated_output_tokens,
        total_tokens=estimated_input_tokens + estimated_output_tokens,
        count_source="estimated",
        estimation_method="chars_div_4",
        image_input_count=image_input_count,
    )


def _coerce_status_code(exc: Exception) -> int | None:
    for attribute in ("status_code", "status"):
        value = getattr(exc, attribute, None)
        if isinstance(value, int):
            return value
    response = getattr(exc, "response", None)
    status_code = getattr(response, "status_code", None)
    if isinstance(status_code, int):
        return status_code
    return None


def _coerce_error_message(exc: Exception) -> str:
    for attribute in ("error", "message", "detail"):
        value = getattr(exc, attribute, None)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return str(exc).strip()


def _raise_ollama_error(
    *,
    exc: Exception,
    provider_name: str,
    base_url: str,
    model: str,
    operation: str,
) -> None:
    status_code = _coerce_status_code(exc)
    message_text = _coerce_error_message(exc)
    lowered = message_text.lower()
    details = {
        "provider": provider_name,
        "base_url": base_url,
        "model": model,
        "operation": operation,
        "error_type": type(exc).__name__,
        "error": message_text,
    }
    if provider_name == "ollama_cloud" and status_code in {401, 403}:
        raise _external_error(
            code="ollama_auth_failed",
            message="Ollama Cloud rejected the supplied bearer token.",
            details={**details, "status_code": status_code},
            retryable=False,
        ) from exc
    if status_code == 404 and "model" in lowered:
        raise _provider_error(
            code="llm_model_unavailable",
            message=f"Ollama model '{model}' is unavailable.",
            details={**details, "status_code": status_code},
        ) from exc
    if "not found" in lowered and "model" in lowered:
        raise _provider_error(
            code="llm_model_unavailable",
            message=f"Ollama model '{model}' is unavailable.",
            details={**details, "status_code": status_code},
        ) from exc
    if "vision" in lowered and ("support" in lowered or "capab" in lowered):
        raise _provider_error(
            code="llm_model_lacks_vision",
            message=f"Ollama model '{model}' does not support image inputs.",
            details={**details, "status_code": status_code},
        ) from exc
    if status_code is not None:
        raise _external_error(
            code="ollama_http_failed",
            message=f"Ollama {operation} failed with HTTP {status_code}.",
            details={**details, "status_code": status_code},
            retryable=500 <= status_code < 600,
        ) from exc
    error_type = type(exc).__name__.lower()
    if any(token in error_type for token in ("timeout", "connect", "network", "transport")):
        raise _external_error(
            code="ollama_unreachable",
            message="Ollama is unreachable.",
            details=details,
            retryable=True,
        ) from exc
    raise _external_error(
        code="ollama_request_failed",
        message=f"Ollama {operation} failed.",
        details=details,
        retryable=False,
    ) from exc


def _resolve_host(*, provider_name: str, base_url: str | None) -> str:
    if base_url is not None:
        return _normalize_client_host(base_url)
    if provider_name == "ollama_cloud":
        return _OLLAMA_CLOUD_BASE_URL
    return _OLLAMA_NATIVE_BASE_URL


def _normalize_client_host(base_url: str) -> str:
    """Accept documented Ollama API roots while passing host roots to the Python client."""

    parsed = urlparse(base_url)
    path = parsed.path.rstrip("/")
    if path == "/api":
        return urlunparse(parsed._replace(path="", params="", query="", fragment="")).rstrip("/")
    return base_url


def _build_client(*, provider_name: str, request: LlmRequest) -> tuple[Any, str]:
    host = _resolve_host(provider_name=provider_name, base_url=request.provider_config.base_url)
    headers: dict[str, str] | None = None
    if provider_name == "ollama_cloud":
        headers = {"Authorization": f"Bearer {request.provider_config.api_key}"}
    client = _client_factory(
        host=host,
        headers=headers,
        timeout_seconds=request.provider_config.timeout_seconds,
    )
    return client, host


@dataclass(frozen=True)
class _BaseOllamaProvider(LlmProvider):
    provider_name: str
    interface_version: str = PROVIDER_INTERFACE_VERSION

    @property
    def capability(self) -> Literal["llm_provider"]:
        return "llm_provider"

    def generate(self, request: LlmRequest) -> LlmResponse:
        client, host = _build_client(provider_name=self.provider_name, request=request)
        show_response: dict[str, Any] | None = None
        if request.file_inputs:
            show_response = self._ensure_vision_capable(request=request, client=client, host=host)

        payload = self._generate_payload(request)
        try:
            raw_generate = _client_generate(client, **payload)
        except (
            Exception
        ) as exc:  # pragma: no cover - concrete library exception types are environment-specific.
            _raise_ollama_error(
                exc=exc,
                provider_name=self.provider_name,
                base_url=host,
                model=request.provider_config.model,
                operation="generate",
            )
        generate_response = _response_dict(
            response=raw_generate,
            provider_name=self.provider_name,
            base_url=host,
            model=request.provider_config.model,
            operation="generate",
        )
        raw_text = generate_response.get("response")
        if not isinstance(raw_text, str):
            raise _external_error(
                code="ollama_response_invalid",
                message="Ollama generate response did not include string field 'response'.",
                details={
                    "provider": self.provider_name,
                    "base_url": host,
                    "model": request.provider_config.model,
                    "response": generate_response,
                },
                retryable=False,
            )
        return LlmResponse(
            raw_text=raw_text,
            provider_response={
                "model": request.provider_config.model,
                "host": host,
                "generate": generate_response,
                "show": show_response,
            },
            usage=_llm_usage_from_generate(
                request=request,
                generate_response=generate_response,
                raw_text=raw_text,
            ),
        )

    def _ensure_vision_capable(
        self, *, request: LlmRequest, client: Any, host: str
    ) -> dict[str, Any]:
        try:
            raw_show = _client_show(client, model=request.provider_config.model)
        except (
            Exception
        ) as exc:  # pragma: no cover - concrete library exception types are environment-specific.
            _raise_ollama_error(
                exc=exc,
                provider_name=self.provider_name,
                base_url=host,
                model=request.provider_config.model,
                operation="show",
            )
        show_response = _response_dict(
            response=raw_show,
            provider_name=self.provider_name,
            base_url=host,
            model=request.provider_config.model,
            operation="show",
        )
        capabilities = show_response.get("capabilities")
        if not isinstance(capabilities, list):
            raise _provider_error(
                code="llm_model_capabilities_unknown",
                message=(
                    f"Ollama model '{request.provider_config.model}' did not expose capabilities needed "
                    "to confirm image support."
                ),
                details={
                    "provider": self.provider_name,
                    "base_url": host,
                    "model": request.provider_config.model,
                    "show_response": show_response,
                },
            )
        normalized = {str(capability).lower() for capability in capabilities}
        if "vision" not in normalized:
            raise _provider_error(
                code="llm_model_lacks_vision",
                message=f"Ollama model '{request.provider_config.model}' does not support image inputs.",
                details={
                    "provider": self.provider_name,
                    "base_url": host,
                    "model": request.provider_config.model,
                    "capabilities": sorted(normalized),
                },
            )
        return show_response

    def _generate_payload(self, request: LlmRequest) -> dict[str, Any]:
        provider_options, prompt_keep_alive, prompt_think = _provider_payload_options(
            request.execution_options
        )
        payload: dict[str, Any] = {
            "model": request.provider_config.model,
            "prompt": request.rendered_text,
            "stream": False,
            "options": provider_options,
        }
        if request.file_inputs:
            payload["images"] = [_base64_image(path) for path in request.file_inputs.values()]
        keep_alive = request.provider_config.keep_alive or prompt_keep_alive
        if keep_alive is not None:
            payload["keep_alive"] = keep_alive
        if prompt_think is not None:
            payload["think"] = prompt_think
        response_format = _ollama_format(request.response_mode, request.response_contract)
        if response_format is not None:
            payload["format"] = response_format
        return payload


@dataclass(frozen=True)
class OllamaNativeProvider(_BaseOllamaProvider):
    """Local Ollama provider shipped in public scriptscore."""

    provider_name: str = "ollama_native"


@dataclass(frozen=True)
class OllamaCloudProvider(_BaseOllamaProvider):
    """Remote Ollama Cloud provider shipped in public scriptscore."""

    provider_name: str = "ollama_cloud"
