import { describe, expect, it } from "vitest";
import {
  boardToScreen,
  copperAt,
  elementsInBox,
  fitBoard,
  hitElement,
  MM,
  padContains,
  parseMm,
  pointInPolygon,
  route45,
  screenToBoard,
  snapTo,
  zoomCamera,
} from "./geometry";
import { BoardView, PadView } from "./types";

const p = (x: number, y: number) => ({ x: x * MM, y: y * MM });

const pad: PadView = {
  instance: 1,
  reference: "R1",
  number: "1",
  net: "A",
  at: p(0, 0),
  size: p(2, 1),
  shape: "rect",
  drill: null,
  plated: true,
  layer: 0,
  bottom: false,
};

const board: BoardView = {
  copper_layers: 2,
  rules: {
    min_clearance: 1,
    min_track_width: 1,
    track_width: 1,
    via_drill: 1,
    via_diameter: 2,
    min_drill: 1,
    max_drill: 1,
    min_annular_ring: 1,
    min_hole_spacing: 1,
    edge_clearance: 1,
    mask_margin: 1,
  },
  outline: [],
  footprints: [
    {
      id: 1,
      reference: "R1",
      value: "",
      footprint: "",
      at: p(0, 0),
      rotation: 0,
      bottom: false,
      silk: [],
      courtyard: [p(-2, -2), p(2, 2)],
    },
  ],
  pads: [pad],
  tracks: [{ id: 2, net: "B", layer: 1, a: p(10, 0), b: p(20, 0), width: MM }],
  vias: [{ id: 3, net: "C", at: p(30, 0), drill: MM / 2, diameter: MM }],
  zones: [{ id: 4, net: "D", layer: 0, outline: [p(40, 0), p(50, 0), p(50, 10), p(40, 10)] }],
  fills: [],
  airwires: [],
  net_lengths: {},
  nets: [],
};

describe("camera", () => {
  const camera = { ox: 100, oy: 50, scale: 10 };

  it("converts positions back and forth", () => {
    expect(screenToBoard(camera, 130, 90)).toEqual(p(3, 4));
    expect(boardToScreen(camera, p(3, 4))).toEqual({ x: 130, y: 90 });
  });

  it("keeps the point under the cursor while zooming", () => {
    const before = screenToBoard(camera, 200, 200);
    const after = screenToBoard(zoomCamera(camera, 200, 200, 2), 200, 200);
    expect(after.x).toBeCloseTo(before.x, 3);
  });

  it("shows an empty board with the origin centred", () => {
    const empty = { ...board, footprints: [], tracks: [], vias: [], zones: [] };
    expect(fitBoard(empty, 800, 600)).toEqual({ scale: 10, ox: 400, oy: 300 });
  });
});

describe("routing", () => {
  it("snaps to the chosen grid", () => {
    expect(snapTo({ x: 1.3 * MM, y: -0.3 * MM }, 0.5 * MM)).toEqual({ x: 1.5 * MM, y: -0.5 * MM });
  });

  it("goes straight when it can", () => {
    expect(route45(p(0, 0), p(5, 0))).toEqual([p(5, 0)]);
    expect(route45(p(0, 0), p(0, -5))).toEqual([p(0, -5)]);
    expect(route45(p(0, 0), p(5, 5))).toEqual([p(5, 5)]);
    expect(route45(p(0, 0), p(0, 0))).toEqual([]);
  });

  it("combines a straight part with a 45 degree part", () => {
    expect(route45(p(0, 0), p(10, 4))).toEqual([p(6, 0), p(10, 4)]);
    expect(route45(p(0, 0), p(-4, -10))).toEqual([p(0, -6), p(-4, -10)]);
  });
});

describe("hit testing", () => {
  const all = new Set([0, 1]);

  it("finds what lies under a point", () => {
    expect(hitElement(board, p(1, 1), 0, all)).toBe(1);
    expect(hitElement(board, p(15, 0), 0, all)).toBe(2);
    expect(hitElement(board, p(30, 0), 0, all)).toBe(3);
    expect(hitElement(board, p(45, 5), 0, all)).toBe(4);
    expect(hitElement(board, p(100, 100), 0, all)).toBeNull();
  });

  it("ignores copper on hidden layers", () => {
    expect(hitElement(board, p(15, 0), 0, new Set([0]))).toBeNull();
  });

  it("selects only what lies completely in the box", () => {
    expect(elementsInBox(board, p(-5, -5), p(25, 5)).sort()).toEqual([1, 2]);
    expect(elementsInBox(board, p(12, -5), p(25, 5))).toEqual([]);
  });

  it("tests polygons", () => {
    expect(pointInPolygon(p(45, 5), board.zones[0]?.outline ?? [])).toBe(true);
    expect(pointInPolygon(p(35, 5), board.zones[0]?.outline ?? [])).toBe(false);
  });

  it("checks pads with their real size", () => {
    expect(padContains(pad, p(0.9, 0))).toBe(true);
    expect(padContains(pad, p(0, 0.6))).toBe(false);
  });
});

describe("copper under the cursor", () => {
  it("takes the net from a pad and snaps to its centre", () => {
    expect(copperAt(board, p(0.5, 0), 0, 0)).toEqual({ net: "A", at: p(0, 0) });
  });

  it("does not take surface pads of another layer", () => {
    expect(copperAt(board, p(0.5, 0), 1, 0)).toBeNull();
  });

  it("takes vias from any layer", () => {
    expect(copperAt(board, p(30, 0), 0, 0)?.net).toBe("C");
    expect(copperAt(board, p(30, 0), 1, 0)?.net).toBe("C");
  });

  it("snaps to track ends and accepts mid-track starts", () => {
    expect(copperAt(board, p(10, 0), 1, 0)).toEqual({ net: "B", at: p(10, 0) });
    expect(copperAt(board, p(15, 0), 1, 0)).toEqual({ net: "B", at: p(15, 0) });
    expect(copperAt(board, p(15, 0), 0, 0)).toBeNull();
  });
});

describe("length input", () => {
  it.each([
    ["0.25", 250_000],
    ["0,25", 250_000],
    ["1", MM],
    [" 2.5 ", 2_500_000],
  ])("reads %j", (text, expected) => {
    expect(parseMm(text)).toBe(expected);
  });

  it.each(["", "abc", "-1"])("rejects %j", (text) => {
    expect(parseMm(text)).toBeNull();
  });
});
