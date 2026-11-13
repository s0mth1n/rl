#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./install.sh [--prefix DIR] [--systemd] [--yes] [--skip-deps] [--uninstall]

Build and install zl to ~/.local, or remove its installed files.
  --prefix DIR  Install bin/ and share/ under DIR (default: ~/.local)
  --systemd     Also install/remove the user service in $XDG_CONFIG_HOME/systemd/user
  --yes         Approve package installation and the install confirmation
  --skip-deps   Do not install system packages; still verify dependencies
  --uninstall   Remove installed files without building; keep launcher data/config
  --help        Show this help

Rustup, prefix-write sudo and service activation require separate prompts, even
with --yes. The build never runs through sudo. Uninstall keeps shared dependencies.
EOF
}
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
ask() {
    local answer
    printf '%s [y/N] ' "$1" >&2
    read -r answer || return 1
    [[ "$answer" == y || "$answer" == Y || "$answer" == yes ]]
}
need() { command -v "$1" >/dev/null 2>&1 || fail "$1 is required. $2"; }

prefix=${HOME:?HOME must be set}/.local
systemd=false
yes=false
uninstall=false
skip_deps=false
while (($#)); do
    case "$1" in
        --prefix)
            if (($# < 2)) || [[ -z "$2" || "$2" == --* ]]; then
                fail '--prefix needs a directory'
            fi
            prefix=$2; shift 2 ;;
        --systemd) systemd=true; shift ;;
        --yes) yes=true; shift ;;
        --skip-deps) skip_deps=true; shift ;;
        --uninstall) uninstall=true; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1 (see --help)" ;;
    esac
done
[[ "$prefix" == /* ]] || prefix="$PWD/$prefix"
while [[ "$prefix" != / && "$prefix" == */ ]]; do prefix=${prefix%/}; done
[[ "$prefix" != *$'\n'* && "$prefix" != *$'\r'* ]] || fail 'The prefix must not contain line breaks.'
config=${XDG_CONFIG_HOME:-$HOME/.config}
[[ "$config" == /* ]] || fail 'XDG_CONFIG_HOME must be an absolute path.'
unit="$config/systemd/user/zl.service"
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

printf 'zl installer\n'

if ! "$yes"; then
    action=Install
    "$uninstall" && action=Uninstall
    ask "$action zl in $prefix?" || exit 1
fi

# Only prefix writes use this sudo command, never the build or user service.
writer=()
writable=true
for destination in "$prefix/bin/zl" "$prefix/share/applications/zl.desktop" \
    "$prefix/share/icons/hicolor/scalable/apps/zl.svg" "$prefix/libexec/zl"; do
    probe=$(dirname -- "$destination")
    while [[ ! -e "$probe" ]]; do probe=$(dirname -- "$probe"); done
    [[ -d "$probe" ]] || fail "Not a directory: $probe"
    [[ -w "$probe" ]] || writable=false
done
if ! "$writable"; then
    need sudo 'Choose a writable --prefix to install without sudo.'
    ask "$prefix is not writable. Use sudo for installed files only?" || exit 1
    writer=(sudo)
fi

if "$uninstall"; then
    if "$systemd" && [[ -f "$unit" ]]; then
        if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet zl.service; then
            ask 'Stop and disable the zl user service before removing it?' || fail 'Service left installed and running.'
            systemctl --user disable --now zl.service
        fi
        rm -f -- "$unit"
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --user daemon-reload || printf 'User service manager unavailable; reload it when the session starts.\n' >&2
        fi
    fi
    "${writer[@]}" rm -f -- "$prefix/bin/zl" "$prefix/share/applications/zl.desktop" \
        "$prefix/share/icons/hicolor/scalable/apps/zl.svg" "$prefix/libexec/zl"
    printf 'Removed zl from %s. Launcher config and history were kept.\n' "$prefix"
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
# shellcheck source=packaging/install-deps.sh
. "$root/packaging/install-deps.sh"
install_dependencies
ensure_rust
linker=${CC:-cc}
printf 'int main(void) { return 0; }\n' > "$work/link.c"
"$linker" "$work/link.c" -o "$work/link" || fail 'The C toolchain could not link a program. Check CC and your C development libraries.'
if ! command -v systemctl >/dev/null 2>&1 || ! command -v loginctl >/dev/null 2>&1; then
    warn 'No systemd/logind tools found. Built-in power/session actions need systemctl/loginctl; use compositor autostart on non-systemd systems.'
fi
if [[ -z "${TERMINAL:-}" ]] && ! command -v xterm >/dev/null 2>&1; then
    warn 'Set TERMINAL to your terminal executable for Terminal=true applications, or install xterm.'
fi

printf 'Building zl...\n'
(cd "$root" && cargo build --release --locked)
target=${CARGO_TARGET_DIR:-$root/target}
[[ "$target" == /* ]] || target="$root/$target"
[[ -x "$target/release/zl" ]] || fail 'Cargo did not produce target/release/zl; cross-target builds are not supported by this installer.'

# Desktop entries and systemd both treat percent signs as substitutions.
binary="$prefix/bin/zl"
desktop=${binary//\\/\\\\}
desktop=${desktop//\"/\\\"}
desktop=${desktop//\$/\\\$}
desktop=${desktop//\`/\\\`}
desktop=${desktop//%/%%}
desktop=${desktop//\\/\\\\}
escape_sed() { sed 's/[\\&|]/\\&/g'; }
replacement=$(printf '"%s"' "$desktop" | escape_sed)
sed "s|@BINARY@|$replacement|g" "$root/packaging/zl.desktop" > "$work/zl.desktop"
if [[ -n "${emoji_pattern:-}" && -z "${EMOJI_FONT:-}" ]]; then
    # shellcheck disable=SC2016
    printf '#!/usr/bin/env bash\nif [[ -z "${EMOJI_FONT:-}" ]]; then\n    export EMOJI_FONT=%q\nfi\nexec %q "$@"\n' \
        "$emoji_pattern" "$prefix/libexec/zl" > "$work/zl-wrapper"
    "${writer[@]}" install -Dm755 "$target/release/zl" "$prefix/libexec/zl"
    "${writer[@]}" install -Dm755 "$work/zl-wrapper" "$binary"
else
    "${writer[@]}" install -Dm755 "$target/release/zl" "$binary"
    "${writer[@]}" rm -f -- "$prefix/libexec/zl"
fi
"${writer[@]}" install -Dm644 "$work/zl.desktop" "$prefix/share/applications/zl.desktop"
"${writer[@]}" install -Dm644 "$root/packaging/zl.svg" "$prefix/share/icons/hicolor/scalable/apps/zl.svg"
"$binary" --version
"$binary" --help > "$work/help"
grep -q 'Usage: zl' "$work/help" || fail 'Installed binary failed its help check.'

if "$systemd"; then
    service=${binary//\\/\\\\}
    service=${service//\"/\\\"}
    service=${service//%/%%}
    service=${service//\$/\$\$}
    replacement=$(printf '"%s"' "$service" | escape_sed)
    sed "s|@BINARY@|$replacement|g" "$root/packaging/zl.service" > "$work/zl.service"
    install -Dm644 "$work/zl.service" "$unit"
    printf 'Installed user unit: %s\n' "$unit"
    if ! "$yes" && ask 'Enable and start the zl user service now?'; then
        need systemctl 'The unit was installed; enable it later in a systemd user session.'
        systemctl --user daemon-reload
        systemctl --user enable --now zl.service
    else
        printf 'Service not enabled. To enable it later: systemctl --user daemon-reload && systemctl --user enable --now zl.service\n'
    fi
fi
printf 'Installed zl in %s. Add %s/bin to PATH if needed.\n' "$prefix" "$prefix"
printf 'Use only one daemon autostart method. Compositor config examples:\n'
printf '  niri: spawn-at-startup "%s"\n' "${binary//\"/\\\"}"
printf '  sway: exec "%s"\n' "${binary//\"/\\\"}"
printf '  hyprland: exec-once = "%s"\n' "${binary//\"/\\\"}"
