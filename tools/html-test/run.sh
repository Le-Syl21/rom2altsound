#!/usr/bin/env bash
# Checks rom2altsound's HTML pages in headless Chromium and Firefox (see check_pages.py).
#
#   tools/html-test/run.sh [PACK_DIR...]
#
# Each PACK_DIR is a pack folder holding index.html (its listening page; a pack's folder
# from tools/gui-test/run.sh or the command line), checked from disk as people open it.
# The catalog site (docs/) is always checked, served over HTTP.
# Screenshots: $WORK/html-shots (default WORK: ~/.cache/claude-work/r2a-html-test).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${WORK:-$HOME/.cache/claude-work/r2a-html-test}
image=r2a-test-html
name=r2a-test-html-$$

docker build -q -t "$image" "$here" >/dev/null
rm -rf "$work/html-shots"
mkdir -p "$work/html-shots"

mounts=(-v "$repo/docs:/site:ro" -v "$work/html-shots:/shots")
args=(--shots /shots --site /site)
i=0
for d in "$@"; do
  d=$(cd "$d" && pwd)
  mounts+=(-v "$d:/pack$i:ro")
  args+=(--listen "/pack$i/index.html")
  i=$((i + 1))
done

trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
timeout 600 docker run --name "$name" --init --ipc=host \
  --label com.centurylinklabs.watchtower.enable=false \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  "${mounts[@]}" "$image" "${args[@]}"
