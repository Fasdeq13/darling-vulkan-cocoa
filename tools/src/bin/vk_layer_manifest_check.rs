use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const REQUIRED_TOP_LEVEL: &[&str] = &["file_format_version", "layer"];
const REQUIRED_LAYER_FIELDS: &[&str] = &["name", "type", "library_path", "api_version", "implementation_version", "description"];
const VALID_TYPES: &[&str] = &["GLOBAL", "INSTANCE"];

fn load_manifest(path: &str) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn validate_manifest(data: &Value, path: &str) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let obj = data.as_object();

    for field in REQUIRED_TOP_LEVEL {
        let present = obj.map(|o| o.contains_key(*field)).unwrap_or(false);
        if !present {
            errors.push(format!("missing top-level field: {field}"));
        }
    }

    let layer = obj.and_then(|o| o.get("layer"));

    match layer {
        Some(Value::Object(layer_obj)) => {
            for field in REQUIRED_LAYER_FIELDS {
                if !layer_obj.contains_key(*field) {
                    errors.push(format!("missing layer field: {field}"));
                }
            }

            if let Some(layer_type) = layer_obj.get("type") {
                if let Some(type_str) = layer_type.as_str() {
                    if !VALID_TYPES.contains(&type_str) {
                        errors.push(format!("invalid layer type: {type_str}"));
                    }
                } else if !layer_type.is_null() {
                    errors.push(format!("invalid layer type: {layer_type}"));
                }
            }

            if let Some(Value::String(lib_path)) = layer_obj.get("library_path") {
                if lib_path.starts_with('.') {
                    let manifest_dir = Path::new(path)
                        .canonicalize()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                        .unwrap_or_else(|| Path::new(".").to_path_buf());
                    let resolved = manifest_dir.join(lib_path);
                    let normalized = normalize_path(&resolved);
                    if !normalized.exists() {
                        warnings.push(format!(
                            "library_path does not resolve to an existing file: {}",
                            normalized.display()
                        ));
                    }
                }
            }

            if let Some(Value::String(api_version)) = layer_obj.get("api_version") {
                let parts: Vec<&str> = api_version.split('.').collect();
                let malformed = parts.len() < 2 || !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
                if malformed {
                    errors.push(format!("malformed api_version: {api_version}"));
                }
            }

            match layer_obj.get("instance_extensions") {
                None => {}
                Some(Value::Array(extensions)) => {
                    for ext in extensions {
                        let ok = ext
                            .as_object()
                            .map(|e| e.contains_key("name") && e.contains_key("spec_version"))
                            .unwrap_or(false);
                        if !ok {
                            errors.push(format!("malformed instance_extensions entry: {ext}"));
                        }
                    }
                }
                Some(_) => {
                    errors.push("instance_extensions must be a list".to_string());
                }
            }
        }
        Some(Value::Null) | None => {}
        Some(_) => {
            errors.push("layer field must be an object".to_string());
        }
    }

    (errors, warnings)
}

fn normalize_path(path: &Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: vk_layer_manifest_check <manifest.json> [...]");
        return ExitCode::from(1);
    }

    let mut overall_ok = true;

    for path in &args {
        let data = match load_manifest(path) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: FAILED TO LOAD ({e})");
                overall_ok = false;
                continue;
            }
        };

        let (errors, warnings) = validate_manifest(&data, path);

        if !errors.is_empty() {
            overall_ok = false;
            println!("{path}: INVALID");
            for err in &errors {
                println!("  error: {err}");
            }
        } else {
            println!("{path}: OK");
        }

        for warn in &warnings {
            println!("  warning: {warn}");
        }
    }

    if overall_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
