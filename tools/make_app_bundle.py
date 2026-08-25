import os
import shutil
import sys
import plistlib
import argparse


def build_bundle(app_name, binary_path, output_dir, bundle_id, version, icon_path=None):
    bundle_name = f"{app_name}.app"
    bundle_root = os.path.join(output_dir, bundle_name)
    contents_dir = os.path.join(bundle_root, "Contents")
    macos_dir = os.path.join(contents_dir, "MacOS")
    resources_dir = os.path.join(contents_dir, "Resources")

    os.makedirs(macos_dir, exist_ok=True)
    os.makedirs(resources_dir, exist_ok=True)

    binary_name = os.path.basename(binary_path)
    dest_binary = os.path.join(macos_dir, binary_name)
    shutil.copy2(binary_path, dest_binary)
    os.chmod(dest_binary, 0o755)

    if icon_path and os.path.isfile(icon_path):
        shutil.copy2(icon_path, os.path.join(resources_dir, os.path.basename(icon_path)))

    info_plist = {
        "CFBundleName": app_name,
        "CFBundleDisplayName": app_name,
        "CFBundleIdentifier": bundle_id,
        "CFBundleVersion": version,
        "CFBundleShortVersionString": version,
        "CFBundleExecutable": binary_name,
        "CFBundlePackageType": "APPL",
        "CFBundleInfoDictionaryVersion": "6.0",
        "LSMinimumSystemVersion": "10.13",
        "NSHighResolutionCapable": True,
        "LSRequiresNativeExecution": True,
        "CFBundleSupportedPlatforms": ["MacOSX"],
    }

    if icon_path:
        info_plist["CFBundleIconFile"] = os.path.splitext(os.path.basename(icon_path))[0]

    plist_path = os.path.join(contents_dir, "Info.plist")
    with open(plist_path, "wb") as f:
        plistlib.dump(info_plist, f)

    pkginfo_path = os.path.join(contents_dir, "PkgInfo")
    with open(pkginfo_path, "w", encoding="utf-8") as f:
        f.write("APPL????")

    return bundle_root


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--name", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--bundle-id", required=True)
    parser.add_argument("--version", default="1.0.0")
    parser.add_argument("--icon", default=None)
    return parser.parse_args()


def main():
    args = parse_args()

    if not os.path.isfile(args.binary):
        sys.stderr.write(f"binary not found: {args.binary}\n")
        return 1

    os.makedirs(args.output, exist_ok=True)

    bundle_path = build_bundle(
        app_name=args.name,
        binary_path=args.binary,
        output_dir=args.output,
        bundle_id=args.bundle_id,
        version=args.version,
        icon_path=args.icon,
    )

    print(bundle_path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
