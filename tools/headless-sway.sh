#!/usr/bin/env bash
# Run a command with private headless Wayland tools, without installing system packages.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cache=${RL_HEADLESS_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/rl-headless}
mkdir -p "$cache/lists/partial" "$cache/debs" "$cache/root"
if [ ! -f "$cache/ready" ]; then
    command -v apt-get >/dev/null
    command -v dpkg-deb >/dev/null
    apt-get -o "Dir::State::lists=$cache/lists" -o Debug::NoLocking=1 update
    packages=$(apt-get -o "Dir::State::lists=$cache/lists" -o Debug::NoLocking=1 \
        -s --no-install-recommends install sway grim wl-clipboard | awk '$1 == "Inst" {print $2}')
    (
        cd "$cache/debs"
        # Download the entry points even when they are already installed on the host.
        # The solver adds the shared libraries missing from this host.
        apt-get -o "Dir::State::lists=$cache/lists" download sway swaybg grim wl-clipboard $packages
        for package in ./*.deb; do dpkg-deb -x "$package" "$cache/root"; done
    )
fi
export PATH="$cache/root/usr/bin:$PATH"
export LD_LIBRARY_PATH="$cache/root/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
sway --version
touch "$cache/ready"
if [ "$#" = 0 ]; then
    echo 'usage: tools/headless-sway.sh COMMAND [ARG...]' >&2
    echo 'example: tools/headless-sway.sh bash tests/headless.sh bash tests/outside-click.sh' >&2
    exit 2
fi
cd "$root"
exec "$@"
