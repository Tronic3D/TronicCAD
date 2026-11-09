import { describe, expect, it } from "vitest";
import {
  clipFromSelection,
  clipOrigin,
  elementsInRect,
  fitCamera,
  GRID,
  hitTest,
  isConnectionPoint,
  parsePinList,
  place,
  pinPoints,
  route,
  screenToWorld,
  sheetBounds,
  snap,
  worldToScreen,
  zoomAt,
} from "./geometry";
import { Component, Sheet, SymbolDef } from "./types";

const g = (x: number, y: number) => ({ x: x * GRID, y: y * GRID });

const symbol: SymbolDef = {
  id: "s",
  name: "S",
  category: "resistors",
  kind: "part",
  reference_prefix: "R",
  default_value: "",
  description: "",
  graphics: [{ type: "rect", a: g(-1, -2), b: g(1, 2) }],
  pins: [
    { number: "1", name: "1", kind: "passive", at: g(0, -4), direction: "down", length: 2 * GRID, hide_name: true, hide_number: false },
    { number: "2", name: "2", kind: "passive", at: g(0, 4), direction: "up", length: 2 * GRID, hide_name: true, hide_number: false },
  ],
  part: { footprint: null, model_3d: null, manufacturer: null, part_number: null, datasheet: null, limits: [] },
  provenance: { source: "t", author: "t", license: "t", reviewed: false },
};
const symbols = { s: symbol };

const component = (id: number, x: number, y: number): Component => ({
  id,
  symbol: "s",
  reference: `R${id}`,
  value: "",
  at: g(x, y),
  rotation: 0,
  mirror: false,
});

const sheet = (overrides: Partial<Sheet> = {}): Sheet => ({
  id: 1,
  name: "1",
  parent: null,
  components: [],
  wires: [],
  buses: [],
  labels: [],
  no_connects: [],
  junctions: [],
  instances: [],
  ...overrides,
});

describe("placement", () => {
  // Same vectors as the Rust test `place_mirrors_before_rotating` and
  // `rotation_is_clockwise_on_a_y_down_screen`.
  it("matches the backend", () => {
    expect(place({ x: 2, y: 1 }, { x: 10, y: 10 }, 90, true)).toEqual({ x: 9, y: 8 });
    expect(place({ x: 1, y: 0 }, { x: 0, y: 0 }, 90, false)).toEqual({ x: 0, y: 1 });
    expect(place({ x: 1, y: 2 }, { x: 0, y: 0 }, 180, false)).toEqual({ x: -1, y: -2 });
    expect(place({ x: 1, y: 0 }, { x: 0, y: 0 }, 270, false)).toEqual({ x: 0, y: -1 });
  });

  it("computes absolute pin positions", () => {
    const placed = { ...component(1, 10, 10), rotation: 90 as const };
    expect(pinPoints(symbol, placed)).toEqual([g(14, 10), g(6, 10)]);
  });
});

describe("camera", () => {
  const camera = { ox: 100, oy: 50, scale: 10 };

  it("converts screen and world positions back and forth", () => {
    const world = screenToWorld(camera, 130, 90);
    expect(world).toEqual(g(3, 4));
    expect(worldToScreen(camera, world)).toEqual({ x: 130, y: 90 });
  });

  it("keeps the point under the cursor while zooming", () => {
    const before = screenToWorld(camera, 200, 200);
    const after = screenToWorld(zoomAt(camera, 200, 200, 1.5), 200, 200);
    expect(after.x).toBeCloseTo(before.x, 3);
    expect(after.y).toBeCloseTo(before.y, 3);
  });

  it("limits the zoom range", () => {
    expect(zoomAt(camera, 0, 0, 1000).scale).toBe(120);
    expect(zoomAt(camera, 0, 0, 0.0001).scale).toBe(0.5);
  });

  it("fits content into the viewport", () => {
    const rect = { minX: 0, minY: 0, maxX: 20 * GRID, maxY: 10 * GRID };
    const fitted = fitCamera(rect, 1000, 500);
    const a = worldToScreen(fitted, g(0, 0));
    const b = worldToScreen(fitted, g(20, 10));
    expect(a.x).toBeGreaterThanOrEqual(0);
    expect(b.x).toBeLessThanOrEqual(1000);
    expect(a.y).toBeGreaterThanOrEqual(0);
    expect(b.y).toBeLessThanOrEqual(500);
  });

  it("centres an empty sheet on the origin", () => {
    expect(worldToScreen(fitCamera(null, 800, 600), g(0, 0))).toEqual({ x: 400, y: 300 });
  });
});

describe("snapping and routing", () => {
  it("snaps to the nearest grid point", () => {
    expect(snap({ x: 1.4 * GRID, y: -1.6 * GRID })).toEqual(g(1, -2));
  });

  it("routes horizontally first and only orthogonally", () => {
    expect(route(g(0, 0), g(5, 3))).toEqual([g(5, 0), g(5, 3)]);
    expect(route(g(0, 0), g(5, 0))).toEqual([g(5, 0)]);
    expect(route(g(0, 0), g(0, 0))).toEqual([]);
  });
});

describe("hit testing", () => {
  const scene = sheet({
    components: [component(1, 0, 0)],
    wires: [{ id: 2, a: g(0, 4), b: g(10, 4) }],
    labels: [{ id: 3, at: g(5, 10), name: "NET", kind: "local", rotation: 0 }],
    no_connects: [{ id: 4, at: g(-5, -5) }],
    instances: [{ id: 5, sheet: 9, at: g(20, 0), size: g(10, 10), ports: [] }],
  });

  it.each([
    ["component", g(0, 0), 1],
    ["wire", g(5, 4), 2],
    ["label", g(6, 10), 3],
    ["marker", g(-5, -5), 4],
    ["block", g(25, 5), 5],
    ["nothing", g(50, 50), null],
  ])("finds the %s", (_name, point, expected) => {
    expect(hitTest(scene, symbols, point, GRID / 4)).toBe(expected);
  });

  it("prefers the wire over the component it touches", () => {
    expect(hitTest(scene, symbols, g(0, 4), GRID / 4)).toBe(2);
  });
});

describe("selection helpers", () => {
  const scene = sheet({
    components: [component(1, 0, 0), component(2, 50, 0)],
    wires: [{ id: 3, a: g(0, 4), b: g(4, 4) }],
  });

  it("selects only what lies completely inside the box", () => {
    const rect = { minX: -5 * GRID, minY: -5 * GRID, maxX: 5 * GRID, maxY: 5 * GRID };
    expect(elementsInRect(scene, symbols, rect).sort()).toEqual([1, 3]);
    const tooSmall = { minX: -1 * GRID, minY: -1 * GRID, maxX: 1 * GRID, maxY: 1 * GRID };
    expect(elementsInRect(scene, symbols, tooSmall)).toEqual([]);
  });

  it("copies selected elements only", () => {
    const clip = clipFromSelection(scene, new Set([2, 3]));
    expect(clip.components.map((c) => c.id)).toEqual([2]);
    expect(clip.wires.map((w) => w.id)).toEqual([3]);
    expect(clipOrigin(clip)).toEqual(g(0, 0));
  });

  it("has no paste origin for an empty clipboard", () => {
    expect(clipOrigin(clipFromSelection(scene, new Set()))).toBeNull();
  });

  it("knows the bounds of a sheet", () => {
    expect(sheetBounds(sheet(), symbols)).toBeNull();
    expect(sheetBounds(scene, symbols)?.maxX).toBe(51 * GRID);
  });

  it("recognises places where a wire connects", () => {
    expect(isConnectionPoint(scene, symbols, g(0, -4))).toBe(true);
    expect(isConnectionPoint(scene, symbols, g(2, 4))).toBe(true);
    expect(isConnectionPoint(scene, symbols, g(2, 7))).toBe(false);
  });
});

describe("pin list of a custom part", () => {
  it("parses numbers, names, kinds and sides with defaults", () => {
    expect(parsePinList("1 VCC power_in\n\n2 OUT output right\n3 X")).toEqual({
      ok: true,
      pins: [
        { number: "1", name: "VCC", kind: "power_in", side: "left" },
        { number: "2", name: "OUT", kind: "output", side: "right" },
        { number: "3", name: "X", kind: "passive", side: "left" },
      ],
    });
  });

  it.each([
    ["1", 1],
    ["1 A input\n2 B nonsense", 2],
    ["1 A input left extra", 1],
    ["", 1],
  ])("reports the line of a mistake in %j", (text, line) => {
    expect(parsePinList(text)).toEqual({ ok: false, line });
  });
});
