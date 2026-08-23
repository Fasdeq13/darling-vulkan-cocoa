use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const BITCODE_WRAPPER_MAGIC: u32 = 0x0B17_C0DE;
const RAW_BITCODE_MAGIC: [u8; 4] = [0x42, 0x43, 0xC0, 0xDE];

struct WrapperHeader {
    magic: u32,
    version: u32,
    offset: u32,
    size: u32,
    cpu_type: u32,
}

fn read_wrapper_header(data: &[u8]) -> Option<WrapperHeader> {
    if data.len() < 20 {
        return None;
    }
    let magic = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if magic != BITCODE_WRAPPER_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let offset = u32::from_le_bytes(data[8..12].try_into().ok()?);
    let size = u32::from_le_bytes(data[12..16].try_into().ok()?);
    let cpu_type = u32::from_le_bytes(data[16..20].try_into().ok()?);
    Some(WrapperHeader { magic, version, offset, size, cpu_type })
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

enum FormatResult {
    Wrapper {
        path: String,
        header: WrapperHeader,
        valid_bounds: bool,
        inner_magic_matches_raw: Option<bool>,
        file_size: Option<usize>,
    },
    Raw {
        path: String,
        file_size: usize,
    },
    Unknown {
        path: String,
        file_size: Option<usize>,
        reason: Option<&'static str>,
    },
    Error {
        path: String,
        reason: String,
    },
}

impl FormatResult {
    fn to_json(&self) -> String {
        match self {
            FormatResult::Wrapper { path, header, valid_bounds, inner_magic_matches_raw, file_size } => {
                let mut s = format!(
                    "{{\"path\": \"{}\", \"format\": \"wrapper\", \"header\": {{\"magic\": {}, \"version\": {}, \"offset\": {}, \"size\": {}, \"cpu_type\": {}}}, \"valid_bounds\": {}",
                    json_escape(path), header.magic, header.version, header.offset, header.size, header.cpu_type, valid_bounds
                );
                if let Some(matches) = inner_magic_matches_raw {
                    s.push_str(&format!(", \"inner_magic_matches_raw\": {}", matches));
                }
                if let Some(fs) = file_size {
                    s.push_str(&format!(", \"file_size\": {}", fs));
                }
                s.push('}');
                s
            }
            FormatResult::Raw { path, file_size } => {
                format!("{{\"path\": \"{}\", \"format\": \"raw\", \"file_size\": {}}}", json_escape(path), file_size)
            }
            FormatResult::Unknown { path, file_size, reason } => {
                let mut s = format!("{{\"path\": \"{}\", \"format\": \"unknown\"", json_escape(path));
                if let Some(fs) = file_size {
                    s.push_str(&format!(", \"file_size\": {}", fs));
                }
                if let Some(r) = reason {
                    s.push_str(&format!(", \"reason\": \"{}\"", r));
                }
                s.push('}');
                s
            }
            FormatResult::Error { path, reason } => {
                format!("{{\"path\": \"{}\", \"format\": \"error\", \"reason\": \"{}\"}}", json_escape(path), json_escape(reason))
            }
        }
    }
}

fn detect_format(path: &Path) -> FormatResult {
    let path_str = path.to_string_lossy().into_owned();
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            return FormatResult::Error { path: path_str, reason: e.to_string() };
        }
    };

    if data.len() < 4 {
        return FormatResult::Unknown { path: path_str, file_size: None, reason: Some("too_short") };
    }

    if let Some(header) = read_wrapper_header(&data) {
        let start = header.offset as usize;
        let end = start.saturating_add(header.size as usize);
        if end > data.len() {
            return FormatResult::Wrapper {
                path: path_str,
                header,
                valid_bounds: false,
                inner_magic_matches_raw: None,
                file_size: None,
            };
        }
        let inner_magic = &data[start..start + 4.min(data.len() - start)];
        let matches = inner_magic.len() == 4 && inner_magic == RAW_BITCODE_MAGIC;
        return FormatResult::Wrapper {
            path: path_str,
            header,
            valid_bounds: true,
            inner_magic_matches_raw: Some(matches),
            file_size: Some(data.len()),
        };
    }

    if data.len() >= 4 && data[0..4] == RAW_BITCODE_MAGIC {
        return FormatResult::Raw { path: path_str, file_size: data.len() };
    }

    FormatResult::Unknown { path: path_str, file_size: Some(data.len()), reason: None }
}

fn scan_directory(dir: &Path, out: &mut Vec<FormatResult>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_directory(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "air" || ext == "metallib" || ext == "bc" {
                out.push(detect_format(&path));
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: air_header_inspect <file_or_directory> [...]");
        return ExitCode::from(1);
    }

    let mut all_results: Vec<FormatResult> = Vec::new();
    for target in &args {
        let path = Path::new(target);
        if path.is_dir() {
            scan_directory(path, &mut all_results);
        } else if path.is_file() {
            all_results.push(detect_format(path));
        } else {
            all_results.push(FormatResult::Error {
                path: target.clone(),
                reason: "not_found".to_string(),
            });
        }
    }

    let items: Vec<String> = all_results.iter().map(|r| r.to_json()).collect();
    println!("[\n  {}\n]", items.join(",\n  "));
    ExitCode::SUCCESS
}
