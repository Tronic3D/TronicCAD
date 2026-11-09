import { describe, expect, it } from "vitest";
import { boundsOf, Camera, fitted, panned, project, radiusOf, rotated, Vec3, viewProjection, zoomed } from "./camera";

const BOUNDS = { min: [0, -40, -1.6] as Vec3, max: [50, 0, 10] as Vec3 };
const corners: Vec3[] = [
  [0, -40, -1.6],
  [50, -40, -1.6],
  [0, 0, -1.6],
  [50, 0, -1.6],
  [0, -40, 10],
  [50, -40, 10],
  [0, 0, 10],
  [50, 0, 10],
];

function at(camera: Camera, aspect = 1.5) {
  const matrix = viewProjection(camera, aspect, radiusOf(BOUNDS));
  return (point: Vec3) => project(matrix, point);
}

describe("3D camera", () => {
  it("looks from above with X to the right and Y up", () => {
    const see = at(fitted(BOUNDS, 1.5, "top", true));
    const center = see([25, -20, 0]);
    expect(see([50, -20, 0])[0]).toBeGreaterThan(center[0]);
    expect(see([25, 0, 0])[1]).toBeGreaterThan(center[1]);
  });

  it("looks from below without mirroring: X stays right, Y turns over", () => {
    const see = at(fitted(BOUNDS, 1.5, "bottom", true));
    const center = see([25, -20, 0]);
    expect(see([50, -20, 0])[0]).toBeGreaterThan(center[0]);
    expect(see([25, 0, 0])[1]).toBeLessThan(center[1]);
  });

  it("shows a taller part higher in the front view", () => {
    const see = at(fitted(BOUNDS, 1.5, "front", false));
    expect(see([25, -20, 10])[1]).toBeGreaterThan(see([25, -20, 0])[1]);
  });

  it.each(["top", "bottom", "front", "side", "iso"] as const)("fits the whole scene in the %s view", (preset) => {
    for (const aspect of [0.5, 1, 2.5]) {
      for (const perspective of [true, false]) {
        const see = at(fitted(BOUNDS, aspect, preset, perspective), aspect);
        for (const corner of corners) {
          const [x, y] = see(corner);
          expect(Math.abs(x)).toBeLessThanOrEqual(1);
          expect(Math.abs(y)).toBeLessThanOrEqual(1);
        }
      }
    }
  });

  it("makes far things smaller only in perspective", () => {
    const base = fitted(BOUNDS, 1, "front", true);
    const size = (camera: Camera, y: number) => {
      const see = at(camera, 1);
      return see([30, y, 0])[0] - see([20, y, 0])[0];
    };
    expect(size(base, -40)).toBeGreaterThan(size(base, 0));
    const flat = { ...base, perspective: false };
    expect(size(flat, -40)).toBeCloseTo(size(flat, 0), 5);
  });

  it("keeps the elevation short of the poles and the azimuth in range", () => {
    const camera = fitted(BOUNDS, 1, "iso", true);
    expect(rotated(camera, 0, 100000).elevation).toBeLessThan(90);
    expect(rotated(camera, 0, -100000).elevation).toBeGreaterThan(-90);
    const azimuth = rotated(camera, 100000, 0).azimuth;
    expect(Math.abs(azimuth)).toBeLessThanOrEqual(180);
  });

  it("moves the content along with the pointer when panning", () => {
    const camera = fitted(BOUNDS, 1, "top", false);
    const point: Vec3 = [25, -20, 0];
    const before = at(camera, 1)(point);
    const after = at(panned(camera, 100, 0, 1000), 1)(point);
    // 100 of 1000 pixels are 0.2 of the -1 to 1 range.
    expect(after[0] - before[0]).toBeCloseTo(0.2, 4);
    expect(after[1]).toBeCloseTo(before[1], 4);
  });

  it("limits the zoom", () => {
    const camera = fitted(BOUNDS, 1, "iso", true);
    const radius = radiusOf(BOUNDS);
    expect(zoomed(camera, 1e-9, radius).distance).toBeCloseTo(radius * 0.02);
    expect(zoomed(camera, 1e9, radius).distance).toBeCloseTo(radius * 40);
  });

  it("finds bounds across meshes and copes with an empty scene", () => {
    expect(boundsOf([[1, 2, 3], [4, -5, 6]])).toEqual({ min: [1, -5, 3], max: [4, 2, 6] });
    expect(boundsOf([[]]).max[0]).toBe(10);
  });
});
