use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const EXPECTED_MAGIC: u32 = 0x514D_5631;
const HEADER_SIZE: usize = 4 * 6 + 8 * 2;

struct SurfaceInfo {
    path: String,
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32,
    locked: bool,
    frame_counter: u64,
    data_size: u64,
}

fn read_header(path: &str) -> Option<SurfaceInfo> {
    let mut file = File::open(path).ok()?;
    let mut raw = [0u8; HEADER_SIZE];
    let read = file.read(&mut raw).ok()?;
    if read < HEADER_SIZE {
        return None;
    }

    let magic = u32::from_le_bytes(raw[0..4].try_into().ok()?);
    if magic != EXPECTED_MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(raw[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(raw[8..12].try_into().ok()?);
    let stride = u32::from_le_bytes(raw[12..16].try_into().ok()?);
    let pixel_format = u32::from_le_bytes(raw[16..20].try_into().ok()?);
    let lock_state = u32::from_le_bytes(raw[20..24].try_into().ok()?);
    let frame_counter = u64::from_le_bytes(raw[24..32].try_into().ok()?);
    let data_size = u64::from_le_bytes(raw[32..40].try_into().ok()?);

    Some(SurfaceInfo {
        path: path.to_string(),
        width,
        height,
        stride,
        pixel_format,
        locked: (lock_state & 1) != 0,
        frame_counter,
        data_size,
    })
}

fn glob_match(pattern: &str) -> Vec<String> {
    let path = PathBuf::from(pattern);
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("/"));
    let file_pattern = match path.file_name().and_then(|f| f.to_str()) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let (prefix, suffix) = match file_pattern.split_once('*') {
        Some((p, s)) => (p, s),
        None => {
            return if path.exists() {
                vec![path.to_string_lossy().into_owned()]
            } else {
                Vec::new()
            };
        }
    };

    let entries = match std::fs::read_dir(parent) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(prefix) && name.ends_with(suffix) && name.len() >= prefix.len() + suffix.len() {
                out.push(entry.path().to_string_lossy().into_owned());
            }
        }
    }
    out
}

fn discover_surfaces(patterns: &[String]) -> Vec<String> {
    let mut found: Vec<String> = patterns.iter().flat_map(|p| glob_match(p)).collect();
    found.sort();
    found.dedup();
    found
}

fn format_row(info: &SurfaceInfo, previous_counter: Option<u64>) -> String {
    let delta = previous_counter.map(|p| info.frame_counter.saturating_sub(p)).unwrap_or(0);
    let lock_str = if info.locked { "LOCKED" } else { "free" };
    format!(
        "{:<40} {}x{} stride={} fmt={} frames={} (+{}) {}",
        info.path, info.width, info.height, info.stride, info.pixel_format, info.frame_counter, delta, lock_str
    )
}

fn monitor(patterns: &[String], interval: Duration, iterations: Option<u64>) {
    let mut previous_counters: HashMap<String, u64> = HashMap::new();
    let mut count: u64 = 0;

    loop {
        if let Some(max) = iterations {
            if count >= max {
                break;
            }
        }

        let paths = discover_surfaces(patterns);
        if paths.is_empty() {
            println!("no matching surfaces found");
        }
        for path in &paths {
            match read_header(path) {
                Some(info) => {
                    let prev = previous_counters.get(path).copied();
                    println!("{}", format_row(&info, prev));
                    previous_counters.insert(path.clone(), info.frame_counter);
                }
                None => continue,
            }
        }
        println!("{}", "-".repeat(60));

        count += 1;
        let should_sleep = iterations.map(|max| count < max).unwrap_or(true);
        if should_sleep {
            std::thread::sleep(interval);
        } else {
            break;
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut globs: Vec<String> = Vec::new();
    let mut interval = 1.0f64;
    let mut iterations: Option<u64> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--glob" => {
                if let Some(v) = args.get(i + 1) {
                    globs.push(v.clone());
                    i += 1;
                }
            }
            "--interval" => {
                if let Some(v) = args.get(i + 1) {
                    interval = v.parse().unwrap_or(1.0);
                    i += 1;
                }
            }
            "--iterations" => {
                if let Some(v) = args.get(i + 1) {
                    iterations = v.parse().ok();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if globs.is_empty() {
        globs = vec![
            "/dev/shm/qmv_surface_*".to_string(),
            "/tmp/qmv_surface_*".to_string(),
        ];
    }

    monitor(&globs, Duration::from_secs_f64(interval), iterations);
    ExitCode::SUCCESS
}
