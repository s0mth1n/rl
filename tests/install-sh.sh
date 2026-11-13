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
"$prefix/bin/zl" --help | grep -q 'Usage: zl'
"$prefix/bin/zl" --version | grep -q '^zl [0-9]'
[ -x "$prefix/bin/zl" ]
[ -f "$prefix/share/applications/zl.desktop" ]
cmp "$root/packaging/zl.svg" "$prefix/share/icons/hicolor/scalable/apps/zl.svg"
[ -f "$XDG_CONFIG_HOME/systemd/user/zl.service" ]
if grep -q '@BINARY@' "$prefix/share/applications/zl.desktop" "$XDG_CONFIG_HOME/systemd/user/zl.service"; then
    printf 'Unexpanded binary path in an installed template\n' >&2
    exit 1
fi
grep -Fx "Exec=env \"$prefix/bin/zl\" show" "$prefix/share/applications/zl.desktop"
grep -Fx "ExecStart=\"$prefix/bin/zl\"" "$XDG_CONFIG_HOME/systemd/user/zl.service"
bash "$root/install.sh" --prefix "$prefix" --systemd --uninstall --yes
[ -z "$(find "$prefix" "$XDG_CONFIG_HOME" -type f -print -quit)" ]
printf 'PASS install, binary help, template paths and uninstall in a temporary prefix\n'
