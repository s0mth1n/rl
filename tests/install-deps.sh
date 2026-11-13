#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
# shellcheck source=packaging/install-deps.sh
. "$root/packaging/install-deps.sh"
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "Missing $1"; }
ask() { [[ "$reply" == yes ]]; }

if (($#)); then
    scenario=$1
    export LOG="$work/calls" INSTALLED="$work/installed" FAKE_RUST="$work/rust"
    mkdir -p "$work/bin"
    export PATH="$work/bin:$PATH" HOME="$work/home" TEST_FONT="$work/font"
    mkdir -p "$HOME"
    printf COLR > "$TEST_FONT"
    cat > "$work/bin/mock" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${0##*/} $*" >> "$LOG"
case "${0##*/}" in
    sudo) exec "$@" ;;
    rustc|cargo)
        if [[ -f "$FAKE_RUST" ]]; then printf '%s 1.99.0\n' "${0##*/}";
        else printf '%s 1.80.0\n' "${0##*/}"; fi ;;
    rustup) touch "$FAKE_RUST" ;;
    fc-cache) ;;
    fc-match) printf '%s' "$TEST_FONT" ;;
    curl)
        [[ "${curl_error:-false}" == false ]] || exit 7
        for output in "$@"; do :; done
        printf 'invalid font download' > "$output" ;;
    *)
        [[ "${break_install:-false}" == false ]] || exit 7
        [[ "${no_effect:-false}" == true ]] || touch "$INSTALLED" ;;
esac
EOF
    chmod +x "$work/bin/mock"
    for program in sudo apt-get dnf pacman zypper apk xbps-install emerge fc-cache fc-match curl rustc cargo rustup; do
        ln -s mock "$work/bin/$program"
    done
    if [[ "$scenario" != font-* ]]; then
        ensure_bitmap_emoji() { :; }
        font_available() { return 0; }
    fi
    yes=true skip_deps=false reply=no
    immutable=false manager=${2:-apt-get} distro='test Linux'
    detect_distro() { :; }
    missing_dependencies() {
        missing=()
        [[ -f "$INSTALLED" ]] || missing=('test dependency')
    }
    case "$scenario" in
        install)
            install_dependencies
            [[ -f "$INSTALLED" ]]
            case "$manager" in
                apt-get) grep -Fx 'apt-get update' "$LOG"; grep -F 'apt-get install -y --no-install-recommends build-essential' "$LOG" ;;
                dnf) grep -F 'dnf install -y gcc glibc-devel' "$LOG" ;;
                pacman) grep -F 'pacman -S --needed --noconfirm base-devel' "$LOG" ;;
                zypper) grep -F 'zypper --non-interactive install gcc' "$LOG" ;;
                apk) grep -F 'apk add build-base pkgconf libxkbcommon-dev libxkbcommon-static' "$LOG" ;;
                xbps-install) grep -F 'xbps-install -S -y base-devel' "$LOG" ;;
                emerge) grep -F 'emerge --ask=n sys-devel/gcc' "$LOG" ;;
            esac
            grep -Fx 'fc-cache -f' "$LOG"
            if ((EUID != 0)); then grep -q '^sudo ' "$LOG"; fi ;;
        already-present)
            touch "$INSTALLED"
            install_dependencies
            [[ ! -e "$LOG" ]] ;;
        decline) yes=false; install_dependencies ;;
        skip) skip_deps=true; install_dependencies ;;
        unknown) manager=; install_dependencies ;;
        nix) manager=nix; install_dependencies ;;
        immutable) immutable=true; install_dependencies ;;
        package-failure) export break_install=true; install_dependencies ;;
        verification-failure) export no_effect=true; install_dependencies ;;
        font-decline) ensure_bitmap_emoji ;;
        font-download-failure) reply=yes; export curl_error=true; ensure_bitmap_emoji ;;
        font-checksum-failure) reply=yes; ensure_bitmap_emoji ;;
        font-override) export EMOJI_FONT=unsupported; ensure_bitmap_emoji ;;
        rust-decline) ensure_rust ;;
        rust-upgrade)
            reply=yes
            ensure_rust
            grep -Fx 'rustup toolchain install stable --profile minimal' "$LOG"
            [[ "$RUSTUP_TOOLCHAIN" == stable ]] ;;
        rust-current)
            touch "$FAKE_RUST"
            ensure_rust
            if grep -q '^rustup ' "$LOG"; then fail 'Changed an adequate toolchain'; fi ;;
        *) fail "Unknown scenario: $scenario" ;;
    esac
    exit 0
fi

for tag in CBDT sbix COLR; do
    printf '%s' "$tag" > "$work/font"
    # shellcheck disable=SC2329
    fc-match() { printf '%s' "$work/font"; }
    if [[ "$tag" == COLR ]]; then
        if font_available emoji; then fail 'Accepted a COLR-only font'; fi
    else
        font_available emoji || fail "Rejected $tag font"
    fi
done
unset -f fc-match

for entry in 'debian::apt-get' 'ubuntu::apt-get' 'linuxmint:ubuntu debian:apt-get' \
    'fedora::dnf' 'rocky:rhel centos fedora:dnf' 'arch::pacman' \
    'endeavouros:arch:pacman' 'opensuse-tumbleweed:opensuse suse:zypper' \
    'alpine::apk' 'void::xbps-install' 'gentoo::emerge' 'nixos::nix' 'unknown::'; do
    IFS=: read -r id like expected <<< "$entry"
    select_manager "$id" "$like"
    [[ "$manager" == "$expected" ]] || fail "$id: expected $expected, got $manager"
done
for manager in apt-get dnf pacman zypper apk xbps-install emerge; do
    bash "$0" install "$manager"
done
for scenario in already-present rust-upgrade rust-current; do
    bash "$0" "$scenario"
done
for scenario in decline skip unknown nix immutable package-failure verification-failure rust-decline font-decline font-download-failure font-checksum-failure font-override; do
    if bash "$0" "$scenario" > "$work/output" 2>&1; then
        fail "$scenario unexpectedly succeeded"
    fi
    case "$scenario" in
        decline) grep -q 'Dependencies were not installed' "$work/output" ;;
        skip) grep -q 'skips package installation' "$work/output" ;;
        unknown) grep -q 'Unknown distribution' "$work/output" ;;
        nix) grep -q 'NixOS: use the flake' "$work/output" ;;
        immutable) grep -q 'Image-built/atomic OS' "$work/output" ;;
        package-failure) grep -q 'Packages (apt-get)' "$work/output" ;;
        verification-failure) grep -q 'Dependency verification failed' "$work/output" ;;
        font-decline) grep -q 'supported bitmap emoji font' "$work/output" ;;
        font-download-failure) grep -q 'selected emoji font has no CBDT' "$work/output" ;;
        font-checksum-failure) grep -q 'FAILED' "$work/output" ;;
        font-override) grep -q 'EMOJI_FONT selects an unsupported font' "$work/output" ;;
        rust-decline) grep -q 'Install a recent Rust toolchain' "$work/output" ;;
    esac
done
printf 'PASS distro selection, package commands, confirmations, failures and Rust version handling\n'
