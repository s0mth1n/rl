#!/bin/bash
# Starts a compositor from an unpacked Alpine edge rootfs (/tmp/alp/root, bwrap user namespace) on the sway socket.
# WM_CMD runs inside the rootfs; WM_ROOT overrides "--uid 0 --gid 0" for compositors that refuse to run as root.
# Use with RL_NESTED_LAUNCH and tests/nested-wm.sh.
# $1 runtime dir, $2 sway socket path, $3 out dir; WM_CMD = command run inside the alpine rootfs
mkdir -p "$1/cfg"
[ -n "${WM_PRE:-}" ] && eval "$WM_PRE"
exec bwrap --unshare-user ${WM_ROOT:---uid 0 --gid 0} --bind /tmp/alp/root / --dev-bind /dev /dev --proc /proc --bind /tmp /tmp --ro-bind /etc/resolv.conf /etc/resolv.conf \
  --setenv PATH /usr/bin:/bin --setenv HOME /root --setenv XDG_RUNTIME_DIR "$1" --setenv WAYLAND_DISPLAY "$2" \
  --setenv WLR_BACKENDS wayland --setenv WLR_RENDERER pixman --setenv WLR_NO_HARDWARE_CURSORS 1 \
  --setenv LIBGL_ALWAYS_SOFTWARE 1 --setenv XDG_CONFIG_HOME "$1/cfg" \
  /bin/sh -c "$WM_CMD" > "$3/nested.log" 2>&1
