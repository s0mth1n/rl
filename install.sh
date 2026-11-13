#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./install.sh [--prefix DIR] [--systemd] [--yes] [--skip-deps] [--uninstall]

Build and install rl to ~/.local, or remove its installed files.
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
unit="$config/systemd/user/rl.service"
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

printf 'rl installer\n'

if ! "$yes"; then
    action=Install
    "$uninstall" && action=Uninstall
    ask "$action rl in $prefix?" || exit 1
fi

# Only prefix writes use this sudo command, never the build or user service.
writer=()
writable=true
for destination in "$prefix/bin/rl" "$prefix/share/applications/rl.desktop" \
    "$prefix/share/icons/hicolor/scalable/apps/rl.svg" "$prefix/libexec/rl"; do
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
        if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet rl.service; then
            ask 'Stop and disable the rl user service before removing it?' || fail 'Service left installed and running.'
            systemctl --user disable --now rl.service
        fi
        rm -f -- "$unit"
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --user daemon-reload || printf 'User service manager unavailable; reload it when the session starts.\n' >&2
        fi
    fi
    "${writer[@]}" rm -f -- "$prefix/bin/rl" "$prefix/share/applications/rl.desktop" \
        "$prefix/share/icons/hicolor/scalable/apps/rl.svg" "$prefix/libexec/rl"
    printf 'Removed rl from %s. Launcher config and history were kept.\n' "$prefix"
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

printf 'Building rl...\n'
(cd "$root" && cargo build --release --locked)
target=${CARGO_TARGET_DIR:-$root/target}
[[ "$target" == /* ]] || target="$root/$target"
[[ -x "$target/release/rl" ]] || fail 'Cargo did not produce target/release/rl; cross-target builds are not supported by this installer.'

# Desktop entries and systemd both treat percent signs as substitutions.
binary="$prefix/bin/rl"
desktop=${binary//\\/\\\\}
desktop=${desktop//\"/\\\"}
desktop=${desktop//\$/\\\$}
desktop=${desktop//\`/\\\`}
desktop=${desktop//%/%%}
desktop=${desktop//\\/\\\\}
escape_sed() { sed 's/[\\&|]/\\&/g'; }
replacement=$(printf '"%s"' "$desktop" | escape_sed)
sed "s|@BINARY@|$replacement|g" "$root/packaging/rl.desktop" > "$work/rl.desktop"
if [[ -n "${emoji_pattern:-}" && -z "${EMOJI_FONT:-}" ]]; then
    # shellcheck disable=SC2016
    printf '#!/usr/bin/env bash\nif [[ -z "${EMOJI_FONT:-}" ]]; then\n    export EMOJI_FONT=%q\nfi\nexec %q "$@"\n' \
        "$emoji_pattern" "$prefix/libexec/rl" > "$work/rl-wrapper"
    "${writer[@]}" install -Dm755 "$target/release/rl" "$prefix/libexec/rl"
    "${writer[@]}" install -Dm755 "$work/rl-wrapper" "$binary"
else
    "${writer[@]}" install -Dm755 "$target/release/rl" "$binary"
    "${writer[@]}" rm -f -- "$prefix/libexec/rl"
fi
"${writer[@]}" install -Dm644 "$work/rl.desktop" "$prefix/share/applications/rl.desktop"
"${writer[@]}" install -Dm644 "$root/packaging/rl.svg" "$prefix/share/icons/hicolor/scalable/apps/rl.svg"
"$binary" --version
"$binary" --help > "$work/help"
grep -q 'Usage: rl' "$work/help" || fail 'Installed binary failed its help check.'

if "$systemd"; then
    service=${binary//\\/\\\\}
    service=${service//\"/\\\"}
    service=${service//%/%%}
    service=${service//\$/\$\$}
    replacement=$(printf '"%s"' "$service" | escape_sed)
    sed "s|@BINARY@|$replacement|g" "$root/packaging/rl.service" > "$work/rl.service"
    install -Dm644 "$work/rl.service" "$unit"
    printf 'Installed user unit: %s\n' "$unit"
    if ! "$yes" && ask 'Enable and start the rl user service now?'; then
        need systemctl 'The unit was installed; enable it later in a systemd user session.'
        systemctl --user daemon-reload
        systemctl --user enable --now rl.service
    else
        printf 'Service not enabled. To enable it later: systemctl --user daemon-reload && systemctl --user enable --now rl.service\n'
    fi
fi
printf 'Installed rl in %s. Add %s/bin to PATH if needed.\n' "$prefix" "$prefix"
printf 'Use only one daemon autostart method. Compositor config examples:\n'
printf '  niri: spawn-at-startup "%s"\n' "${binary//\"/\\\"}"
printf '  sway: exec "%s"\n' "${binary//\"/\\\"}"
printf '  hyprland: exec-once = "%s"\n' "${binary//\"/\\\"}"
