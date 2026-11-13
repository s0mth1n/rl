{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  libxkbcommon,
  fontconfig,
  coreutils,
  which,
  wl-clipboard,
  xdg-utils,
}:

let
  cargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = "zl";
  version = cargoToml.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../assets
      ../examples
      ../packaging
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  buildInputs = [ libxkbcommon ];

  # The unit tests need a writable HOME and no display; the live compositor tests are not run here.
  doCheck = true;
  preCheck = ''
    export HOME=$TMPDIR
  '';

  postInstall = ''
    install -Dm644 packaging/zl.svg $out/share/icons/hicolor/scalable/apps/zl.svg
    substitute packaging/zl.desktop $out/share/applications/zl.desktop --replace-fail @BINARY@ $out/bin/zl
    mkdir -p $out/lib/systemd/user
    substitute packaging/zl.service $out/lib/systemd/user/zl.service --replace-fail @BINARY@ $out/bin/zl
    # fc-match for fonts and icons, timeout/date/which from coreutils, wl-copy as the copy fallback, xdg-open for web search.
    wrapProgram $out/bin/zl --suffix PATH : ${
      lib.makeBinPath [
        fontconfig
        coreutils
        which
        wl-clipboard
        xdg-utils
      ]
    }
  '';

  meta = {
    description = "A small Wayland app launcher, emoji picker and clipboard history, drawn on the CPU";
    homepage = "https://github.com/s0mth1n/rl";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "zl";
  };
}
