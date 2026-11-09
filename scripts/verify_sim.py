#!/usr/bin/env python3
"""Compares TronicCAD's simulator with ngspice, an independent implementation.

Development tool only. TronicCAD neither ships nor links ngspice.
    cargo run -p tronic-sim --example sim_decks -- <directory>
    python scripts/verify_sim.py <directory> <path to ngspice_con>
"""
import bisect
import json
import subprocess
import sys
from pathlib import Path

directory = Path(sys.argv[1])
ngspice = sys.argv[2]
failures = []

# Allowed deviation: absolute plus relative. Transient runs differ more, because
# ngspice controls its own time step.
TOLERANCE = {
    "point": (1e-5, 1e-3),
    "V": (1e-3, 2e-3),
    "Hz": (1e-6, 5e-3),
    "s": (0.05, 0.01),
}


def run(case, output):
    is_op = case["kind"] == "point"
    vectors = " ".join(f"v({name})" for name in case["vectors"])
    if case["kind"] == "Hz":
        vectors = " ".join(f"vm({name})" for name in case["vectors"])
    action = f"print {vectors}" if is_op else f"wrdata {output.name} {vectors}"
    deck = (
        case["deck"]
        + f".control\nset wr_singlescale\nset numdgt=10\noption reltol=1e-6 vntol=1e-7 abstol=1e-12\n{case['command']}\n{action}\nquit\n.endc\n.end\n"
    )
    path = output.with_suffix(".cir")
    path.write_text(deck)
    done = subprocess.run(
        [ngspice, "-b", path.name],
        capture_output=True,
        text=True,
        cwd=directory,
        timeout=300,
    )
    return done.stdout + done.stderr


def interpolate(xs, ys, x):
    index = bisect.bisect_left(xs, x)
    if index == 0:
        return ys[0]
    if index >= len(xs):
        return ys[-1]
    span = xs[index] - xs[index - 1]
    weight = (x - xs[index - 1]) / span if span else 0.0
    return ys[index - 1] + (ys[index] - ys[index - 1]) * weight


def compare(name, mine_x, mine, theirs_x, theirs, tolerance):
    absolute, relative = tolerance
    worst = 0.0
    bad = None
    for x, value in zip(mine_x, mine):
        other = interpolate(theirs_x, theirs, x) if len(theirs) > 1 else theirs[0]
        deviation = abs(value - other)
        if deviation > worst:
            worst = deviation
        if deviation > absolute + relative * max(abs(value), abs(other)):
            bad = (x, value, other)
    return worst, bad


for file in sorted(directory.glob("*.json")):
    case = json.loads(file.read_text())
    name = case["name"]
    output = directory / f"{name}.out"
    output.unlink(missing_ok=True)
    log = run(case, output)
    traces = {t["name"]: t["values"] for t in case["traces"]}
    tolerance = TOLERANCE[case["kind"]]
    theirs_x = None
    columns = None
    if case["kind"] == "point":
        values = {}
        for line in log.splitlines():
            parts = line.replace("=", " ").split()
            if len(parts) == 2 and parts[0].lower().startswith("v(n"):
                values[parts[0].lower()] = float(parts[1])
    else:
        if not output.exists():
            print(f"FAIL  {name}: ngspice wrote no result\n{log[-400:]}")
            failures.append(name)
            continue
        rows = []
        for line in output.read_text().splitlines():
            parts = line.split()
            try:
                rows.append([float(part) for part in parts])
            except ValueError:
                continue
        theirs_x = [row[0] for row in rows]
        columns = list(zip(*[row[1:] for row in rows]))
    largest = 0.0
    failed = False
    for index, node in enumerate(case["nodes"]):
        vector = case["vectors"][index]
        mine = traces[f"V({node})"]
        if case["kind"] == "point":
            key = f"v({vector})"
            if key not in values:
                print(f"FAIL  {name}: no value for {key}\n{log[-400:]}")
                failed = True
                continue
            worst, bad = compare(node, [0.0], mine, [0.0], [values[key]], tolerance)
        else:
            worst, bad = compare(node, case["x"], mine, theirs_x, list(columns[index]), tolerance)
        largest = max(largest, worst)
        if bad:
            print(f"FAIL  {name} V({node}) at x={bad[0]:g}: TronicCAD {bad[1]:g}, ngspice {bad[2]:g}")
            failed = True
    if failed:
        failures.append(name)
    else:
        print(f"ok    {name}: largest deviation {largest:.3g}")

print(f"\n{'FAILED: ' + ', '.join(failures) if failures else 'All circuits match ngspice'}")
sys.exit(1 if failures else 0)
