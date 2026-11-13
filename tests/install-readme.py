#!/usr/bin/env python3
"""Run the README install commands in a new home, with service control mocked."""
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[1]
readme = (repo / "README.md").read_text()
section = readme.split("## Install (optional)\n", 1)[1]
commands = section.split("```sh\n", 1)[1].split("```", 1)[0]
with tempfile.TemporaryDirectory(prefix="zl-install-") as temporary:
    home = Path(temporary)
    mock_bin = home / "mock-bin"
    mock_bin.mkdir()
    systemctl = mock_bin / "systemctl"
    systemctl.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$HOME/service-calls"\n')
    systemctl.chmod(0o755)
    env = dict(os.environ, HOME=str(home), PATH=f"{mock_bin}:{os.environ['PATH']}")
    subprocess.run(["bash", "-eu", "-c", commands], cwd=repo, env=env, check=True)
    binary = home / ".local/bin/zl"
    assert binary.read_bytes() == (repo / "target/release/zl").read_bytes()
    assert os.access(binary, os.X_OK)
    desktop = home / ".local/share/applications/zl.desktop"
    service = home / ".config/systemd/user/zl.service"
    icon = home / ".local/share/icons/hicolor/scalable/apps/zl.svg"
    assert desktop.read_text() == (repo / "packaging/zl.desktop").read_text().replace("@BINARY@", str(binary))
    assert service.read_text() == (repo / "packaging/zl.service").read_text().replace("@BINARY@", str(binary))
    assert icon.read_bytes() == (repo / "packaging/zl.svg").read_bytes()
    assert (home / "service-calls").read_text() == "--user enable --now zl.service\n"
    subprocess.run([str(binary), "--help"], env=env, check=True, stdout=subprocess.DEVNULL)
    for path in (binary, desktop, service, icon):
        path.unlink()
    assert not any(path.exists() for path in (binary, desktop, service, icon))
print("PASS README install in a clean home, exact installed files, binary help and file removal")
