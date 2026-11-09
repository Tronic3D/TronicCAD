#!/usr/bin/env python3
"""Builds, tests, packages and starts TronicCAD on a clean Linux.

Everything runs in a fresh import of a WSL image that nothing else has touched: all Rust
and interface tests, the Debian package and AppImage, installing the package, and starting
the installed program on a virtual screen, with a picture of what it shows.
Usage: verify_linux.py <clean-image.tar> <work-directory> <directory for the picture> [step]
"""
import subprocess
import sys
import tarfile
import time
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
image, work, pictures = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
root = Path(__file__).resolve().parent.parent
distro = "TronicVerifyLinux"
work.mkdir(parents=True, exist_ok=True)
pictures.mkdir(parents=True, exist_ok=True)
archive = work / "troniccad-source.tar.gz"
files = subprocess.run(
    ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    check=True,
    capture_output=True,
).stdout
with tarfile.open(archive, "w:gz") as package:
    for name in files.decode("utf-8").split(chr(0)):
        path = root / name
        if name and path.is_file():
            package.add(path, arcname=name)


def wsl(*arguments, **options):
    return subprocess.run(["wsl.exe", *arguments], **options)


def windows_to_wsl(path):
    text = str(path.resolve()).replace("\\", "/")
    return "/mnt/" + text[0].lower() + text[2:]


# name, may it fail without ending the run, commands
STEPS = [
    ("packages", False, """
export DEBIAN_FRONTEND=noninteractive
# The system's own updates would hold the package lock in the middle of the run.
systemctl stop unattended-upgrades apt-daily.timer apt-daily-upgrade.timer 2>/dev/null || true
apt-get -o DPkg::Lock::Timeout=600 update
apt-get -o DPkg::Lock::Timeout=600 install -y build-essential pkg-config curl file libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \\
  libayatana-appindicator3-dev libssl-dev libxdo-dev nodejs npm xvfb imagemagick dbus-x11 x11-utils xz-utils
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
"""),
    ("source", False, f"""
mkdir -p /root/src
tar xzf {windows_to_wsl(archive)} -C /root/src
cd /root/src/apps/desktop
npm ci
"""),
    ("versions", False, """
. "$HOME/.cargo/env"
cd /root/src
rustc --version
node --version
. /etc/os-release; echo "$PRETTY_NAME"
"""),
    ("rust tests", False, """
. "$HOME/.cargo/env"
cd /root/src
cargo test --workspace --no-fail-fast 2>&1 | grep -E '^test result|FAILED|failed|panicked|^error' | sort | uniq -c | sort -rn | head -20
cargo test --workspace --no-fail-fast > /tmp/test.log 2>&1
"""),
    ("interface tests", False, """
cd /root/src/apps/desktop
npx tsc --noEmit
npx vitest run 2>&1 | tail -5
"""),
    ("debian package", False, """
. "$HOME/.cargo/env"
cd /root/src/apps/desktop
npx tauri build --bundles deb 2>&1 | tail -6
ls -la /root/src/target/release/bundle/deb/
"""),
    ("appimage", True, """
. "$HOME/.cargo/env"
cd /root/src/apps/desktop
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true npx tauri build --bundles appimage 2>&1 | tail -6
ls -la /root/src/target/release/bundle/appimage/
"""),
    ("install", False, """
apt-get -o DPkg::Lock::Timeout=600 install -y /root/src/target/release/bundle/deb/*.deb 2>&1 | tail -3
dpkg -l | grep -i tronic
command -v troniccad
"""),
    ("start", False, f"""
export TRONICCAD_DATA_DIR=/root/tronic-data
export WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
# WSL offers Wayland, and the window would open there instead of on this screen.
unset WAYLAND_DISPLAY
export GDK_BACKEND=x11
Xvfb :77 -screen 0 1280x800x24 &
XVFB=$!
sleep 2
export DISPLAY=:77
dbus-run-session -- troniccad > /tmp/app.log 2>&1 &
APP=$!
sleep 25
import -window root /tmp/linux.png
cp /tmp/linux.png {windows_to_wsl(pictures)}/linux-started.png
if kill -0 $APP; then echo "still running after 25 s"; else echo "it ended"; cat /tmp/app.log; exit 1; fi
xwininfo -root -tree | grep -i -m3 tronic || true
# A running process proves little: a screen of one color means nothing is shown.
COLORS=$(identify -format '%k' /tmp/linux.png)
echo "colors on the screen: $COLORS"
test "$COLORS" -gt 50
test -f /root/tronic-data/troniccad.db && echo "settings database exists"
kill $APP || true
kill $XVFB || true
tail -5 /tmp/app.log
"""),
    ("remove", False, """
apt-get -o DPkg::Lock::Timeout=600 remove -y tronic-cad 2>&1 | tail -2
command -v troniccad && exit 1 || echo "the program is gone"
test -f /root/tronic-data/troniccad.db && echo "the user's data stayed"
"""),
]

# To go on after a failure on the same system, name the step to go on from.
resume = sys.argv[4] if len(sys.argv) > 4 else None
if resume:
    STEPS = STEPS[[name for name, _, _ in STEPS].index(resume):]
else:
    wsl("--unregister", distro, capture_output=True)
    wsl("--import", distro, str(work / distro), str(image), "--version", "2", check=True, capture_output=True)
status = 1
results = []
try:
    for name, optional, block in STEPS:
        print(f"--- {name}", flush=True)
        started = time.time()
        script = "set -eu\ncd /root\n" + block
        result = wsl(
            "-d", distro, "-u", "root", "--exec", "bash", "-c",
            "cat > /tmp/verify-step.sh; exec bash /tmp/verify-step.sh < /dev/null",
            input=script.encode("utf-8"), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        tail = "\n".join(result.stdout.decode("utf-8", "replace").splitlines()[-22:])
        print(tail, flush=True)
        seconds = time.time() - started
        print(f"exit {result.returncode}, {seconds:.0f} s", flush=True)
        results.append((name, result.returncode, round(seconds)))
        if result.returncode != 0 and not optional:
            raise SystemExit(f"{name} failed")
    print("ALL REQUIRED STEPS PASSED", flush=True)
    status = 0
finally:
    for name, code, seconds in results:
        print(f"{name}: exit {code}, {seconds} s", flush=True)
    if status == 0:
        wsl("--terminate", distro, capture_output=True)
        wsl("--unregister", distro, capture_output=True)
    else:
        print(f"kept {distro} for inspection", flush=True)
