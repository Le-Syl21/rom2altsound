#!/bin/sh
# Starts the virtual display (and, with VNC=1, a VNC server on it), then the program
# given as arguments (rom2altsound-gui).
set -e
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
Xvfb :99 -screen 0 "${SCREEN:-1280x900x24}" -nolisten tcp &
i=0
while [ ! -e /tmp/.X11-unix/X99 ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
if [ "${VNC:-0}" = 1 ]; then
  x11vnc -display :99 -forever -shared -nopw -quiet -rfbport 5900 &
fi
exec "$@"
