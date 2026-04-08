"""Rust language module for codegen.

Provides Rust-specific type mapping, identifier handling, and Jinja2 filter registration.
"""

from __future__ import annotations

import re

from jinja2 import Environment


RUST_TYPE_MAP: dict[str, str] = {
    "integer": "i64",
    "string": "String",
    "boolean": "bool",
    "number": "f64",
    "datetime": "chrono::DateTime<chrono::Utc>",
    "file": "std::path::PathBuf",
    "object": "serde_json::Value",
    "void": "()",
}

RUST_KEYWORDS = {
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
    # Reserved for future use
    "abstract", "become", "box", "do", "final", "macro", "override",
    "priv", "try", "typeof", "unsized", "virtual",
}


def _rust_safe_ident(name: str) -> str:
    """Make a Rust-safe identifier from a field/param name."""
    safe = re.sub(r'[^a-zA-Z0-9_]', '_', name)
    if safe and safe[0].isdigit():
        safe = '_' + safe
    safe = re.sub(r'_+', '_', safe).strip('_')
    if not safe:
        safe = '_unnamed'
    if safe in RUST_KEYWORDS:
        safe = f'r#{safe}'
    return safe


def _rust_pascal_case(name: str) -> str:
    """Convert snake_case to PascalCase."""
    return "".join(word.capitalize() for word in name.split("_"))


def _resolve_rust_type(ir_type: str, items: dict | None, model_names: set[str]) -> str:
    """Resolve an IR type to a Rust type string."""
    if ir_type == "array":
        if items:
            inner_ir = items.get("type", "string")
            if inner_ir == "array":
                inner = "serde_json::Value"
            elif inner_ir in model_names:
                inner = inner_ir
            else:
                inner = RUST_TYPE_MAP.get(inner_ir)
                if not inner:
                    inner = "serde_json::Value"
        else:
            inner = "serde_json::Value"
        return f"Vec<{inner}>"

    if ir_type in model_names:
        return ir_type

    mapped = RUST_TYPE_MAP.get(ir_type)
    if mapped:
        return mapped

    # Unknown type -> serde_json::Value
    return "serde_json::Value"


def _needs_serde_rename(field_name: str) -> bool:
    """Check if a field needs a serde rename attribute."""
    safe = _rust_safe_ident(field_name)
    if safe.startswith("r#"):
        return True
    return safe != field_name


def _serde_rename_attr(field_name: str) -> str:
    """Generate serde rename attribute if needed."""
    if _needs_serde_rename(field_name):
        return f'#[serde(rename = "{field_name}")]'
    return ""


def _make_map_type_filter(model_names: set[str]):
    def map_type(ir_type: str) -> str:
        mapped = RUST_TYPE_MAP.get(ir_type)
        if mapped:
            return mapped
        if ir_type == "UploadToken":
            return "UploadToken"
        if ir_type in model_names:
            return ir_type
        return ir_type
    return map_type


def _make_rust_field_type_filter(model_names: set[str]):
    def filter_fn(field: dict) -> str:
        return _resolve_rust_type(field["type"], field.get("items"), model_names)
    return filter_fn


def _make_rust_param_type_filter(model_names: set[str]):
    def filter_fn(param: dict) -> str:
        return _resolve_rust_type(param["type"], param.get("items"), model_names)
    return filter_fn


def register_filters(env: Environment, model_names: set[str]) -> None:
    """Register all Rust-specific Jinja2 filters and tests."""
    env.filters["map_type"] = _make_map_type_filter(model_names)
    env.filters["safe_ident"] = _rust_safe_ident
    env.filters["pascal_case"] = _rust_pascal_case
    env.filters["rust_field_type"] = _make_rust_field_type_filter(model_names)
    env.filters["rust_param_type"] = _make_rust_param_type_filter(model_names)
    env.filters["serde_rename"] = _serde_rename_attr
    env.tests["needs_rename"] = _needs_serde_rename
