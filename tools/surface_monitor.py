import struct
import sys
import time
import glob
import argparse

HEADER_FORMAT = "<IIIIIIQQ"
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)
EXPECTED_MAGIC = 0x514D5631


def read_header(path):
    with open(path, "rb") as f:
        raw = f.read(HEADER_SIZE)
    if len(raw) < HEADER_SIZE:
        return None
    fields = struct.unpack(HEADER_FORMAT, raw)
    magic, width, height, stride, pixel_format, lock_state, frame_counter, data_size = fields
    if magic != EXPECTED_MAGIC:
        return None
    return {
        "path": path,
        "width": width,
        "height": height,
        "stride": stride,
        "pixel_format": pixel_format,
        "locked": bool(lock_state & 1),
        "frame_counter": frame_counter,
        "data_size": data_size,
    }


def discover_surfaces(search_globs):
    found = []
    for pattern in search_globs:
        for path in glob.glob(pattern):
            found.append(path)
    return sorted(set(found))


def format_row(info, previous_counter):
    delta = info["frame_counter"] - previous_counter if previous_counter is not None else 0
    lock_str = "LOCKED" if info["locked"] else "free"
    return "{path:<40} {width}x{height} stride={stride} fmt={fmt} frames={frames} (+{delta}) {lock}".format(
        path=info["path"],
        width=info["width"],
        height=info["height"],
        stride=info["stride"],
        fmt=info["pixel_format"],
        frames=info["frame_counter"],
        delta=delta,
        lock=lock_str,
    )


def monitor(search_globs, interval, iterations):
    previous_counters = {}
    count = 0
    while iterations is None or count < iterations:
        paths = discover_surfaces(search_globs)
        if not paths:
            print("no matching surfaces found")
        for path in paths:
            try:
                info = read_header(path)
            except OSError as exc:
                print(f"{path}: read error ({exc})")
                continue
            if info is None:
                continue
            prev = previous_counters.get(path)
            print(format_row(info, prev))
            previous_counters[path] = info["frame_counter"]
        print("-" * 60)
        count += 1
        if iterations is None or count < iterations:
            time.sleep(interval)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--glob", action="append", dest="globs", default=None)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--iterations", type=int, default=None)
    return parser.parse_args()


def main():
    args = parse_args()
    globs = args.globs if args.globs else ["/dev/shm/qmv_surface_*", "/tmp/qmv_surface_*"]
    monitor(globs, args.interval, args.iterations)
    return 0


if __name__ == "__main__":
    sys.exit(main())
