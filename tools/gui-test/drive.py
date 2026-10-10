#!/usr/bin/env python3
"""Drives a running rom2altsound-gui through the egui_mcp server (the MCP server of the
egui_inspection protocol, https://crates.io/crates/egui_mcp), the way an agent would:
add ROMs, set the output folder, start, watch the progress, cancel, run again to the end,
switch the language, and take screenshots along the way.

Usage: drive.py --mcp <egui-mcp binary> --port <inspection port> --shots <dir>
                --rom <zip path inside the container>... [--limit N] [--check-procs CMD]

Every wait is bounded; the script exits non-zero on the first failed step.
"""

import argparse
import json
import socket
import subprocess
import sys
import time


class Mcp:
    """A minimal MCP client over stdio (newline-delimited JSON-RPC 2.0)."""

    def __init__(self, exe):
        self.p = subprocess.Popen(
            [exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1
        )
        self.next_id = 0
        self.request("initialize", {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "rom2altsound-gui-test", "version": "1"},
        })
        self.notify("notifications/initialized", {})

    def notify(self, method, params):
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
        self.p.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        rid = self.next_id
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("egui-mcp closed its output")
            msg = json.loads(line)
            if msg.get("id") == rid:
                if "error" in msg:
                    raise RuntimeError(f"{method}: {msg['error']}")
                return msg["result"]

    def call(self, tool, **args):
        r = self.request("tools/call", {"name": tool, "arguments": args})
        texts = [c.get("text", "") for c in r.get("content", []) if c.get("type") == "text"]
        if r.get("isError"):
            raise RuntimeError(f"{tool}({args}): {' '.join(texts)}")
        return r.get("structuredContent"), texts

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)


def wait_port(port, timeout):
    """Waits until the app answers the inspection handshake (a container's port accepts
    connections before the app inside listens)."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=2) as s:
                if s.recv(4) == b"eins":
                    return
        except OSError:
            pass
        time.sleep(0.5)
    raise RuntimeError(f"no inspection port on {port} after {timeout} s")


def step(msg):
    print(f"[drive] {msg}", flush=True)


def nodes(mcp, **query):
    out, _ = mcp.call("query_tree", **query)
    return (out or {}).get("nodes", [])


def text_of(n):
    return f"{n.get('label') or ''} {n.get('value') or ''}".strip()


def wait_text(mcp, text, timeout):
    """Waits until a visible node shows `text`."""
    mcp.call("wait_for", content_contains=text, timeout_secs=timeout)


def wait_gone(mcp, text, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if not nodes(mcp, content_contains=text):
            return
        time.sleep(1)
    raise RuntimeError(f"'{text}' still shown after {timeout} s")


def set_field(mcp, label, value):
    """Replaces a text field's contents (the field found by its label)."""
    fields = [n for n in nodes(mcp, role="TextInput", label_contains=label)]
    if not fields:
        raise RuntimeError(f"no text field labelled '{label}'")
    fid = fields[0]["id"]
    mcp.call("click", id=fid)
    mcp.call("press_key", key="A", modifiers={"command": True, "ctrl": True})
    mcp.call("type_text", id=fid, text=value)


def click_button(mcp, label):
    """Clicks the button whose label is exactly `label` (a substring would also match
    "Add ROM zips…" for "Add")."""
    exact = [n for n in nodes(mcp, role="Button", label_contains=label) if (n.get("label") or "").strip() == label]
    if not exact:
        raise RuntimeError(f"no button '{label}'")
    mcp.call("click", id=exact[0]["id"])


def shot(mcp, shots, name):
    path = f"{shots}/{name}.png"
    mcp.call("wait_for", min_steps=3, timeout_secs=10)
    mcp.call("screenshot", pixels_per_point=1.0, save_path=path)
    step(f"screenshot {path}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcp", required=True)
    ap.add_argument("--port", type=int, default=5719)
    ap.add_argument("--shots", required=True)
    ap.add_argument("--rom", action="append", required=True)
    ap.add_argument("--out", default="/out/packs")
    ap.add_argument("--limit", type=int, default=12)
    ap.add_argument("--folder", help="a ROM folder to add at the end (the list of a whole collection)")
    ap.add_argument("--check-procs", help="command printing the number of extraction processes left")
    a = ap.parse_args()

    step("wait for the window")
    wait_port(a.port, 90)
    mcp = Mcp(a.mcp)
    step("attach")
    mcp.call("attach", host="127.0.0.1", port=a.port, timeout_secs=60)
    mcp.call("resize", width=1280, height=860)

    step("wait for PinMAME's table")
    wait_text(mcp, "Drop the ROM zips here", 60)
    wait_gone(mcp, "Reading PinMAME", 120)
    shot(mcp, a.shots, "00-empty")

    for z in a.rom:
        step(f"add {z}")
        set_field(mcp, "or type a zip or folder path", z)
        click_button(mcp, "Add")
    names = [z.rsplit("/", 1)[-1].removesuffix(".zip") for z in a.rom]
    for n in names:
        wait_text(mcp, n, 120)
    wait_gone(mcp, "Checking", 120)
    shot(mcp, a.shots, "01-roms")

    step("output folder")
    set_field(mcp, "Where the packs go", a.out)

    # A short run: the advanced --limit keeps each ROM to a few commands.
    step("advanced options")
    mcp.call("click", content_contains="Advanced options")
    set_field(mcp, "--limit", str(a.limit))
    shot(mcp, a.shots, "02-options")
    mcp.call("click", content_contains="Advanced options")

    step("start, then cancel")
    mcp.call("click", role="Button", label_contains="Make the packs")
    wait_text(mcp, "Cancel", 20)
    # Some ROM is past its boot: its commands are being recorded.
    wait_text(mcp, "recording the sounds", 300)
    shot(mcp, a.shots, "03-running")
    click_button(mcp, "Cancel")
    wait_text(mcp, "cancelled", 60)
    wait_text(mcp, "Make the packs", 60)
    shot(mcp, a.shots, "04-cancelled")
    if a.check_procs:
        time.sleep(2)
        left = subprocess.run(a.check_procs, shell=True, capture_output=True, text=True).stdout.strip()
        step(f"extraction processes left after cancel: {left}")
        if left != "0":
            raise RuntimeError(f"cancel left {left} extraction process(es) running")

    step("run to the end")
    mcp.call("click", role="Button", label_contains="Make the packs")
    wait_text(mcp, "Cancel", 20)
    deadline = time.time() + 1800
    while time.time() < deadline:
        done = nodes(mcp, content_contains="done in")
        failed = nodes(mcp, content_contains="failed")
        if len(done) + len([f for f in failed if "✘" in text_of(f)]) >= len(names) and not nodes(
            mcp, role="Button", label_contains="Cancel"
        ):
            break
        time.sleep(3)
    else:
        raise RuntimeError("the run did not end in time")
    step(f"{len(nodes(mcp, content_contains='done in'))} ROM(s) done")
    shot(mcp, a.shots, "05-done")
    mcp.call("click", content_contains="Details")
    shot(mcp, a.shots, "06-log")
    mcp.call("click", content_contains="Details")

    if a.folder:
        step(f"a whole folder: {a.folder}")
        click_button(mcp, "Clear the list")
        set_field(mcp, "or type a zip or folder path", a.folder)
        click_button(mcp, "Add")
        mcp.call("wait_for", min_steps=3, timeout_secs=10)
        wait_gone(mcp, "Checking", 600)
        shot(mcp, a.shots, "08-folder")

    step("French")
    mcp.call("click", content_contains="English")
    mcp.call("click", content_contains="Français")
    wait_text(mcp, "Créer les packs", 10)
    shot(mcp, a.shots, "07-fr")
    mcp.close()
    step("OK")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # noqa: BLE001 — a test script: report and fail
        print(f"[drive] FAILED: {e}", file=sys.stderr)
        sys.exit(1)
