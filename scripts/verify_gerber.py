#!/usr/bin/env python3
"""Checks TronicCAD's manufacturing files with an independent parser.

Reads the files written by `cargo run -p tronic-fab --example fab_sample` using
gerbonara, a third-party Gerber library that shares no code with TronicCAD, and
compares what it understood with the known sample board.

Development tool only, not part of the product:
    python -m pip install --no-deps gerbonara click
    python scripts/verify_gerber.py <directory> [layers]
"""
import sys
from pathlib import Path

from gerbonara import ExcellonFile, GerberFile
from gerbonara.graphic_objects import Flash, Line, Region
from gerbonara.utils import MM

directory = Path(sys.argv[1])
layers = int(sys.argv[2]) if len(sys.argv) > 2 else 2
errors = []


def expect(condition, message):
    print(("ok    " if condition else "FAIL  ") + message)
    if not condition:
        errors.append(message)


def close(a, b, tolerance=1e-4):
    return abs(a - b) <= tolerance


def gerber(name):
    return GerberFile.open(directory / f"sample-{name}.gbr", override_settings=None)


def objects(layer, kind):
    return [o for o in layer.objects if isinstance(o, kind)]


def has_flash(layer, x, y):
    return any(close(o.x, x) and close(o.y, y) for o in objects(layer, Flash))


def to_mm(layer):
    """TronicCAD writes millimetres; anything else would be a defect."""
    for o in layer.objects:
        if o.unit != MM:
            raise SystemExit(f"unexpected unit {o.unit}")
    return layer


top = to_mm(gerber("copper-l1-top"))
bottom_name = f"copper-l{layers}-bottom"
bottom = to_mm(gerber(bottom_name)) if layers > 1 else None

# Known positions: Gerber Y is the negated board Y.
expect(has_flash(top, 15 - 0.95, -10), "R1 pad 1 lies where the board says")
expect(has_flash(top, 15 + 0.95, -10), "R1 pad 2 lies where the board says")
expect(has_flash(top, 10, -25 + 1.27), "J1 pad 1 lies where the board says")
expect(has_flash(top, 25, -10), "the via is on the top layer")

lines = objects(top, Line)
expected_segments = 3 if layers > 1 else 4
expect(len(lines) == expected_segments, f"top layer has {expected_segments} track segments, found {len(lines)}")
signal = [l for l in lines if close(l.y1, -10) and close(l.y2, -10) and close(l.x2, 25)]
expect(bool(signal) and close(signal[0].aperture.diameter, 0.25), "track SIG has the width 0.25 mm")
power = [l for l in lines if close(l.aperture.diameter, 0.4)]
expect(len(power) == 2, "both VCC segments have the width 0.4 mm")

if bottom is not None:
    flashes = objects(bottom, Flash)
    # C1 is mirrored on the bottom side: pad 1 lies on the right.
    expect(has_flash(bottom, 30 + 0.95, -10), "C1 pad 1 is mirrored on the bottom side")
    expect(has_flash(bottom, 30 - 0.95, -10), "C1 pad 2 is mirrored on the bottom side")
    expect(has_flash(bottom, 25, -10), "the via is on the bottom layer")
    expect(has_flash(bottom, 10, -25 + 1.27), "through-hole pads are on the bottom layer")
    expect(not has_flash(bottom, 15 - 0.95, -10), "top surface pads are not on the bottom layer")
    regions = objects(bottom, Region)
    dark = [r for r in regions if r.polarity_dark]
    clear = [r for r in regions if not r.polarity_dark]
    expect(len(dark) >= 1, f"the ground zone is a dark region ({len(dark)})")
    expect(len(clear) >= 1, f"the ground zone has cut-outs ({len(clear)})")
    (x0, y0), (x1, y1) = max(dark, key=lambda r: len(r.outline)).bounding_box(MM)
    expect(x0 > 0 and x1 < 50 and y0 > -40 and y1 < 0, "the zone stays inside the board")
    expect(close(x0, 1.0, 0.01) and close(x1, 49.0, 0.01), "the zone spans its outline")

for inner in range(2, layers):
    layer = to_mm(gerber(f"copper-l{inner}-inner"))
    expect(has_flash(layer, 25, -10), f"the via is on inner layer {inner}")
    expect(not objects(layer, Line), f"inner layer {inner} has no tracks")

outline = to_mm(gerber("outline"))
segments = objects(outline, Line)
expect(len(segments) == 4, "the outline has 4 edges")
xs = [v for l in segments for v in (l.x1, l.x2)]
ys = [v for l in segments for v in (l.y1, l.y2)]
expect(close(min(xs), 0) and close(max(xs), 50), "the outline is 50 mm wide")
expect(close(max(ys), 0) and close(min(ys), -40), "the outline is 40 mm high")

mask = to_mm(gerber("mask-top"))
opening = next(o for o in objects(mask, Flash) if close(o.x, 15 - 0.95) and close(o.y, -10))
expect(close(opening.aperture.w, 1.1) and close(opening.aperture.h, 1.4), "mask opening is the pad plus margin")
expect(not has_flash(mask, 25, -10), "vias stay covered by the mask")
paste = to_mm(gerber("paste-top"))
expect(has_flash(paste, 15 - 0.95, -10), "surface pads have paste")
expect(not has_flash(paste, 10, -25 + 1.27), "through-hole pads have no paste")
silk = to_mm(gerber("silk-top"))
expect(len(objects(silk, Line)) > 4, "the silkscreen has strokes for outlines and references")

plated = ExcellonFile.open(directory / "sample-plated.drl")
plated_hits = list(to_mm(plated).objects)
holes = sorted((round(o.x, 3), round(o.y, 3), round(o.tool.diameter, 3)) for o in plated_hits)
expect(len(holes) == 3, f"3 plated holes, found {len(holes)}")
expect((25.0, -10.0, 0.3) in holes, "the via hole has 0.3 mm at the right place")
expect((10.0, -23.73, 1.0) in holes, "header hole 1 has 1 mm at the right place")
bare = ExcellonFile.open(directory / "sample-nonplated.drl")
to_mm(bare)
expect(
    len(bare.objects) == 1 and close(bare.objects[0].x, 44) and close(bare.objects[0].y, -34)
    and close(bare.objects[0].tool.diameter, 3.2),
    "the mounting hole is unplated with 3.2 mm at the right place",
)

print(f"\n{'FAILED: ' + str(len(errors)) if errors else 'All checks passed'}")
sys.exit(1 if errors else 0)
