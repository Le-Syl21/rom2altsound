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
import sys
import threading

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
        if not (r.resource_type == "media" and "ABORT" in (r.failure or "").upper()) else None,
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
    plays = page.locator("#list tr.row:not([hidden]) td.btns button:not(.ab)")
    for i in range(min(plays.count(), 4)):
        plays.nth(i).click()
        page.wait_for_timeout(500)
        if (err := audio_error(page)) and page.context.browser.browser_type.name != "firefox":
            problems.append(err)
    # A sound with several files (a loop: its body, its intro + one cycle...): each one.
    multi = page.locator("#list tr.row:not([hidden])").filter(
        has=page.locator("td.btns button:not(.ab) + button:not(.ab)")
    )
    if multi.count():
        btns = multi.first.locator("td.btns button:not(.ab)")
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
    page.locator("input[type=file]").set_input_files(csv)
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
    return f"{rows} row(s), {decoded} file(s) decoded"


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
        for engine in (pw.chromium, pw.firefox):
            # Firefox: no sound device in the container, so a null audio output (else every
            # play fails with a media sink error that says nothing about the page).
            prefs = {"media.cubeb.force_null_context": True} if engine.name == "firefox" else None
            browser = engine.launch(firefox_user_prefs=prefs) if prefs else engine.launch()
            for n, (kind, url, exercise) in enumerate(targets):
                ctx = browser.new_context(viewport={"width": 1280, "height": 900}, accept_downloads=True)
                if engine.name == "chromium":
                    ctx.grant_permissions(["clipboard-read", "clipboard-write"])
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
