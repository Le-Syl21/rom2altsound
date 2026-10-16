#!/usr/bin/env python3
"""Measures whether the log ("Details") panel holds still: runs a few ROMs with the log
open and its filter on every ROM, and samples the visible log lines over consecutive
frames (see `sample`). Also takes a few screenshots. The window is narrow (--width)
by default: the lines of every ROM are then wider than the log, the case that bounced.

Usage: log_jitter.py --mcp <egui-mcp binary> --port <inspection port> --shots <dir>
                     --rom <zip path inside the container>... [--samples N]

Run it through run.sh: DRIVER=log_jitter.py tools/gui-test/run.sh
Every wait is bounded; exits non-zero when the log bounced.
"""

import argparse
import json
import sys
import time

sys.dont_write_bytecode = True  # no __pycache__ next to the scripts
from drive import Mcp, click_button, nodes, set_field, shot, step, wait_gone, wait_port, wait_text  # noqa: E402


def log_rows(mcp, area):
    """The visible log lines inside the log area: (text, top y, left x), top to bottom."""
    out = []
    for n in nodes(mcp, role="Label", visible_only=True, limit=500):
        b = n.get("bounds")
        if not b:
            continue
        x0, y0 = b["x"], b["y"]
        if area and not (area[0] - 1 <= y0 < area[1] - 1):
            continue
        out.append(((n.get("label") or n.get("value") or "").strip(), round(y0, 2), round(x0, 2)))
    out.sort(key=lambda r: r[1])
    return out


def log_area(mcp):
    """The vertical extent of the log's scroll area: from below the filter row to the footer."""
    combo = nodes(mcp, content_contains="every ROM") or nodes(mcp, role="ComboBox")
    foot = nodes(mcp, content_contains="runs PinMAME's sound board")
    def bottom(n):
        return n["bounds"]["y"] + n["bounds"]["h"]

    def top(n):
        return n["bounds"]["y"]

    lo = max(bottom(n) for n in combo) if combo else 0
    hi = min(top(n) for n in foot) if foot else 1e9
    return (lo, hi)


def sample(mcp, a, label):
    """`a.samples` frames, a pointer move over the log before each (a frame, as with the
    window focused and the mouse on it). Returns the problems seen:
    - a frame showing the same lines as the one before, elsewhere (moving by itself);
    - the newest line coming to rest at different heights from one line to the next
      (a view stuck to the bottom always rests it at the same place: when it does not,
      the text bounces at each new line)."""
    area = log_area(mcp)
    prev = None
    moved = same = changed = 0
    rests = []
    trace = []
    for i in range(a.samples):
        mcp.call("hover", pos={"x": 200 + (i % 7) * 40, "y": (area[0] + min(area[1], area[0] + 200)) / 2})
        mcp.call("wait_for", min_steps=1, timeout_secs=5)
        rows = log_rows(mcp, area)
        if rows:
            trace.append((rows[0][1], rows[-1][1], rows[-1][2], len(rows), rows[-1][0][:24]))
        if prev is not None and rows:
            if [r[0] for r in rows] == [r[0] for r in prev]:
                if [r[1:] for r in rows] != [r[1:] for r in prev]:
                    moved += 1
                    step(f"  moved: {json.dumps([r[1:] for r in prev[-2:]])} -> {json.dumps([r[1:] for r in rows[-2:]])}")
                else:
                    same += 1
                    rests.append(rows[-1][1])
            else:
                changed += 1
        prev = rows
    spread = max(rests) - min(rests) if rests else 0.0
    step(
        f"{label}: {same} still, {moved} moved without new lines, {changed} with new lines; "
        f"the newest line rests at y {sorted(set(rests))} (spread {spread:.2f} px)"
    )
    if a.trace:
        for t in trace:
            step(f"  frame: first y {t[0]}, last y {t[1]}, x {t[2]}, {t[3]} lines, last '{t[4]}'")
    problems = []
    if moved:
        problems.append(f"{label}: the log moved by itself in {moved} frame(s)")
    # Less than a pixel: the text's sub-pixel placement, invisible.
    if spread >= 1.0:
        problems.append(f"{label}: the newest line rests {spread:.2f} px apart from one line to the next")
    return problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcp", required=True)
    ap.add_argument("--port", type=int, default=5719)
    ap.add_argument("--shots", required=True)
    ap.add_argument("--rom", action="append", required=True)
    ap.add_argument("--out", default="/out/packs")
    ap.add_argument("--samples", type=int, default=60)
    ap.add_argument("--width", type=int, default=900)
    ap.add_argument("--trace", action="store_true")
    # Ignored: run.sh's arguments for drive.py.
    ap.add_argument("--folder")
    ap.add_argument("--check-procs")
    a = ap.parse_args()

    step("wait for the window")
    wait_port(a.port, 90)
    mcp = Mcp(a.mcp)
    mcp.call("attach", host="127.0.0.1", port=a.port, timeout_secs=60)
    mcp.call("resize", width=a.width, height=860)
    wait_text(mcp, "Drop the ROM zips here", 60)
    wait_gone(mcp, "Reading PinMAME", 120)
    for z in a.rom:
        set_field(mcp, "or type a zip or folder path", z)
        click_button(mcp, "Add")
    for z in a.rom:
        wait_text(mcp, z.rsplit("/", 1)[-1].removesuffix(".zip"), 120)
    wait_gone(mcp, "Checking", 120)
    set_field(mcp, "Where the packs go", a.out)

    step("start with the log open")
    mcp.call("click", content_contains="Details")
    mcp.call("click", role="Button", label_contains="Make the packs")
    wait_text(mcp, "Cancel", 20)
    wait_text(mcp, "recording the sounds", 300)
    time.sleep(5)
    for i in range(4):
        shot(mcp, a.shots, f"jitter-run-{i}")
    problems = sample(mcp, a, "while running, every ROM")

    click_button(mcp, "Cancel")
    wait_text(mcp, "Make the packs", 120)
    time.sleep(2)
    for i in range(3):
        shot(mcp, a.shots, f"jitter-idle-{i}")
    problems += sample(mcp, a, "after the run, every ROM")
    mcp.close()
    if problems:
        for p in problems:
            step(f"FAILED: {p}")
        sys.exit(1)
    step("OK: the log held still")


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except Exception as e:  # noqa: BLE001 — a test script: report and fail
        print(f"[drive] FAILED: {e}", file=sys.stderr)
        sys.exit(1)
