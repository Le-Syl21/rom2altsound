#!/usr/bin/env bash
# Runs rom2altsound-gui in a container with a virtual display and drives it through
# egui_mcp (see drive.py): add ROMs, start, cancel, run to the end, screenshots.
#
#   tools/gui-test/run.sh                  build, run the scenario, remove the container
#   VNC=1 KEEP=1 tools/gui-test/run.sh     also serve the window on 127.0.0.1:5900 (any VNC
#                                          viewer) and leave it running after the scenario
#
# Environment:
#   ROMS      the ROM folder, mounted read-only (default: ~/roms)
#   ZIPS      the zips of the scenario, by name (default: a few quick early-board games)
#   WORK      output, PinMAME's cache and screenshots (default: ~/.cache/claude-work/r2a-gui-test)
#   EGUI_MCP  the egui-mcp server (`cargo install egui_mcp`; default: egui-mcp on PATH)
#   NO_BUILD  1: use the already built target/release/rom2altsound-gui
#
# The window program is built on the host and mounted in: the image is the same Ubuntu
# release as the build machine (24.04), so the same glibc.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
roms=${ROMS:-$HOME/roms}
work=${WORK:-$HOME/.cache/claude-work/r2a-gui-test}
zips=${ZIPS:-"xenon spyhuntr vikingb bsmt2000"}
mcp=${EGUI_MCP:-egui-mcp}
port=${PORT:-15719}
image=r2a-test-gui
name=r2a-test-gui-$$

if [ "${NO_BUILD:-0}" != 1 ]; then
  (cd "$repo" && cargo build --release -j2 -p rom2altsound-gui --features inspection)
fi
bin="$repo/target/release/rom2altsound-gui"
[ -x "$bin" ] || { echo "missing $bin" >&2; exit 1; }

docker build -q -t "$image" "$here" >/dev/null
rm -rf "$work/out" "$work/shots"
mkdir -p "$work/out" "$work/cache" "$work/shots"

cleanup() {
  if [ "${KEEP:-0}" = 1 ]; then
    echo "left running: $name (docker rm -f $name)"
  else
    docker rm -f "$name" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

vnc=()
[ "${VNC:-0}" = 1 ] && vnc=(-p 127.0.0.1:5900:5900 -e VNC=1)
docker run -d --name "$name" \
  --label com.centurylinklabs.watchtower.enable=false \
  --user "$(id -u):$(id -g)" \
  -e HOME=/tmp -e XDG_CACHE_HOME=/cache \
  -e EGUI_INSPECTION=0.0.0.0:5719 \
  -p "127.0.0.1:$port:5719" "${vnc[@]}" \
  -v "$bin:/usr/local/bin/rom2altsound-gui:ro" \
  -v "$roms:/roms:ro" \
  -v "$work/out:/out" -v "$work/cache:/cache" \
  "$image" rom2altsound-gui >/dev/null

rom_args=()
for z in $zips; do rom_args+=(--rom "/roms/$z.zip"); done
# The extraction processes still alive (the window itself excluded): none after a cancel.
procs="docker exec $name sh -c 'ps -eo args | grep -c \"^rom2altsound-gui .*--in-process\" || true'"
status=0
timeout 2400 python3 "$here/drive.py" --mcp "$mcp" --port "$port" --shots "$work/shots" \
  --out /out/packs --folder /roms --check-procs "$procs" "${rom_args[@]}" || status=$?
docker logs "$name" > "$work/container.log" 2>&1 || true
echo "screenshots: $work/shots   packs: $work/out/packs   log: $work/container.log"
exit $status
