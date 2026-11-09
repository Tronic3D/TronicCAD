// Pure geometry of the board editor. Lengths in nanometres, the camera in
// pixels per millimetre.
import { BoardView, PadView, Point } from "./types";

export const MM = 1_000_000;

export interface PcbCamera {
  ox: number;
  oy: number;
  scale: number;
}

export const toMm = (nm: number) => nm / MM;

export const fromMm = (mm: number) => Math.round(mm * MM);

export function screenToBoard(camera: PcbCamera, sx: number, sy: number): Point {
  return {
    x: ((sx - camera.ox) / camera.scale) * MM,
    y: ((sy - camera.oy) / camera.scale) * MM,
  };
}

export function boardToScreen(camera: PcbCamera, p: Point): { x: number; y: number } {
  return {
    x: (p.x / MM) * camera.scale + camera.ox,
    y: (p.y / MM) * camera.scale + camera.oy,
  };
}

export function zoomCamera(camera: PcbCamera, sx: number, sy: number, factor: number): PcbCamera {
  const scale = Math.min(400, Math.max(1, camera.scale * factor));
  const applied = scale / camera.scale;
  return { scale, ox: sx - (sx - camera.ox) * applied, oy: sy - (sy - camera.oy) * applied };
}

export function snapTo(p: Point, grid: number): Point {
  return { x: Math.round(p.x / grid) * grid, y: Math.round(p.y / grid) * grid };
}

/** The way from a point to a target: straight, then 45 degrees. */
export function route45(from: Point, to: Point): Point[] {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (dx === 0 && dy === 0) return [];
  const adx = Math.abs(dx);
  const ady = Math.abs(dy);
  if (adx === 0 || ady === 0 || adx === ady) return [to];
  if (adx > ady) {
    return [{ x: from.x + Math.sign(dx) * (adx - ady), y: from.y }, to];
  }
  return [{ x: from.x, y: from.y + Math.sign(dy) * (ady - adx) }, to];
}

function distanceToSegment(p: Point, a: Point, b: Point): number {
  const abx = b.x - a.x;
  const aby = b.y - a.y;
  const length = abx * abx + aby * aby;
  const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * abx + (p.y - a.y) * aby) / length));
  return Math.hypot(p.x - (a.x + abx * t), p.y - (a.y + aby * t));
}

export function pointInPolygon(p: Point, polygon: Point[]): boolean {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i] as Point;
    const b = polygon[j] as Point;
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) {
      inside = !inside;
    }
  }
  return inside;
}

export function padContains(pad: PadView, p: Point, tolerance = 0): boolean {
  return (
    Math.abs(p.x - pad.at.x) <= pad.size.x / 2 + tolerance &&
    Math.abs(p.y - pad.at.y) <= pad.size.y / 2 + tolerance
  );
}

/** Copper under a point that could start or end a track, with its net. */
export function copperAt(
  board: BoardView,
  p: Point,
  layer: number,
  tolerance: number,
): { net: string; at: Point } | null {
  for (const pad of board.pads) {
    if (pad.net && (pad.layer === null || pad.layer === layer) && padContains(pad, p, tolerance)) {
      return { net: pad.net, at: pad.at };
    }
  }
  for (const via of board.vias) {
    if (Math.hypot(via.at.x - p.x, via.at.y - p.y) <= via.diameter / 2 + tolerance) {
      return { net: via.net, at: via.at };
    }
  }
  for (const track of board.tracks) {
    if (track.layer === layer && distanceToSegment(p, track.a, track.b) <= track.width / 2 + tolerance) {
      for (const end of [track.a, track.b]) {
        if (Math.hypot(end.x - p.x, end.y - p.y) <= track.width / 2 + tolerance) {
          return { net: track.net, at: end };
        }
      }
      return { net: track.net, at: p };
    }
  }
  return null;
}

export interface LayerVisibility {
  copper: ReadonlySet<number>;
}

/** The element under a point. Small things win over large ones. */
export function hitElement(
  board: BoardView,
  p: Point,
  tolerance: number,
  visibleLayers: ReadonlySet<number>,
): number | null {
  for (const via of board.vias) {
    if (Math.hypot(via.at.x - p.x, via.at.y - p.y) <= via.diameter / 2 + tolerance) return via.id;
  }
  for (const track of board.tracks) {
    if (visibleLayers.has(track.layer) && distanceToSegment(p, track.a, track.b) <= track.width / 2 + tolerance) {
      return track.id;
    }
  }
  for (const footprint of board.footprints) {
    const [min, max] = footprint.courtyard;
    if (p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y) return footprint.id;
  }
  for (const zone of board.zones) {
    if (visibleLayers.has(zone.layer) && pointInPolygon(p, zone.outline)) return zone.id;
  }
  return null;
}

export function elementsInBox(board: BoardView, min: Point, max: Point): number[] {
  const inside = (p: Point) => p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
  return [
    ...board.footprints.filter((f) => f.courtyard.every(inside)),
    ...board.tracks.filter((t) => inside(t.a) && inside(t.b)),
    ...board.vias.filter((v) => inside(v.at)),
    ...board.zones.filter((z) => z.outline.every(inside)),
  ].map((element) => element.id);
}

export function boardBounds(board: BoardView): { min: Point; max: Point } | null {
  const points: Point[] = [
    ...board.outline,
    ...board.footprints.flatMap((f) => f.courtyard),
    ...board.tracks.flatMap((t) => [t.a, t.b]),
    ...board.vias.map((v) => v.at),
  ];
  if (!points.length) return null;
  return {
    min: { x: Math.min(...points.map((p) => p.x)), y: Math.min(...points.map((p) => p.y)) },
    max: { x: Math.max(...points.map((p) => p.x)), y: Math.max(...points.map((p) => p.y)) },
  };
}

export function fitBoard(board: BoardView, width: number, height: number): PcbCamera {
  const bounds = boardBounds(board);
  if (!bounds) return { scale: 10, ox: width / 2, oy: height / 2 };
  const w = toMm(bounds.max.x - bounds.min.x) + 10;
  const h = toMm(bounds.max.y - bounds.min.y) + 10;
  const scale = Math.max(1, Math.min(60, Math.min(width / w, height / h)));
  return {
    scale,
    ox: width / 2 - (toMm(bounds.min.x + bounds.max.x) / 2) * scale,
    oy: height / 2 - (toMm(bounds.min.y + bounds.max.y) / 2) * scale,
  };
}

/** Parses a length in millimetres, with comma or dot. Returns null if unusable. */
export function parseMm(text: string): number | null {
  const value = Number(text.trim().replace(",", "."));
  if (!Number.isFinite(value) || value < 0 || text.trim() === "") return null;
  return fromMm(value);
}
