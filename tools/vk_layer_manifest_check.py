import json
import sys
import os

REQUIRED_TOP_LEVEL = ["file_format_version", "layer"]
REQUIRED_LAYER_FIELDS = ["name", "type", "library_path", "api_version", "implementation_version", "description"]
VALID_TYPES = {"GLOBAL", "INSTANCE"}


def load_manifest(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def validate_manifest(data, path):
    errors = []
    warnings = []

    for field in REQUIRED_TOP_LEVEL:
        if field not in data:
            errors.append(f"missing top-level field: {field}")

    layer = data.get("layer")
    if isinstance(layer, dict):
        for field in REQUIRED_LAYER_FIELDS:
            if field not in layer:
                errors.append(f"missing layer field: {field}")

        layer_type = layer.get("type")
        if layer_type is not None and layer_type not in VALID_TYPES:
            errors.append(f"invalid layer type: {layer_type}")

        lib_path = layer.get("library_path")
        if isinstance(lib_path, str) and lib_path.startswith("."):
            manifest_dir = os.path.dirname(os.path.abspath(path))
            resolved = os.path.normpath(os.path.join(manifest_dir, lib_path))
            if not os.path.exists(resolved):
                warnings.append(f"library_path does not resolve to an existing file: {resolved}")

        api_version = layer.get("api_version")
        if isinstance(api_version, str):
            parts = api_version.split(".")
            if len(parts) < 2 or not all(p.isdigit() for p in parts):
                errors.append(f"malformed api_version: {api_version}")

        instance_extensions = layer.get("instance_extensions", [])
        if not isinstance(instance_extensions, list):
            errors.append("instance_extensions must be a list")
        else:
            for ext in instance_extensions:
                if not isinstance(ext, dict) or "name" not in ext or "spec_version" not in ext:
                    errors.append(f"malformed instance_extensions entry: {ext}")
    elif layer is not None:
        errors.append("layer field must be an object")

    return errors, warnings


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: vk_layer_manifest_check.py <manifest.json> [...]\n")
        return 1

    overall_ok = True
    for path in sys.argv[1:]:
        try:
            data = load_manifest(path)
        except (OSError, json.JSONDecodeError) as exc:
            print(f"{path}: FAILED TO LOAD ({exc})")
            overall_ok = False
            continue

        errors, warnings = validate_manifest(data, path)

        if errors:
            overall_ok = False
            print(f"{path}: INVALID")
            for err in errors:
                print(f"  error: {err}")
        else:
            print(f"{path}: OK")

        for warn in warnings:
            print(f"  warning: {warn}")

    return 0 if overall_ok else 1


if __name__ == "__main__":
    sys.exit(main())
