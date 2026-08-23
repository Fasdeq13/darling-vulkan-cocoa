use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct BundleArgs {
    name: String,
    binary: PathBuf,
    output: PathBuf,
    bundle_id: String,
    version: String,
    icon: Option<PathBuf>,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_info_plist(path: &Path, app: &BundleArgs, binary_name: &str) -> std::io::Result<()> {
    let mut body = String::new();
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    body.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    body.push_str("<plist version=\"1.0\">\n<dict>\n");

    let mut push_string = |key: &str, value: &str, body: &mut String| {
        body.push_str(&format!("\t<key>{}</key>\n\t<string>{}</string>\n", xml_escape(key), xml_escape(value)));
    };

    push_string("CFBundleName", &app.name, &mut body);
    push_string("CFBundleDisplayName", &app.name, &mut body);
    push_string("CFBundleIdentifier", &app.bundle_id, &mut body);
    push_string("CFBundleVersion", &app.version, &mut body);
    push_string("CFBundleShortVersionString", &app.version, &mut body);
    push_string("CFBundleExecutable", binary_name, &mut body);
    push_string("CFBundlePackageType", "APPL", &mut body);
    push_string("CFBundleInfoDictionaryVersion", "6.0", &mut body);
    push_string("LSMinimumSystemVersion", "10.13", &mut body);

    body.push_str("\t<key>NSHighResolutionCapable</key>\n\t<true/>\n");
    body.push_str("\t<key>LSRequiresNativeExecution</key>\n\t<true/>\n");

    body.push_str("\t<key>CFBundleSupportedPlatforms</key>\n\t<array>\n\t\t<string>MacOSX</string>\n\t</array>\n");

    if let Some(icon) = &app.icon {
        if let Some(stem) = icon.file_stem().and_then(|s| s.to_str()) {
            push_string("CFBundleIconFile", stem, &mut body);
        }
    }

    body.push_str("</dict>\n</plist>\n");

    fs::write(path, body)
}

fn build_bundle(app: &BundleArgs) -> std::io::Result<PathBuf> {
    let bundle_name = format!("{}.app", app.name);
    let bundle_root = app.output.join(&bundle_name);
    let contents_dir = bundle_root.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");

    fs::create_dir_all(&macos_dir)?;
    fs::create_dir_all(&resources_dir)?;

    let binary_name = app
        .binary
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid binary path"))?
        .to_string();
    let dest_binary = macos_dir.join(&binary_name);
    fs::copy(&app.binary, &dest_binary)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest_binary, fs::Permissions::from_mode(0o755))?;
    }

    if let Some(icon) = &app.icon {
        if icon.is_file() {
            if let Some(icon_name) = icon.file_name() {
                fs::copy(icon, resources_dir.join(icon_name))?;
            }
        }
    }

    write_info_plist(&contents_dir.join("Info.plist"), app, &binary_name)?;
    fs::write(contents_dir.join("PkgInfo"), b"APPL????")?;

    Ok(bundle_root)
}

fn parse_args(args: &[String]) -> Result<BundleArgs, String> {
    let mut name = None;
    let mut binary = None;
    let mut output = None;
    let mut bundle_id = None;
    let mut version = "1.0.0".to_string();
    let mut icon = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--name" => {
                name = args.get(i + 1).cloned();
                i += 1;
            }
            "--binary" => {
                binary = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--output" => {
                output = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--bundle-id" => {
                bundle_id = args.get(i + 1).cloned();
                i += 1;
            }
            "--version" => {
                if let Some(v) = args.get(i + 1) {
                    version = v.clone();
                }
                i += 1;
            }
            "--icon" => {
                icon = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    Ok(BundleArgs {
        name: name.ok_or("--name is required")?,
        binary: binary.ok_or("--binary is required")?,
        output: output.ok_or("--output is required")?,
        bundle_id: bundle_id.ok_or("--bundle-id is required")?,
        version,
        icon,
    })
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let app = match parse_args(&raw_args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    if !app.binary.is_file() {
        eprintln!("binary not found: {}", app.binary.display());
        return ExitCode::from(1);
    }

    if let Err(e) = fs::create_dir_all(&app.output) {
        eprintln!("failed to create output directory: {e}");
        return ExitCode::from(1);
    }

    match build_bundle(&app) {
        Ok(bundle_path) => {
            println!("{}", bundle_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("failed to build bundle: {e}");
            ExitCode::from(1)
        }
    }
}
