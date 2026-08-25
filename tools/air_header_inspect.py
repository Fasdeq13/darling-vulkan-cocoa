import struct
import sys
import os
import json

BITCODE_WRAPPER_MAGIC = 0x0B17C0DE
RAW_BITCODE_MAGIC = bytes([0x42, 0x43, 0xC0, 0xDE])


def read_wrapper_header(data):
    if len(data) < 20:
        return None
    magic = struct.unpack_from("<I", data, 0)[0]
    if magic != BITCODE_WRAPPER_MAGIC:
        return None
    version, offset, size, cpu_type = struct.unpack_from("<IIII", data, 4)
    return {
        "magic": magic,
        "version": version,
        "offset": offset,
        "size": size,
        "cpu_type": cpu_type,
    }


def detect_format(path):
    with open(path, "rb") as f:
        data = f.read()

    if len(data) < 4:
        return {"path": path, "format": "unknown", "reason": "too_short"}

    wrapper = read_wrapper_header(data)
    if wrapper is not None:
        start = wrapper["offset"]
        end = start + wrapper["size"]
        if end > len(data):
            return {"path": path, "format": "wrapper", "header": wrapper, "valid_bounds": False}
        inner_magic = data[start:start + 4]
        return {
            "path": path,
            "format": "wrapper",
            "header": wrapper,
            "valid_bounds": True,
            "inner_magic_matches_raw": inner_magic == RAW_BITCODE_MAGIC,
            "file_size": len(data),
        }

    if data[0:4] == RAW_BITCODE_MAGIC:
        return {"path": path, "format": "raw", "file_size": len(data)}

    return {"path": path, "format": "unknown", "file_size": len(data)}


def scan_directory(directory):
    results = []
    for root, _dirs, files in os.walk(directory):
        for name in files:
            if name.endswith((".air", ".metallib", ".bc")):
                full_path = os.path.join(root, name)
                try:
                    results.append(detect_format(full_path))
                except OSError as exc:
                    results.append({"path": full_path, "format": "error", "reason": str(exc)})
    return results


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: air_header_inspect.py <file_or_directory> [...]\n")
        return 1

    all_results = []
    for target in sys.argv[1:]:
        if os.path.isdir(target):
            all_results.extend(scan_directory(target))
        elif os.path.isfile(target):
            all_results.append(detect_format(target))
        else:
            all_results.append({"path": target, "format": "error", "reason": "not_found"})

    print(json.dumps(all_results, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
