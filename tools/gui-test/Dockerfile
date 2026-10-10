# A display for rom2altsound-gui on a machine that has none: Xvfb (a virtual X server)
# and Mesa's llvmpipe (OpenGL in software), plus what eframe/winit load at run time.
# The program itself is built on the host (same Ubuntu release, so the same glibc) and
# mounted in: see run.sh. x11vnc lets a person watch or use the window (port 5900).
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      xvfb x11vnc xauth \
      libgl1 libegl1 libgl1-mesa-dri libglx-mesa0 \
      libx11-6 libx11-xcb1 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libxkbcommon-x11-0 \
      libwayland-client0 libwayland-egl1 \
      fonts-dejavu-core ca-certificates procps \
 && rm -rf /var/lib/apt/lists/*

# Software OpenGL; no real GPU in the container.
ENV LIBGL_ALWAYS_SOFTWARE=1 \
    GALLIUM_DRIVER=llvmpipe \
    DISPLAY=:99 \
    XDG_RUNTIME_DIR=/tmp/xdg

# The X socket folder, for an X server started by any user (the tests run as the host's).
RUN mkdir -m 1777 /tmp/.X11-unix

COPY entrypoint.sh /usr/local/bin/r2a-entrypoint
RUN chmod +x /usr/local/bin/r2a-entrypoint
ENTRYPOINT ["/usr/local/bin/r2a-entrypoint"]
