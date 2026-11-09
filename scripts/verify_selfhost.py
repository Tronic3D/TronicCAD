#!/usr/bin/env python3
"""Runs docs/SELF_HOSTING.md literally on a clean system.

Takes the bash blocks marked `<!-- selfhost:<way> -->`, and runs them unchanged, in
order, as root, in a fresh import of a WSL image that nothing else has touched.
Usage: verify_selfhost.py <docker|native> <clean-image.tar> <work-directory>
"""
import re
import subprocess
import sys
import tarfile
import time
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
way, image, work = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
root = Path(__file__).resolve().parent.parent
guide = (root / "docs" / "SELF_HOSTING.md").read_text(encoding="utf-8")
blocks = re.findall(rf"<!-- selfhost:{way} -->\s*```bash\n(.*?)```", guide, re.S)
if not blocks:
    raise SystemExit(f"no blocks for {way}")
print(f"{len(blocks)} blocks from the guide", flush=True)

distro = f"TronicVerify{way.capitalize()}"
work.mkdir(parents=True, exist_ok=True)
archive = work / "troniccad-source.tar.gz"
# The working tree as it is, without what git ignores.
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
print(f"source package: {archive.stat().st_size} bytes", flush=True)


def wsl(*arguments, **options):
    return subprocess.run(["wsl.exe", *arguments], **options)


wsl("--unregister", distro, capture_output=True)
wsl("--import", distro, str(work / distro), str(image), "--version", "2", check=True, capture_output=True)
status = 1
try:
    drive = str(archive).replace("\\", "/")
    drive = "/mnt/" + drive[0].lower() + drive[2:]
    wsl("-d", distro, "-u", "root", "--", "cp", drive, "/root/troniccad-source.tar.gz", check=True)
    for number, block in enumerate(blocks, 1):
        print(f"--- block {number}/{len(blocks)}\n{block.splitlines()[0]}", flush=True)
        script = "set -euo pipefail\ncd /root\n" + block
        started = time.time()
        result = wsl(
            # On standard input: wsl.exe would send arguments through another shell,
            # which would change the commands. From a file, so that a command that
            # reads its input cannot swallow the rest of the block.
            "-d", distro, "-u", "root", "--exec", "bash", "-c",
            "cat > /tmp/selfhost-block.sh; exec bash /tmp/selfhost-block.sh < /dev/null",
            # As bytes: text mode on Windows would turn every line end into CR LF.
            input=script.encode("utf-8"), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        tail = "\n".join(result.stdout.decode("utf-8", "replace").splitlines()[-12:])
        print(tail, flush=True)
        print(f"exit {result.returncode}, {time.time() - started:.0f} s", flush=True)
        if result.returncode != 0:
            raise SystemExit(f"block {number} failed")
    print(f"ALL BLOCKS PASSED: {way}", flush=True)
    status = 0
finally:
    if status == 0:
        wsl("--terminate", distro, capture_output=True)
        wsl("--unregister", distro, capture_output=True)
    else:
        print(f"kept {distro} for inspection", flush=True)
