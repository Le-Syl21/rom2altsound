#!/usr/bin/env python3
"""Opens each page in Chromium and Firefox (headless), plays with it the way a visitor
would, and reports JavaScript errors, failed requests and console errors.

Usage: check_pages.py --shots DIR [--listen PAGE.html]... [--site DIR]

--listen  a pack's listening page, opened from disk (file://), as people open it
--site    the catalog site's folder (docs/), served over HTTP as GitHub Pages serves it;
          its index.html and fr/index.html are checked

Exit code 1 when any page reported an error.
"""

import argparse
import functools
import http.server
import json
import os
import re
import sys
import threading
import wave
import zipfile

from playwright.sync_api import sync_playwright


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def serve(folder):
    handler = functools.partial(QuietHandler, directory=folder)
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def watch(page, problems, engine):
    page.on("pageerror", lambda e: problems.append(f"page error: {e}"))
    page.on(
        "console",
        lambda m: problems.append(f"console {m.type}: {m.text}")
        if m.type in ("error", "warning") and not noise(engine, m.text) else None,
    )
    page.on(
        "requestfailed",
        lambda r: problems.append(f"request failed: {r.url} ({r.failure})")
        # A play stopped by the next one aborts its media request: not an error.
        # (WebKit says "Load request cancelled", also for a blob: of the replacement dialog.)
        if not (r.resource_type == "media" or r.url.startswith("blob:"))
        or not any(w in (r.failure or "").upper() for w in ("ABORT", "CANCELLED")) else None,
    )
    page.on(
        "response",
        lambda r: problems.append(f"HTTP {r.status}: {r.url}") if r.status >= 400 else None,
    )
    page.on("dialog", lambda d: d.accept())


# Firefox in a container has no sound output: every play fails in its audio sink, which
# says nothing about the page. The files themselves are decoded with Web Audio instead.
SINK_NOISE = ("PulseAudio", "MEDIASINK", "could not be decoded")


def noise(engine, text):
    return engine == "firefox" and any(n in text for n in SINK_NOISE)


DECODE_JS = """async (b64) => {
  const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
  const ctx = new OfflineAudioContext(2, 44100, 44100);
  try {
    const buf = await ctx.decodeAudioData(bytes.buffer);
    return buf.duration > 0 ? null : 'decoded to nothing';
  } catch (e) { return String(e); }
}"""


def check_decodes(page, problems, folder, limit=6):
    """Decodes the pack's first WAV files in the browser (Web Audio)."""
    import base64

    wavs = sorted(f for f in os.listdir(folder) if f.lower().endswith(".wav"))[:limit]
    for w in wavs:
        with open(os.path.join(folder, w), "rb") as f:
            err = page.evaluate(DECODE_JS, base64.b64encode(f.read()).decode())
        if err:
            problems.append(f"{w}: the browser cannot decode it: {err}")
    return len(wavs)


def audio_error(page):
    return page.evaluate(
        "() => { const a = document.getElementById('audio');"
        " return a && a.error ? 'audio error ' + a.error.code + ' ' + (a.currentSrc || '') : null; }"
    )


def exercise_listen(page, problems, shots, name):
    """A pack's listening page (or the batch's page of every ROM: then each ROM's page it
    links to): sort, filter, play, rename, export and import the names, A/B compare."""
    page.wait_for_load_state("load")
    links = page.locator("#index a")
    if page.locator("#list tr.row").count() == 0 and links.count():
        hrefs = [links.nth(i).get_attribute("href") for i in range(links.count())]
        base = page.url.rsplit("/", 1)[0] + "/"
        for h in hrefs:
            page.goto(base + h, wait_until="load")
            exercise_listen(page, problems, shots, name)
        return f"index of {len(hrefs)} ROM(s)"
    # Silent commands are listed but hidden ("With sound" is ticked).
    page.wait_for_selector("#list tr.row:not([hidden])", timeout=15000)
    rows = page.locator("#list tr.row").count()
    for i in range(page.locator("#cols button.sort").count()):
        b = page.locator("#cols button.sort").nth(i)
        b.click()
        b.click()
    page.fill("#q", "a")
    page.wait_for_timeout(200)
    page.press("#q", "Escape")
    page.fill("#q", "")
    opts = page.locator("#type option")
    for i in range(opts.count()):
        page.select_option("#type", index=i)
    page.select_option("#type", index=0)
    for box in ("#loops", "#sound"):
        page.click(box)
        page.click(box)
    # Play a few sounds with the buttons, then with the keyboard.
    plays = page.locator("#list tr.row:not([hidden]) td.btns button:not(.ab):not(.rp)")
    for i in range(min(plays.count(), 4)):
        plays.nth(i).click()
        page.wait_for_timeout(500)
        if (err := audio_error(page)) and page.context.browser.browser_type.name != "firefox":
            problems.append(err)
    # A sound with several files (a loop: its body, its intro + one cycle...): each one.
    multi = page.locator("#list tr.row:not([hidden])").filter(
        has=page.locator("td.btns button:not(.ab):not(.rp) + button:not(.ab):not(.rp)")
    )
    if multi.count():
        btns = multi.first.locator("td.btns button:not(.ab):not(.rp)")
        for i in range(btns.count()):
            btns.nth(i).click()
            page.wait_for_timeout(300)
    # A twin's link goes to the sound it shares its audio with.
    twin = page.locator("#list button.link")
    if twin.count():
        twin.first.click()
    first = page.locator("#list tr.row:not([hidden])").first
    first.focus()
    page.keyboard.press("ArrowDown")
    page.keyboard.press("Enter")
    page.wait_for_timeout(500)
    if (err := audio_error(page)) and page.context.browser.browser_type.name != "firefox":
        problems.append(err)
    folder = os.path.dirname(page.url.removeprefix("file://"))
    decoded = check_decodes(page, problems, folder)
    # Rename one, export the names, import them back, forget the edits.
    page.keyboard.press("F2")
    edit = page.locator("#list input")
    if edit.count():
        edit.first.fill("Test name")
        edit.first.press("Enter")
    with page.expect_download(timeout=5000) as dl:
        page.get_by_role("button", name="Export names").click()
    csv = os.path.join(shots, f"{name}-names.csv")
    dl.value.save_as(csv)
    page.locator("#names-input").set_input_files(csv)
    page.wait_for_timeout(300)
    page.get_by_role("button", name="Clear my edits").click()
    # A/B: compare with the folder itself, flip with b.
    page.fill("#cmp", "./")
    page.press("#cmp", "Enter")
    first.focus()
    page.keyboard.press("b")
    page.wait_for_timeout(500)
    page.keyboard.press("b")
    page.fill("#cmp", "")
    page.press("#cmp", "Enter")
    page.keyboard.press("/")
    replaced = exercise_replace(page, problems, shots, name, folder)
    return f"{rows} row(s), {decoded} file(s) decoded, {replaced}"


# Chromium: a folder in memory behind showDirectoryPicker (the real picker is a native
# dialog), seeded with the pack's text files; window.__fs holds what the page writes.
MOCK_FOLDER_JS = """(seed) => {
  window.__fs = {};
  for (const [k, v] of Object.entries(seed.files)) window.__fs[k] = new Blob([v]);
  const nf = () => new DOMException('not found', 'NotFoundError');
  const dir = (prefix, name) => ({
    kind: 'directory', name,
    async getDirectoryHandle(n, o) {
      const p = prefix + n + '/';
      if (!(o && o.create) && !Object.keys(window.__fs).some(k => k.startsWith(p))) throw nf();
      return dir(p, n);
    },
    async getFileHandle(n, o) {
      const p = prefix + n;
      if (!(p in window.__fs)) { if (!(o && o.create)) throw nf(); window.__fs[p] = new Blob([]); }
      return {
        kind: 'file', name: n,
        async getFile() { return new File([window.__fs[p]], n); },
        async createWritable() {
          const parts = [];
          return { async write(b) { parts.push(b); }, async close() { window.__fs[p] = new Blob(parts); } };
        },
      };
    },
    async removeEntry(n) { if (!((prefix + n) in window.__fs)) throw nf(); delete window.__fs[prefix + n]; },
    async requestPermission() { return 'granted'; },
  });
  window.showDirectoryPicker = async () => dir('', seed.name);
}"""

READ_FS_JS = """async () => {
  const out = {};
  for (const [k, b] of Object.entries(window.__fs)) out[k] = k.endsWith('.wav') ? b.size : await b.text();
  return out;
}"""


def quieter(src, dst, factor):
    """A copy of a 16-bit WAV file, `factor` times as loud."""
    import array

    with wave.open(src, "rb") as r:
        params = r.getparams()
        a = array.array("h", r.readframes(r.getnframes()))
    a = array.array("h", (int(x * factor) for x in a))
    with wave.open(dst, "wb") as w:
        w.setparams(params)
        w.writeframes(a.tobytes())


def page_data(folder):
    with open(os.path.join(folder, "index.html"), encoding="utf-8") as f:
        t = f.read()
    return json.loads(re.search(r'id="data">(.*?)</script>', t, re.S).group(1))


def wait_note(page, problems, what):
    try:
        page.wait_for_function(
            "() => /AltSound will play|cannot read/.test(document.getElementById('rep-note').textContent)",
            timeout=20000,
        )
    except Exception:  # noqa: BLE001
        problems.append(f"replace: {what}: no measurement")
    return page.locator("#rep-note").text_content()


def exercise_replace(page, problems, shots, name, folder):
    """Replaces a sound with its own file (the page must measure it as loud as the
    manifest says), then with a copy 20 dB quieter (raised to its maximum into a WAV copy,
    lowered by GAIN), changes the volume of another, and saves: into a folder (Chromium,
    File System Access) or as a zip (the others)."""
    d = page_data(folder)
    cands = [s for s in d["sounds"] if s.get("rows") and s.get("factory_file") and s.get("level_lufs")
             and s.get("pack", {}).get("kind") == "one_shot" and (s.get("duration") or 0) > 1]
    if len(cands) < 2:
        return "nothing to replace"
    a, b = cands[0], cands[1]
    # No compare folder: its A/B buttons would appear (and move ⇄) when the box loses focus.
    page.fill("#cmp", "")
    page.press("#cmp", "Enter")
    for box in ("#loops", "#changed"):
        if page.is_checked(box):
            page.click(box)

    def row(s):
        return page.locator(f"#s-{d['sounds'].index(s)}")

    row(a).locator("button.rp").click()
    page.wait_for_selector("#rep[open]")
    page.set_input_files("#rep-input", os.path.join(folder, a["factory_file"]))
    note = wait_note(page, problems, "own file")
    if "Same loudness" not in note or page.is_checked("#rep-raise"):
        problems.append(f"replace {a['id']} by its own file: {note}")
    q = os.path.join(shots, f"{name}-quiet.wav")
    quieter(os.path.join(folder, a["factory_file"]), q, 0.1)
    page.set_input_files("#rep-input", q)
    page.wait_for_function("() => /quiet\\.wav:.*LUFS/.test(document.getElementById('rep-file').textContent)", timeout=20000)
    note = wait_note(page, problems, "quiet file")
    gain = page.locator("#rep-gainv").text_content()
    if "Same loudness" not in note or not page.is_checked("#rep-raise") or ".wav (a WAV copy, raised" not in note:
        problems.append(f"replace {a['id']} by a quieter copy: {note} ({gain})")
    for btn in ("#rep-a", "#rep-b", "#rep-ab"):
        page.click(btn)
        page.wait_for_timeout(150)
    page.screenshot(path=f"{shots}/{name}-replace.png")
    page.click("#rep-ok")
    if row(a).locator(".tag.rep").count() != 1:
        problems.append(f"replace {a['id']}: no mark on the row")
    # Volume only.
    row(b).locator("button.rp").click()
    page.evaluate("() => { const g = document.getElementById('rep-gain'); g.value = '-6'; g.dispatchEvent(new Event('input')); }")
    page.click("#rep-ok")
    if "volume 50%" not in (row(b).locator(".tag.rep").text_content() or ""):
        problems.append(f"volume of {b['id']}: no mark 'volume 50%'")
    page.click("#changed")
    shown = page.locator("#list tr.row:not([hidden])").count()
    page.click("#changed")
    if shown != 2:
        problems.append(f"Changed only shows {shown} rows, not 2")
    ga = int(re.search(r"GAIN (\d+)", gain).group(1))
    if page.evaluate("() => typeof window.__fs === 'object'"):
        page.click("text=Save in the pack folder...")
        page.wait_for_function("() => /saved in/.test(document.getElementById('rep-status').textContent)", timeout=10000)
        fs = page.evaluate(READ_FS_JS)
        wav = f"replace/{a['rows'][0]}-{name}-quiet.wav"
        rec = fs.get("replacements.csv", "")
        if not fs.get(wav, 0) > 44:
            problems.append(f"folder: {wav} not written ({sorted(fs)})")
        if f"{a['rows'][0]},{wav},{ga}," not in rec or f"{b['rows'][0]},,50," not in rec:
            problems.append(f"folder: replacements.csv: {rec!r}")
        for csv in ("altsound.csv", "g-sound.csv"):
            if wav not in fs.get(csv, ""):
                problems.append(f"folder: {csv} does not play {wav}")
        if '"replacements":[{' not in fs.get("index.html", ""):
            problems.append("folder: index.html has not the new record")
        # LUFS and OFFSET: the file's loudness, matched to the ROM's sound (OFFSET about 0).
        row_a = next((l.split(",") for l in rec.splitlines() if l.startswith(a["rows"][0] + ",")), [])
        try:
            if len(row_a) != 6 or abs(float(row_a[5])) > 0.3 or float(row_a[4]) > -1:
                raise ValueError
        except ValueError:
            problems.append(f"folder: replacements.csv LUFS/OFFSET of {a['id']}: {row_a}")
        # Opened again, without the file: the page knows its loudness from the record.
        row(a).locator("button.rp").click()
        reopened = page.locator("#rep-note").text_content()
        page.click("#rep-cancel")
        if "Same loudness" not in reopened:
            problems.append(f"reopened {a['id']}: {reopened}")
        # Back to the ROM's sound: its row as extracted.
        row(b).locator("button.rp").click()
        page.click("#rep-revert")
        page.wait_for_function("() => /saved in/.test(document.getElementById('rep-status').textContent)", timeout=10000)
        fs = page.evaluate(READ_FS_JS)
        line = next((l for l in fs["altsound.csv"].splitlines() if l.startswith(b["rows"][0] + ",")), "")
        if line.split(",")[3] != "100" or not line.endswith("," + b["factory_file"]):
            problems.append(f"revert {b['id']}: altsound.csv row {line}")
        return f"replaced {a['id']} (GAIN {ga}), saved in the folder"
    with page.expect_download(timeout=10000) as dl:
        page.click("text=Download changes")
    z = os.path.join(shots, f"{name}-replacements.zip")
    dl.value.save_as(z)
    with zipfile.ZipFile(z) as zf:
        names = zf.namelist()
        rec = zf.read("replacements.csv").decode()
        bad = zf.testzip()
    wav = f"replace/{a['rows'][0]}-{name}-quiet.wav"
    if bad or wav not in names or f"{a['rows'][0]},{wav},{ga}," not in rec:
        problems.append(f"zip: {names} {rec!r} {bad}")
    return f"replaced {a['id']} (GAIN {ga}), zip {os.path.basename(z)}"


def exercise_site(page, problems, shots, name, catalog):
    """The catalog: every filter, every sort, more rows, and the detail of one entry of
    each board family (and of the special cases)."""
    page.wait_for_selector("#rows tr", timeout=20000)
    rows = page.locator("#rows tr").count()
    page.fill("#q", "afm")
    page.wait_for_timeout(300)
    page.fill("#q", "")
    for sel in ("#family", "#support", "#maker"):
        n = page.locator(f"{sel} option").count()
        for i in range(n):
            page.select_option(sel, index=i)
        page.select_option(sel, index=0)
    for box in ("#onlyIds", "#onlyFound", "#hideNoVpx"):
        if page.locator(box).is_visible():
            page.click(box)
            page.click(box)
    for i in range(page.locator("th[data-k] button").count()):
        b = page.locator("th[data-k] button").nth(i)
        b.click()
        b.click()
    if page.locator("#more").is_visible():
        page.click("#more")
    keys, seen = [], set()
    for e in catalog["entries"]:
        kind = (e.get("label"), e.get("id") is None, e.get("vpx_playable", True),
                (e.get("sounds") or {}).get("kind"))
        if kind not in seen:
            seen.add(kind)
            keys.append(e["key"])
    for k in keys + ["no-such-entry"]:
        page.evaluate("k => { location.hash = encodeURIComponent(k); }", k)
        page.wait_for_timeout(60)
        if k != "no-such-entry" and page.locator("#detail h2").count() == 0:
            problems.append(f"detail of {k}: nothing shown")
        if page.locator("#close").count():
            page.click("#close")
    return f"{rows} row(s), {len(keys)} detail(s)"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--shots", required=True)
    ap.add_argument("--listen", action="append", default=[])
    ap.add_argument("--site")
    a = ap.parse_args()
    os.makedirs(a.shots, exist_ok=True)

    targets = [("listen", "file://" + os.path.abspath(p), exercise_listen) for p in a.listen]
    if a.site:
        httpd = serve(a.site)
        base = f"http://127.0.0.1:{httpd.server_port}/"
        with open(os.path.join(a.site, "catalog.json"), encoding="utf-8") as f:
            catalog = json.load(f)
        site = functools.partial(exercise_site, catalog=catalog)
        targets += [("site", base, site), ("site-fr", base + "fr/", site)]

    failed = 0
    with sync_playwright() as pw:
        for engine in (pw.chromium, pw.firefox, pw.webkit):
            # Firefox: no sound device in the container, so a null audio output (else every
            # play fails with a media sink error that says nothing about the page).
            prefs = {"media.cubeb.force_null_context": True} if engine.name == "firefox" else None
            browser = engine.launch(firefox_user_prefs=prefs) if prefs else engine.launch()
            for n, (kind, url, exercise) in enumerate(targets):
                ctx = browser.new_context(viewport={"width": 1280, "height": 900}, accept_downloads=True)
                if engine.name == "chromium":
                    ctx.grant_permissions(["clipboard-read", "clipboard-write"])
                    if kind == "listen":
                        folder = os.path.dirname(os.path.abspath(url.removeprefix("file://")))
                        seed = {"name": os.path.basename(folder), "files": {}}
                        for f in ("manifest.json", "altsound.csv", "g-sound.csv", "index.html"):
                            if os.path.isfile(os.path.join(folder, f)):
                                with open(os.path.join(folder, f), encoding="utf-8") as fh:
                                    seed["files"][f] = fh.read()
                        ctx.add_init_script(f"({MOCK_FOLDER_JS})({json.dumps(seed)})")
                page = ctx.new_page()
                problems = []
                watch(page, problems, engine.name)
                name = f"{engine.name}-{n}-{kind}"
                try:
                    page.goto(url, wait_until="load")
                    what = exercise(page, problems, a.shots, name)
                except Exception as e:  # noqa: BLE001 — report and go on
                    problems.append(f"exercise: {e}")
                    what = "?"
                page.screenshot(path=f"{a.shots}/{name}.png", full_page=False)
                status = "OK" if not problems else f"{len(problems)} problem(s)"
                print(f"{engine.name:8} {kind:8} {url}: {what}, {status}")
                for p in problems:
                    print(f"    {p}")
                failed += bool(problems)
                ctx.close()
            browser.close()
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
