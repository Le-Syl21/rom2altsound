#!/usr/bin/env python3
"""Run a rom2altsound binary on the synthetic test ROM set and check the pack it writes.

    check_pack.py --bin PATH [--zip-arg] [--vpm DIR] [--work DIR] [--fixture DIR]

--bin       the program to run: rom2altsound, or rom2altsound-gui (given arguments, it is
            the command line)
--zip-arg   name the ROM by its zip path instead of `flash_l1 --roms <fixture>`
--vpm       a private PinMAME directory (default: the program's own, in the user cache)
--work      where the pack is written (default: a new temporary directory)

The fixture (tests/fixtures/s67s, see its README) is a synthetic sound program written
for these tests: expected.json says which commands play a tone, at which frequency and
for how long. Exit status 0 when every check passes. Standard library only, so that it
runs as is on every CI runner (Linux, macOS, Windows).
"""

import argparse
import csv
import json
import math
import os
import subprocess
import sys
import tempfile
import time
import wave
from array import array

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_FIXTURE = os.path.join(HERE, "fixtures", "s67s")

# Tolerances: the length of a tone is measured from its first to its last loud sample,
# its frequency from its zero crossings.
SECONDS_TOLERANCE = 0.05
FREQUENCY_TOLERANCE = 0.02
MIN_PEAK = 0.05  # of full scale: a non-silent file


def fail(msg):
    print(f"FAIL: {msg}")
    sys.exit(1)


def read_wav(path):
    with wave.open(path, "rb") as w:
        if w.getsampwidth() != 2:
            fail(f"{path}: {8 * w.getsampwidth()}-bit samples, expected 16")
        channels, rate = w.getnchannels(), w.getframerate()
        data = array("h", w.readframes(w.getnframes()))
    if sys.byteorder == "big":
        data.byteswap()
    # First channel only (the files are mono, or identical channels).
    return [s / 32768.0 for s in data[::channels]], rate


def measure(samples, rate):
    """(peak, length in seconds of the loud part, frequency in Hz) of a tone file."""
    mean = sum(samples) / len(samples)
    centred = [s - mean for s in samples]
    peak = max(abs(s) for s in centred)
    if peak < MIN_PEAK:
        return peak, 0.0, 0.0
    threshold = peak / 4
    loud = [i for i, s in enumerate(centred) if abs(s) >= threshold]
    first, last = loud[0], loud[-1]
    span = centred[first : last + 1]
    # Rising edges through a Schmitt trigger (+-threshold): one per period, unaffected
    # by the small wiggles of the filtered tail.
    rising, low = [], False
    for i, v in enumerate(span):
        if v <= -threshold:
            low = True
        elif v >= threshold and low:
            rising.append(i)
            low = False
    secs = (last - first + 1) / rate
    if len(rising) < 2:
        return peak, secs, 0.0
    return peak, secs, (len(rising) - 1) / ((rising[-1] - rising[0]) / rate)


def run(args, cwd, timeout):
    print("$ " + " ".join(args), flush=True)
    start = time.monotonic()
    try:
        proc = subprocess.run(
            args,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        fail(f"no exit after {timeout} s")
    except OSError as e:
        fail(f"cannot start {args[0]}: {e}")
    out = proc.stdout.decode("utf-8", "replace")
    print(out, end="" if out.endswith("\n") else "\n")
    print(f"exit status {proc.returncode} after {time.monotonic() - start:.1f} s", flush=True)
    return proc.returncode


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bin", required=True)
    ap.add_argument("--fixture", default=DEFAULT_FIXTURE)
    ap.add_argument("--zip-arg", action="store_true")
    ap.add_argument("--vpm")
    ap.add_argument("--work")
    ap.add_argument("--timeout", type=int, default=600)
    a = ap.parse_args()

    with open(os.path.join(a.fixture, "expected.json")) as f:
        expected = json.load(f)
    rom = expected["rom"]
    work = os.path.abspath(a.work or tempfile.mkdtemp(prefix="rom2altsound-check-"))
    os.makedirs(work, exist_ok=True)
    out = os.path.join(work, "out")
    pack = os.path.join(out, rom)

    exe = os.path.abspath(a.bin)
    fixture = os.path.abspath(a.fixture)
    if a.zip_arg:
        args = [exe, os.path.join(fixture, rom + ".zip")]
    else:
        args = [exe, rom, "--roms", fixture]
    args += ["--out", out]
    if a.vpm:
        args += ["--vpm", os.path.abspath(a.vpm)]
    status = run(args, work, a.timeout)
    if status != 0:
        fail(f"exit status {status}")

    for name in ("manifest.json", "altsound.csv", "g-sound.csv", "altsound.ini", "index.html"):
        if not os.path.isfile(os.path.join(pack, name)):
            fail(f"{name} missing from {pack}")

    with open(os.path.join(pack, "altsound.csv"), newline="") as f:
        rows = list(csv.DictReader(f))
    want_ids = [f"0x{s['id']:04X}" for s in expected["sounds"]]
    got_ids = sorted(r["ID"] for r in rows)
    if got_ids != want_ids:
        fail(f"altsound.csv: ids {got_ids}, expected {want_ids}")

    with open(os.path.join(pack, "manifest.json")) as f:
        manifest = json.load(f)
    written = sorted(s["id"] for s in manifest.get("sounds", []) if s.get("file"))
    if written != [f"0x{s['id']:02X}" for s in expected["sounds"]]:
        fail(f"manifest.json: sounds with a file {written}, expected {want_ids}")
    counts = manifest.get("counts", {})
    if counts.get("written") != len(want_ids) or counts.get("tried", 0) < len(want_ids):
        fail(f"manifest.json: counts {counts}")

    by_id = {r["ID"]: r for r in rows}
    errors = []
    for s in expected["sounds"]:
        row = by_id[f"0x{s['id']:04X}"]
        path = os.path.join(pack, row["FNAME"])
        if not os.path.isfile(path):
            errors.append(f"{row['FNAME']} missing")
            continue
        samples, rate = read_wav(path)
        peak, secs, freq = measure(samples, rate)
        line = (
            f"{row['FNAME']}: peak {peak:.3f}, tone {secs:.3f} s (expected {s['seconds']:.3f}), "
            f"{freq:.1f} Hz (expected {s['frequency_hz']:.1f})"
        )
        print(line)
        if peak < MIN_PEAK:
            errors.append(f"{row['FNAME']}: silent (peak {peak:.4f})")
        elif abs(secs - s["seconds"]) > SECONDS_TOLERANCE:
            errors.append(f"{line}: wrong length")
        elif abs(freq - s["frequency_hz"]) > FREQUENCY_TOLERANCE * s["frequency_hz"]:
            errors.append(f"{line}: wrong frequency")
    if errors:
        fail("; ".join(errors))
    print(f"OK: {len(want_ids)} sounds, pack in {pack}")


if __name__ == "__main__":
    main()
