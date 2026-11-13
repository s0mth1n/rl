#!/usr/bin/env bash

: "${yes:=false}" "${skip_deps:=false}" "${root:=.}" "${work:=.}"

say() { printf '%b%s%b\n' "$blue" "$*" "$reset"; }
warn() { printf '%b%s%b\n' "$yellow" "$*" "$reset" >&2; }

blue='' yellow='' reset=''
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    blue=$'\033[1;34m' yellow=$'\033[1;33m' reset=$'\033[0m'
fi

confirm_dependencies() { "$yes" || ask "$1"; }

detect_distro() {
    local ID='' ID_LIKE='' PRETTY_NAME=''
    if [[ -r /etc/os-release ]]; then
        # shellcheck source=/dev/null
        . /etc/os-release
    fi
    distro=${PRETTY_NAME:-${ID:-unknown Linux}}
    select_manager "$ID" "$ID_LIKE"
    immutable=false
    if [[ -e /run/ostree-booted ]] || command -v bootc >/dev/null 2>&1 || command -v transactional-update >/dev/null 2>&1; then
        immutable=true
    fi
    say "System: $distro"
}

select_manager() {
    manager=
    local family
    for family in "$1" ${2:-}; do
        case "$family" in
            nixos) manager=nix; break ;;
            debian|ubuntu) manager=apt-get; break ;;
            fedora|rhel|centos|rocky|almalinux) manager=dnf; break ;;
            arch|manjaro|endeavouros) manager=pacman; break ;;
            opensuse*|suse|sles) manager=zypper; break ;;
            alpine) manager=apk; break ;;
            void) manager=xbps-install; break ;;
            gentoo) manager=emerge; break ;;
        esac
    done
}

font_available() {
    local file
    local pattern=$1
    [[ "$1" != emoji ]] || pattern=${EMOJI_FONT:-${emoji_pattern:-emoji}}
    file=$(fc-match -f '%{file}' "$pattern" 2>/dev/null) || return 1
    [[ -r "$file" && -s "$file" ]] || return 1
    if [[ "$1" == emoji ]]; then
        grep -aqE 'CBDT|sbix' "$file"
    fi
}

ensure_bitmap_emoji() {
    font_available emoji && return 0
    [[ -z "${EMOJI_FONT:-}" ]] || fail 'EMOJI_FONT selects an unsupported font. Unset it or select a CBDT/sbix bitmap font, then rerun.'
    local font_dir=${XDG_DATA_HOME:-$HOME/.local/share}/fonts/zl
    [[ "$font_dir" == /* && "$font_dir" != *$'\n'* ]] || fail 'XDG_DATA_HOME must be an absolute path without line breaks.'
    local font="$font_dir/NotoColorEmoji.ttf" pattern
    pattern=${font//\\/\\\\}
    pattern=${pattern//:/\\:}
    pattern=${pattern//,/\\,}
    emoji_pattern="emoji:file=$pattern"
    if [[ -r "$font" ]] && font_available emoji; then return 0; fi
    warn 'The selected emoji font has no CBDT/sbix bitmaps. zl cannot render COLRv1-only emoji.'
    ask 'Download the pinned Noto bitmap font and its OFL license from Google (GitHub) into your user fonts for zl?' || fail 'A supported bitmap emoji font is required.'
    local upstream=https://raw.githubusercontent.com/googlefonts/noto-emoji/8998f5dd683424a73e2314a8c1f1e359c19e8742/fonts
    need sha256sum 'Install coreutils to verify the font download.'
    curl --proto '=https' --tlsv1.2 -fsSL "$upstream/NotoColorEmoji.ttf" -o "$work/NotoColorEmoji.ttf"
    curl --proto '=https' --tlsv1.2 -fsSL "$upstream/LICENSE" -o "$work/OFL.txt"
    printf '%s  %s\n' 72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b "$work/NotoColorEmoji.ttf" \
        6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2 "$work/OFL.txt" | sha256sum -c -
    install -Dm644 "$work/NotoColorEmoji.ttf" "$font"
    install -Dm644 "$work/OFL.txt" "$font_dir/OFL.txt"
    fc-cache -f "$font_dir"
    font_available emoji || fail 'Fontconfig did not select the downloaded bitmap font. Check EMOJI_FONT and fc-match emoji.'
    say "Bitmap emoji font: $font (system font selection unchanged)"
}

missing_dependencies() {
    missing=()
    command -v "${CC:-cc}" >/dev/null 2>&1 || missing+=('C compiler/linker')
    if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists xkbcommon; then
        missing+=('pkg-config and xkbcommon development files')
    fi
    if [[ "${manager:-}" == apk ]] && command -v pkg-config >/dev/null 2>&1; then
        local xkb_lib
        xkb_lib=$(pkg-config --variable=libdir xkbcommon 2>/dev/null) || xkb_lib=
        [[ -f "$xkb_lib/libxkbcommon.a" ]] || missing+=('libxkbcommon-static for the musl Rust target')
    fi
    local command
    for command in curl install timeout date which; do
        command -v "$command" >/dev/null 2>&1 || missing+=("$command")
    done
    if [[ ! -s /etc/ssl/certs/ca-certificates.crt && ! -s /etc/pki/tls/certs/ca-bundle.crt && ! -s /etc/ssl/ca-bundle.pem ]]; then
        missing+=('CA certificates for HTTPS downloads')
    fi
    if ! command -v fc-match >/dev/null 2>&1 || ! font_available sans-serif; then
        missing+=('fontconfig and a text font')
    fi
    if ! command -v fc-match >/dev/null 2>&1 || ! font_available emoji; then
        missing+=('a bitmap colour emoji font (Noto Color Emoji)')
    fi
    command -v wl-copy >/dev/null 2>&1 || missing+=('wl-clipboard')
    command -v xdg-open >/dev/null 2>&1 || missing+=('xdg-utils')
}

dependency_packages() {
    case "$manager" in
        apt-get) packages=(build-essential pkg-config libxkbcommon-dev curl ca-certificates coreutils debianutils fontconfig fonts-dejavu-core fonts-noto-color-emoji wl-clipboard xdg-utils) ;;
        dnf) packages=(gcc glibc-devel pkgconf-pkg-config libxkbcommon-devel curl ca-certificates coreutils which fontconfig dejavu-sans-fonts google-noto-color-emoji-fonts wl-clipboard xdg-utils) ;;
        pacman) packages=(base-devel pkgconf libxkbcommon curl ca-certificates coreutils which fontconfig ttf-dejavu noto-fonts-emoji wl-clipboard xdg-utils) ;;
        zypper) packages=(gcc glibc-devel pkg-config libxkbcommon-devel curl ca-certificates coreutils which fontconfig dejavu-fonts google-noto-coloremoji-fonts wl-clipboard xdg-utils) ;;
        apk) packages=(build-base pkgconf libxkbcommon-dev libxkbcommon-static curl ca-certificates coreutils which fontconfig font-dejavu font-noto-emoji wl-clipboard xdg-utils) ;;
        xbps-install) packages=(base-devel pkgconf libxkbcommon-devel curl ca-certificates coreutils which fontconfig dejavu-fonts-ttf noto-fonts-emoji wl-clipboard xdg-utils) ;;
        emerge) packages=(sys-devel/gcc virtual/pkgconfig x11-libs/libxkbcommon net-misc/curl app-misc/ca-certificates sys-apps/coreutils sys-apps/which media-libs/fontconfig media-fonts/dejavu media-fonts/noto-emoji gui-apps/wl-clipboard x11-misc/xdg-utils) ;;
        *) fail "No package list for $distro. Install the missing dependencies manually, then rerun." ;;
    esac
}

install_dependencies() {
    say 'Checking build and runtime dependencies'
    detect_distro
    if [[ "$manager" == nix ]]; then
        fail 'NixOS: use the flake (nix profile install .), or the NixOS/Home Manager module in README.md. Add pkgs.noto-fonts-color-emoji to fonts.packages. This script does not change NixOS.'
    fi
    emoji_pattern=
    if ! font_available emoji; then
        local font_dir=${XDG_DATA_HOME:-$HOME/.local/share}/fonts/zl
        if [[ -r "$font_dir/NotoColorEmoji.ttf" ]]; then ensure_bitmap_emoji; fi
    fi
    missing_dependencies
    if ((${#missing[@]})); then
        printf 'Missing:\n' >&2
        printf '  %s\n' "${missing[@]}" >&2
        if "$skip_deps"; then
            fail '--skip-deps skips package installation, not dependency verification. Install the missing dependencies and rerun.'
        fi
        if "$immutable"; then
            fail 'Image-built/atomic OS: add dependencies to your image, or build in a development container with the matching runtime libraries. This script does not run dnf or transactional-update on the host.'
        fi
        [[ -n "$manager" ]] || fail 'Unknown distribution. Install the dependencies listed above manually, then rerun.'
        dependency_packages
        need "$manager" 'Install the dependencies manually or enable your distribution package manager.'
        printf 'Packages (%s): %s\n' "$manager" "${packages[*]}"
        if [[ "$manager" == dnf ]]; then
            warn 'On RHEL derivatives, wl-clipboard may require EPEL/CRB. Enable repositories yourself if needed; this script does not add repositories.'
        elif [[ "$manager" == apk ]]; then
            warn 'Alpine needs the community repository for emoji fonts and wl-clipboard. Enable the matching release repository yourself if needed.'
        fi
        confirm_dependencies "Install these packages with $manager (using root privileges)?" || fail 'Dependencies were not installed.'
        local admin=()
        if ((EUID != 0)); then
            need sudo 'Install these packages as root, then rerun. Do not run the whole installer with sudo.'
            admin=(sudo)
        fi
        case "$manager" in
            apt-get)
                "${admin[@]}" apt-get update
                "${admin[@]}" apt-get install -y --no-install-recommends "${packages[@]}" ;;
            dnf) "${admin[@]}" dnf install -y "${packages[@]}" ;;
            pacman) "${admin[@]}" pacman -S --needed --noconfirm "${packages[@]}" ;;
            zypper) "${admin[@]}" zypper --non-interactive install "${packages[@]}" ;;
            apk) "${admin[@]}" apk add "${packages[@]}" ;;
            xbps-install) "${admin[@]}" xbps-install -S -y "${packages[@]}" ;;
            emerge) "${admin[@]}" emerge --ask=n "${packages[@]}" ;;
        esac
        command -v fc-cache >/dev/null 2>&1 && fc-cache -f
        ensure_bitmap_emoji
        missing_dependencies
        ((${#missing[@]} == 0)) || fail "Dependency verification failed: ${missing[*]}. Check repositories and fontconfig's emoji selection (fc-match emoji); then rerun."
    fi
    say 'Build and runtime dependencies found'
}

rust_is_current() {
    local tool version major minor
    for tool in rustc cargo; do
        version=$("$tool" --version 2>/dev/null) || return 1
        version=${version#* }
        IFS=. read -r major minor _ <<< "$version"
        [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ ]] || return 1
        ((major > rust_major || (major == rust_major && minor >= rust_minor))) || return 1
    done
}

ensure_rust() {
    local required
    required=$(sed -n 's/^rust-version = "\([0-9.]*\)"/\1/p' "$root/Cargo.toml")
    [[ "$required" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || fail 'Could not read rust-version from Cargo.toml.'
    IFS=. read -r rust_major rust_minor _ <<< "$required"
    if ! rust_is_current; then
        warn "Rust and Cargo $required or newer are required."
        ask 'Install the stable Rust toolchain with rustup for this user (download from rust-lang.org)?' || fail 'Install a recent Rust toolchain, then rerun.'
        if command -v rustup >/dev/null 2>&1; then
            rustup toolchain install stable --profile minimal
        else
            curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$work/rustup.sh"
            sh "$work/rustup.sh" -y --profile minimal --default-toolchain stable --no-modify-path
        fi
        export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH" RUSTUP_TOOLCHAIN=stable
        rust_is_current || fail "Rust installation did not provide Rust/Cargo $required or newer."
    fi
    say "Toolchain: $(rustc --version); $(cargo --version)"
}
