"""
Generate api-spec.json (the canonical IR) from Canvas Swagger 1.2 schemas.

Full extraction: no priority filter, processes all schema files.
Pagination annotations loaded from overrides/pagination.json.

Usage: python v2/codegen/gen_ir.py
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import TypedDict, NotRequired, cast


# ---------------------------------------------------------------------------
# IR type definitions
# ---------------------------------------------------------------------------


class IRParamDef(TypedDict):
    type: str
    items: NotRequired[IRItemsDef]
    enum: NotRequired[list[str]]
    required: NotRequired[bool]
    description: NotRequired[str]


class IRItemsDef(TypedDict):
    type: str


class IRResponse(TypedDict):
    type: str
    paginated: bool


class IREndpoint(TypedDict):
    method: str
    path: str
    path_params: list[str]
    response: IRResponse
    group: str
    description: NotRequired[str]
    query_params: NotRequired[dict[str, IRParamDef]]
    form_params: NotRequired[dict[str, IRParamDef]]
    upload: NotRequired[bool]


class IRFieldDef(TypedDict):
    type: str
    items: NotRequired[IRItemsDef]
    nullable: NotRequired[bool]
    description: NotRequired[str]


class IRModel(TypedDict):
    fields: dict[str, IRFieldDef]


class IRSpec(TypedDict):
    endpoints: dict[str, IREndpoint]
    models: dict[str, IRModel]


# ---------------------------------------------------------------------------
# Swagger 1.2 input types (partial)
# ---------------------------------------------------------------------------


class SwaggerParam(TypedDict, total=False):
    paramType: str
    name: str
    type: str
    items: dict[str, str]
    enum: list[str]
    required: bool
    description: str


class SwaggerOperation(TypedDict, total=False):
    method: str
    nickname: str
    summary: str
    parameters: list[SwaggerParam]
    type: str
    items: dict[str, str]


class SwaggerApi(TypedDict, total=False):
    path: str
    operations: list[SwaggerOperation]


SwaggerProperty = TypedDict('SwaggerProperty', {
    'type': str,
    '$ref': str,
    'items': dict[str, str],
    'description': str,
}, total=False)


class SwaggerModel(TypedDict, total=False):
    properties: dict[str, SwaggerProperty]


class SwaggerSchema(TypedDict, total=False):
    basePath: str
    apis: list[SwaggerApi]
    models: dict[str, SwaggerModel]


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

V2_DIR = Path(__file__).resolve().parent
SCHEMAS_DIR = V2_DIR / "schemas"
PAGINATION_PATH = V2_DIR / "overrides" / "pagination.json"
TYPE_FIXES_PATH = V2_DIR / "overrides" / "type_fixes.json"
FILE_UPLOAD_PATH = V2_DIR / "overrides" / "file_upload.json"
OUTPUT_PATH = V2_DIR / "api-spec.json"

# Swagger type -> IR type mapping
_TYPE_MAP: dict[str, str] = {
    "integer": "integer",
    "string": "string",
    "boolean": "boolean",
    "number": "number",
    "float": "number",
    "double": "number",
    "datetime": "datetime",
    "DateTime": "datetime",
    "date": "datetime",
    "Date": "datetime",
    "file": "file",
    "File": "file",
    "object": "object",
    "Hash": "object",
    "json": "object",
    "JSON": "object",
    "void": "void",
    "array": "array",
    "Array": "array",
}

_MAX_INLINE_DESC_LEN = 200


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _convert_path(swagger_path: str) -> str:
    ir_path = re.sub(r"\{(\w+)\}", r":\1", swagger_path)
    if not ir_path.startswith("/api"):
        ir_path = "/api" + ir_path
    return ir_path


def _extract_path_params(swagger_path: str) -> list[str]:
    return re.findall(r"\{(\w+)\}", swagger_path)


def _normalize_param_name(name: str) -> str:
    return re.sub(r"\[(\w+)\]", r".\1", name).rstrip("[]")


def _map_type(swagger_type: str | None) -> str:
    if not swagger_type:
        return "string"
    mapped = _TYPE_MAP.get(swagger_type)
    if mapped:
        return mapped
    # Normalize common non-standard Canvas types
    lower = swagger_type.lower().strip()
    if lower in ("object", "serializedhash", "hash"):
        return "object"
    if lower in ("datetime", "date"):
        return "datetime"
    if lower in ("deprecated", "url", "uuid", "numeric"):
        return "string"
    if lower.startswith("positive"):
        return "integer"
    if lower.startswith("[") or lower.endswith("[]"):
        return "array"
    # Model references or complex types with spaces/braces -> object
    if " " in swagger_type or "{" in swagger_type:
        return "object"
    return swagger_type


def _sanitize_ref(ref: str) -> str:
    """Sanitize a $ref value from Swagger model properties.

    Handles known malformed patterns:
    - Trailing comma: "ModuleItem," → "ModuleItem"
    - Pipe union: "QuestionItem|StimulusItem" → "object"
    - Namespace separator: "Lti::ResourceLink" → "LtiResourceLink"
    """
    ref = ref.rstrip(",")
    if "|" in ref:
        return "object"
    ref = ref.replace("::", "")
    return ref


def _extract_items_type(items: dict[str, str]) -> IRItemsDef:
    raw = items.get("type") or items.get("$ref") or "string"
    return IRItemsDef(type=_map_type(raw))


# ---------------------------------------------------------------------------
# Extraction
# ---------------------------------------------------------------------------


def _extract_param(param: SwaggerParam) -> IRParamDef:
    ptype = _map_type(param.get("type"))
    result = IRParamDef(type=ptype)

    if ptype == "array":
        result["items"] = _extract_items_type(param.get("items", {}))

    enum = param.get("enum")
    if enum:
        result["enum"] = enum

    if param.get("required"):
        result["required"] = True

    desc = param.get("description", "")
    if desc and len(desc) < _MAX_INLINE_DESC_LEN:
        result["description"] = desc

    return result


def _sanitize_type_ref(type_name: str) -> str:
    """Sanitize a type reference for use in generated code."""
    # Types with spaces, braces, pipes etc. -> void (will be serde_json::Value)
    if " " in type_name or "{" in type_name or "|" in type_name:
        return "void"
    # Clean :: and other non-identifier chars
    return re.sub(r'[^a-zA-Z0-9_]', '', type_name) or "void"


def _extract_response(operation: SwaggerOperation) -> IRResponse:
    resp_type = operation.get("type", "void")
    items = operation.get("items", {})

    if resp_type == "array" and items:
        model_ref = items.get("$ref") or items.get("type") or "void"
        return IRResponse(type=_sanitize_type_ref(model_ref), paginated=False)

    if resp_type in ("void", ""):
        return IRResponse(type="void", paginated=False)

    return IRResponse(type=_sanitize_type_ref(resp_type), paginated=False)


def _extract_model(model_def: SwaggerModel) -> IRModel:
    fields: dict[str, IRFieldDef] = {}
    properties = model_def.get("properties") or {}

    for field_name, field_def in properties.items():
        raw_type = field_def.get("type") or field_def.get("$ref")
        if raw_type and not field_def.get("type"):
            raw_type = _sanitize_ref(raw_type)
        field_type = _map_type(raw_type)
        field = IRFieldDef(type=field_type)

        if field_type == "array":
            field["items"] = _extract_items_type(field_def.get("items", {}))

        if field_type == "datetime" and field_name.endswith("_at"):
            field["nullable"] = True

        fields[field_name] = field

    return IRModel(fields=fields)


# ---------------------------------------------------------------------------
# IR builder (full extraction)
# ---------------------------------------------------------------------------


def _load_pagination() -> dict[str, bool]:
    """Load pagination annotations from overrides."""
    if PAGINATION_PATH.exists():
        return json.loads(PAGINATION_PATH.read_text())
    return {}


def _build_ir() -> IRSpec:
    """Build the complete IR from all Canvas schemas (no priority filter)."""
    endpoints: dict[str, IREndpoint] = {}
    models: dict[str, IRModel] = {}

    pagination = _load_pagination()

    for schema_path in sorted(SCHEMAS_DIR.glob("*.json")):
        schema: SwaggerSchema = json.loads(schema_path.read_text())
        group = schema_path.stem

        # --- endpoints ---
        for api in schema.get("apis", []):
            swagger_path = api.get("path", "")
            for operation in api.get("operations", []):
                nickname = operation.get("nickname", "")
                if not nickname:
                    continue

                ir_path = _convert_path(swagger_path)
                method = operation.get("method", "GET")

                query_params: dict[str, IRParamDef] = {}
                form_params: dict[str, IRParamDef] = {}
                for param in operation.get("parameters", []):
                    param_type = param.get("paramType", "")
                    if param_type == "path":
                        continue
                    ir_param = _extract_param(param)
                    param_name = _normalize_param_name(param.get("name", ""))
                    if param_type == "query":
                        query_params[param_name] = ir_param
                    elif param_type == "form":
                        form_params[param_name] = ir_param

                response = _extract_response(operation)
                # Apply pagination from overrides
                response["paginated"] = pagination.get(nickname, False)

                endpoint = IREndpoint(
                    method=method,
                    path=ir_path,
                    path_params=_extract_path_params(swagger_path),
                    response=response,
                    group=group,
                )
                summary = operation.get("summary")
                if summary:
                    endpoint["description"] = summary
                if query_params:
                    endpoint["query_params"] = query_params
                if form_params:
                    endpoint["form_params"] = form_params

                endpoints[nickname] = endpoint

        # --- models ---
        for model_name, model_def in schema.get("models", {}).items():
            # Sanitize model name: Lti::ResourceLink -> LtiResourceLink
            safe_name = re.sub(r'[^a-zA-Z0-9_]', '', model_name)
            # Avoid collision with Rust built-in types
            if safe_name in ("Result", "Error", "Option", "Box", "Vec", "String"):
                safe_name = f"Canvas{safe_name}"
            if safe_name not in models:
                models[safe_name] = _extract_model(model_def)

    # Post-process: map response types referencing unknown models to "void"
    model_names = set(models.keys())
    primitives = {"void", "string", "integer", "boolean", "number", "datetime", "object", "array", "file"}
    for ep in endpoints.values():
        resp_type = ep["response"]["type"]
        if resp_type not in primitives and resp_type not in model_names:
            ep["response"]["type"] = "void"

    return IRSpec(endpoints=endpoints, models=models)


# ---------------------------------------------------------------------------
# Overrides
# ---------------------------------------------------------------------------


type JsonDict = dict[str, "JsonValue"]
type JsonValue = str | int | float | bool | None | list["JsonValue"] | JsonDict


def _deep_merge(base: JsonDict, override: JsonDict) -> None:
    for key, value in override.items():
        existing = base.get(key)
        if isinstance(existing, dict) and isinstance(value, dict):
            _deep_merge(existing, value)
        else:
            base[key] = value


def _apply_overrides(ir: IRSpec) -> IRSpec:
    if not TYPE_FIXES_PATH.exists():
        return ir

    overrides: JsonDict = json.loads(TYPE_FIXES_PATH.read_text())
    if not overrides:
        return ir

    ep_overrides = overrides.get("endpoints")
    if isinstance(ep_overrides, dict):
        for key, override in ep_overrides.items():
            if not isinstance(override, dict):
                continue
            target: JsonDict = cast(JsonDict, ir["endpoints"].get(key, {}))
            _deep_merge(target, override)
            ir["endpoints"][key] = cast(IREndpoint, target)

    model_overrides = overrides.get("models")
    if isinstance(model_overrides, dict):
        for key, override in model_overrides.items():
            if not isinstance(override, dict):
                continue
            target = cast(JsonDict, ir["models"].get(key, {}))
            _deep_merge(target, override)
            ir["models"][key] = cast(IRModel, target)

    return ir


def _apply_file_upload_overrides(ir: IRSpec) -> IRSpec:
    """Mark upload endpoints and add Step 1 form params from file_upload.json."""
    if not FILE_UPLOAD_PATH.exists():
        return ir

    override: dict = json.loads(FILE_UPLOAD_PATH.read_text())
    ep_names: list[str] = override.get("endpoints", [])
    shared_params: dict[str, dict] = override.get("form_params", {})

    for ep_name in ep_names:
        ep = ir["endpoints"].get(ep_name)
        if ep is None:
            continue
        ep["upload"] = True
        # Merge shared form_params into endpoint
        existing_form = ep.get("form_params", {})
        for param_name, param_def in shared_params.items():
            if param_name not in existing_form:
                existing_form[param_name] = cast(IRParamDef, param_def)
        ep["form_params"] = cast(dict[str, IRParamDef], existing_form)
        # Override response type to UploadToken
        ep["response"] = IRResponse(type="UploadToken", paginated=False)

    return ir


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    print("Generating IR from Canvas schemas (full extraction)...")

    ir = _build_ir()
    ir = _apply_overrides(ir)
    ir = _apply_file_upload_overrides(ir)

    OUTPUT_PATH.write_text(json.dumps(ir, indent=2, ensure_ascii=False) + "\n")

    n_endpoints = len(ir["endpoints"])
    n_models = len(ir["models"])
    print(f"Generated {OUTPUT_PATH}: {n_endpoints} endpoints, {n_models} models")


if __name__ == "__main__":
    main()
