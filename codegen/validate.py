"""
Validate api-spec.json structural integrity.

Usage: python v2/codegen/validate.py [path/to/api-spec.json]
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


V2_DIR = Path(__file__).resolve().parent
DEFAULT_IR_PATH = V2_DIR / "api-spec.json"

VALID_METHODS = {"GET", "POST", "PUT", "DELETE", "PATCH"}
VALID_PARAM_TYPES = {"string", "integer", "boolean", "number", "array", "object", "datetime", "file"}
VALID_FIELD_TYPES = {"string", "integer", "boolean", "number", "float", "datetime", "object", "array", "void", "file"}


class ValidationResult:
    def __init__(self) -> None:
        self.errors: list[str] = []
        self.warnings: list[str] = []

    def error(self, msg: str) -> None:
        self.errors.append(msg)

    def warning(self, msg: str) -> None:
        self.warnings.append(msg)


def validate(ir: dict, result: ValidationResult) -> None:
    if "endpoints" not in ir:
        result.error("Missing top-level 'endpoints' key")
        return
    if "models" not in ir:
        result.error("Missing top-level 'models' key")
        return

    model_names = set(ir["models"].keys())

    # --- endpoints ---
    for ep_name, ep in ir["endpoints"].items():
        prefix = f"endpoints.{ep_name}"

        for field in ("method", "path", "path_params", "response", "group"):
            if field not in ep:
                result.error(f"{prefix}: missing required field '{field}'")

        if "method" in ep and ep["method"] not in VALID_METHODS:
            result.error(f"{prefix}: invalid method '{ep['method']}'")

        if "path" in ep and "path_params" in ep:
            path_params_in_path = set(re.findall(r":(\w+)", ep["path"]))
            declared_params = set(ep["path_params"])
            if path_params_in_path != declared_params:
                result.error(
                    f"{prefix}: path_params mismatch — "
                    f"path has {path_params_in_path}, declared {declared_params}"
                )

        if "response" in ep:
            resp = ep["response"]
            if "type" not in resp:
                result.error(f"{prefix}.response: missing 'type'")
            elif resp["type"] != "void" and resp["type"] not in model_names:
                result.warning(f"{prefix}.response: type '{resp['type']}' not found in models")
            if "paginated" not in resp:
                result.error(f"{prefix}.response: missing 'paginated'")

        for param_section in ("query_params", "form_params"):
            params = ep.get(param_section)
            if params is None:
                continue
            for pname, pdef in params.items():
                ppfx = f"{prefix}.{param_section}.{pname}"
                if "type" not in pdef:
                    result.error(f"{ppfx}: missing 'type'")
                    continue
                ptype: str = pdef["type"]
                if ptype not in VALID_PARAM_TYPES and ptype not in model_names:
                    result.warning(f"{ppfx}: type '{ptype}' is neither a valid param type nor a known model")
                if ptype == "array" and "items" not in pdef:
                    result.error(f"{ppfx}: array type missing 'items'")

    # --- models ---
    for model_name, model_def in ir["models"].items():
        prefix = f"models.{model_name}"

        if "fields" not in model_def:
            result.error(f"{prefix}: missing 'fields'")
            continue

        for fname, fdef in model_def["fields"].items():
            fpfx = f"{prefix}.{fname}"
            if "type" not in fdef:
                result.error(f"{fpfx}: missing 'type'")
                continue
            ftype: str = fdef["type"]
            if ftype not in VALID_FIELD_TYPES and ftype not in model_names:
                result.warning(f"{fpfx}: type '{ftype}' is neither a valid type nor a known model")
            if ftype == "array" and "items" not in fdef:
                result.error(f"{fpfx}: array type missing 'items'")


def main() -> None:
    ir_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_IR_PATH

    if not ir_path.exists():
        print(f"Error: {ir_path} not found.", file=sys.stderr)
        sys.exit(1)

    ir = json.loads(ir_path.read_text())
    result = ValidationResult()

    validate(ir, result)

    for error in result.errors:
        print(f"ERROR: {error}")
    for warning in result.warnings:
        print(f"WARNING: {warning}")

    n_endpoints = len(ir.get("endpoints", {}))
    n_models = len(ir.get("models", {}))
    print(f"\nValidated: {n_endpoints} endpoints, {n_models} models. "
          f"{len(result.errors)} errors, {len(result.warnings)} warnings.")

    sys.exit(1 if result.errors else 0)


if __name__ == "__main__":
    main()
