import os
import sys
import argparse
import json
from collections import defaultdict

EXTENSION_LANGUAGE_MAP = {
    ".rs": "Rust",
    ".c": "C",
    ".h": "C Header",
    ".py": "Python",
    ".toml": "TOML",
    ".yml": "YAML",
    ".yaml": "YAML",
    ".md": "Markdown",
}

IGNORED_DIRS = {".git", "target", "node_modules", "__pycache__", ".github"}


def count_lines(path):
    total = 0
    blank = 0
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                total += 1
                if line.strip() == "":
                    blank += 1
    except OSError:
        return 0, 0
    return total, blank


def scan_repository(root):
    stats = defaultdict(lambda: {"files": 0, "lines": 0, "blank_lines": 0})
    file_list = []

    for current_root, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in IGNORED_DIRS]
        for name in files:
            ext = os.path.splitext(name)[1]
            language = EXTENSION_LANGUAGE_MAP.get(ext)
            if language is None:
                continue
            full_path = os.path.join(current_root, name)
            lines, blanks = count_lines(full_path)
            stats[language]["files"] += 1
            stats[language]["lines"] += lines
            stats[language]["blank_lines"] += blanks
            file_list.append({
                "path": os.path.relpath(full_path, root),
                "language": language,
                "lines": lines,
            })

    return stats, file_list


def build_report(root):
    stats, file_list = scan_repository(root)
    total_files = sum(v["files"] for v in stats.values())
    total_lines = sum(v["lines"] for v in stats.values())

    report = {
        "root": os.path.abspath(root),
        "total_files": total_files,
        "total_lines": total_lines,
        "by_language": dict(stats),
        "largest_files": sorted(file_list, key=lambda x: x["lines"], reverse=True)[:10],
    }
    return report


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def print_human_report(report):
    print(f"Repository: {report['root']}")
    print(f"Total files scanned: {report['total_files']}")
    print(f"Total lines: {report['total_lines']}")
    print()
    print("By language:")
    for language, data in sorted(report["by_language"].items(), key=lambda x: x[1]["lines"], reverse=True):
        print(f"  {language:<15} files={data['files']:<5} lines={data['lines']:<8} blank={data['blank_lines']}")
    print()
    print("Largest files:")
    for entry in report["largest_files"]:
        print(f"  {entry['lines']:>6}  {entry['path']}")


def main():
    args = parse_args()
    if not os.path.isdir(args.root):
        sys.stderr.write(f"not a directory: {args.root}\n")
        return 1

    report = build_report(args.root)

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print_human_report(report)

    return 0


if __name__ == "__main__":
    sys.exit(main())
