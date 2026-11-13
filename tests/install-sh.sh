#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
export CARGO_HOME=${CARGO_HOME:-$HOME/.cargo} RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}
export HOME="$work/home" XDG_CONFIG_HOME="$work/config"
mkdir -p "$HOME"
prefix="$work/prefix with spaces"
bash "$root/install.sh" --prefix "$prefix" --systemd --yes
"$prefix/bin/rl" --help | grep -q 'Usage: rl'
"$prefix/bin/rl" --version | grep -q '^rl [0-9]'
[ -x "$prefix/bin/rl" ]
[ -f "$prefix/share/applications/rl.desktop" ]
cmp "$root/packaging/rl.svg" "$prefix/share/icons/hicolor/scalable/apps/rl.svg"
[ -f "$XDG_CONFIG_HOME/systemd/user/rl.service" ]
if grep -q '@BINARY@' "$prefix/share/applications/rl.desktop" "$XDG_CONFIG_HOME/systemd/user/rl.service"; then
    printf 'Unexpanded binary path in an installed template\n' >&2
    exit 1
fi
grep -Fx "Exec=env \"$prefix/bin/rl\" show" "$prefix/share/applications/rl.desktop"
grep -Fx "ExecStart=\"$prefix/bin/rl\"" "$XDG_CONFIG_HOME/systemd/user/rl.service"
bash "$root/install.sh" --prefix "$prefix" --systemd --uninstall --yes
[ -z "$(find "$prefix" "$XDG_CONFIG_HOME" -type f -print -quit)" ]
printf 'PASS install, binary help, template paths and uninstall in a temporary prefix\n'
