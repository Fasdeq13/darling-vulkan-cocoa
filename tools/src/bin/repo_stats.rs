use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn language_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "c" => Some("C"),
        "h" => Some("C Header"),
        "py" => Some("Python"),
        "toml" => Some("TOML"),
        "yml" | "yaml" => Some("YAML"),
        "md" => Some("Markdown"),
        _ => None,
    }
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules" | "__pycache__" | ".github")
}

#[derive(Default, Clone)]
struct LangStats {
    files: u64,
    lines: u64,
    blank_lines: u64,
}

struct FileEntry {
    path: String,
    language: &'static str,
    lines: u64,
}

fn count_lines(path: &Path) -> (u64, u64) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let mut total = 0u64;
    let mut blank = 0u64;
    for line in content.lines() {
        total += 1;
        if line.trim().is_empty() {
            blank += 1;
        }
    }
    (total, blank)
}

fn scan_repository(root: &Path) -> (BTreeMap<&'static str, LangStats>, Vec<FileEntry>) {
    let mut stats: BTreeMap<&'static str, LangStats> = BTreeMap::new();
    let mut files = Vec::new();
    scan_dir(root, root, &mut stats, &mut files);
    (stats, files)
}

fn scan_dir(root: &Path, dir: &Path, stats: &mut BTreeMap<&'static str, LangStats>, files: &mut Vec<FileEntry>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_ignored_dir(name) {
                    continue;
                }
            }
            scan_dir(root, &path, stats, files);
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = match language_for_extension(ext) {
            Some(l) => l,
            None => continue,
        };

        let (lines, blanks) = count_lines(&path);
        let entry_stats = stats.entry(language).or_default();
        entry_stats.files += 1;
        entry_stats.lines += lines;
        entry_stats.blank_lines += blanks;

        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        files.push(FileEntry { path: rel, language, lines });
    }
}

struct Report {
    root: PathBuf,
    total_files: u64,
    total_lines: u64,
    by_language: BTreeMap<&'static str, LangStats>,
    largest_files: Vec<FileEntry>,
}

fn build_report(root: &Path) -> Report {
    let (stats, mut files) = scan_repository(root);
    let total_files: u64 = stats.values().map(|v| v.files).sum();
    let total_lines: u64 = stats.values().map(|v| v.lines).sum();

    files.sort_by(|a, b| b.lines.cmp(&a.lines));
    files.truncate(10);

    Report {
        root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        total_files,
        total_lines,
        by_language: stats,
        largest_files: files,
    }
}

fn print_human_report(report: &Report) {
    println!("Repository: {}", report.root.display());
    println!("Total files scanned: {}", report.total_files);
    println!("Total lines: {}", report.total_lines);
    println!();
    println!("By language:");
    let mut by_lines: Vec<(&&'static str, &LangStats)> = report.by_language.iter().collect();
    by_lines.sort_by(|a, b| b.1.lines.cmp(&a.1.lines));
    for (language, data) in by_lines {
        println!(
            "  {:<15} files={:<5} lines={:<8} blank={}",
            language, data.files, data.lines, data.blank_lines
        );
    }
    println!();
    println!("Largest files:");
    for entry in &report.largest_files {
        println!("  {:>6}  {}", entry.lines, entry.path);
    }
}

fn print_json_report(report: &Report) {
    let mut lang_items = Vec::new();
    for (language, data) in &report.by_language {
        lang_items.push(format!(
            "    \"{}\": {{\"files\": {}, \"lines\": {}, \"blank_lines\": {}}}",
            language, data.files, data.lines, data.blank_lines
        ));
    }

    let mut file_items = Vec::new();
    for entry in &report.largest_files {
        file_items.push(format!(
            "    {{\"path\": \"{}\", \"language\": \"{}\", \"lines\": {}}}",
            entry.path.replace('\\', "\\\\").replace('"', "\\\""),
            entry.language,
            entry.lines
        ));
    }

    println!("{{");
    println!("  \"root\": \"{}\",", report.root.display().to_string().replace('\\', "\\\\"));
    println!("  \"total_files\": {},", report.total_files);
    println!("  \"total_lines\": {},", report.total_lines);
    println!("  \"by_language\": {{\n{}\n  }},", lang_items.join(",\n"));
    println!("  \"largest_files\": [\n{}\n  ]", file_items.join(",\n"));
    println!("}}");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut root = ".".to_string();
    let mut json_output = false;

    for arg in &args {
        if arg == "--json" {
            json_output = true;
        } else {
            root = arg.clone();
        }
    }

    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        eprintln!("not a directory: {root}");
        return ExitCode::from(1);
    }

    let report = build_report(root_path);

    if json_output {
        print_json_report(&report);
    } else {
        print_human_report(&report);
    }

    ExitCode::SUCCESS
}
