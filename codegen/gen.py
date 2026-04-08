"""
Unified codegen driver: IR -> language-specific code via Jinja2 templates.

Usage:
    python v2/codegen/gen.py rust --out v2/cool-api/src/generated/
    python v2/codegen/gen.py rust --ir v2/codegen/api-spec.json --out v2/cool-api/src/generated/
"""

from __future__ import annotations

import argparse
import importlib
import json
import sys
from pathlib import Path

from jinja2 import Environment, FileSystemLoader

V2_DIR = Path(__file__).resolve().parent


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def _load_language(lang: str):
    """Load a language module from codegen/languages/."""
    try:
        return importlib.import_module(f"languages.{lang}")
    except ModuleNotFoundError:
        print(f"Error: unsupported language '{lang}'. No module 'languages/{lang}.py' found.", file=sys.stderr)
        sys.exit(1)


def _setup_env(lang: str) -> Environment:
    templates_dir = V2_DIR / "templates" / lang
    if not templates_dir.exists():
        print(f"Error: no templates directory for language '{lang}' at {templates_dir}", file=sys.stderr)
        sys.exit(1)

    env = Environment(
        loader=FileSystemLoader(str(templates_dir)),
        keep_trailing_newline=True,
        trim_blocks=True,
        lstrip_blocks=True,
    )

    return env


def _load_ir(ir_path: Path) -> dict:
    return json.loads(ir_path.read_text())


def _generate(lang: str, ir: dict, out_dir: Path) -> None:
    from validate import validate, ValidationResult

    result = ValidationResult()
    validate(ir, result)
    for w in result.warnings:
        print(f"  WARNING: {w}", file=sys.stderr)
    if result.errors:
        for e in result.errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        sys.exit(1)

    env = _setup_env(lang)
    model_names = set(ir["models"].keys())
    lang_module = _load_language(lang)
    lang_module.register_filters(env, model_names)

    out_dir.mkdir(parents=True, exist_ok=True)

    templates_dir = V2_DIR / "templates" / lang
    for template_file in sorted(templates_dir.glob("*.jinja2")):
        output_name = template_file.name.removesuffix(".jinja2")
        template = env.get_template(template_file.name)

        rendered = template.render(
            endpoints=ir["endpoints"],
            models=ir["models"],
        )

        out_path = out_dir / output_name
        out_path.write_text(rendered)
        print(f"  Generated {out_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Unified codegen driver")
    parser.add_argument("lang", help="Target language (e.g., rust)")
    parser.add_argument("--ir", type=Path, default=None, help="Path to api-spec.json (default: run gen_ir first)")
    parser.add_argument("--out", type=Path, required=True, help="Output directory")
    args = parser.parse_args()

    # Validate language module exists before proceeding
    lang_module = _load_language(args.lang)

    ir_path = args.ir
    if ir_path is None:
        print("Running IR extraction...")
        from gen_ir import main as gen_ir_main
        gen_ir_main()
        ir_path = V2_DIR / "api-spec.json"

    if not ir_path.exists():
        print(f"Error: {ir_path} not found.", file=sys.stderr)
        sys.exit(1)

    ir = _load_ir(ir_path)
    print(f"Loaded IR: {len(ir['endpoints'])} endpoints, {len(ir['models'])} models")

    _generate(args.lang, ir, args.out)
    print("Done!")


if __name__ == "__main__":
    main()
