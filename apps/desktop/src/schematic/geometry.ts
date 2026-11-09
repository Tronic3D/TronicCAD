// Pure geometry of the editor: no drawing, no state, fully unit tested.
// The placement rule must match `place` in crates/tronic-schematic/src/geometry.rs.
import {
  Clip,
  Component,
  CustomPartSpec,
  Graphic,
  PinKind,
  Point,
  Rotation,
  Segment,
  Sheet,
  SymbolDef,
} from "./types";

export const GRID = 1_270_000;

export interface Rect {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/** Screen pixels = world grid units * scale + origin. */
export interface Camera {
  ox: number;
  oy: number;
  scale: number;
}

export const toGrid = (nm: number) => nm / GRID;

export function snap(p: Point): Point {
  return {
    x: Math.round(p.x / GRID) * GRID,
    y: Math.round(p.y / GRID) * GRID,
  };
}

export function screenToWorld(camera: Camera, sx: number, sy: number): Point {
  return {
    x: ((sx - camera.ox) / camera.scale) * GRID,
    y: ((sy - camera.oy) / camera.scale) * GRID,
  };
}

export function worldToScreen(camera: Camera, p: Point): { x: number; y: number } {
  return {
    x: (p.x / GRID) * camera.scale + camera.ox,
    y: (p.y / GRID) * camera.scale + camera.oy,
  };
}

/** Zooms around a screen position, which stays over the same world point. */
export function zoomAt(camera: Camera, sx: number, sy: number, factor: number): Camera {
  const scale = Math.min(120, Math.max(0.5, camera.scale * factor));
  const applied = scale / camera.scale;
  return {
    scale,
    ox: sx - (sx - camera.ox) * applied,
    oy: sy - (sy - camera.oy) * applied,
  };
}

export function rotate(rotation: Rotation, p: Point): Point {
  switch (rotation) {
    case 90:
      return { x: p.y === 0 ? 0 : -p.y, y: p.x };
    case 180:
      return { x: p.x === 0 ? 0 : -p.x, y: p.y === 0 ? 0 : -p.y };
    case 270:
      return { x: p.y, y: p.x === 0 ? 0 : -p.x };
    default:
      return p;
  }
}

/** Symbol space to sheet space: mirror X, then rotate, then translate. */
export function place(local: Point, at: Point, rotation: Rotation, mirror: boolean): Point {
  const mirrored = mirror ? { x: local.x === 0 ? 0 : -local.x, y: local.y } : local;
  const turned = rotate(rotation, mirrored);
  return { x: turned.x + at.x, y: turned.y + at.y };
}

export function pinPoints(symbol: SymbolDef, component: Component): Point[] {
  return symbol.pins.map((pin) => place(pin.at, component.at, component.rotation, component.mirror));
}

const directions = { left: [-1, 0], right: [1, 0], up: [0, -1], down: [0, 1] } as const;

/** The end of a pin line, where it meets the symbol body. */
export function pinInnerEnd(pin: SymbolDef["pins"][number]): Point {
  const [dx, dy] = directions[pin.direction];
  return { x: pin.at.x + dx * pin.length, y: pin.at.y + dy * pin.length };
}

function graphicPoints(graphic: Graphic): Point[] {
  switch (graphic.type) {
    case "line":
    case "polygon":
      return graphic.points;
    case "rect":
      return [graphic.a, graphic.b];
    case "circle":
    case "arc":
      return [
        { x: graphic.center.x - graphic.radius, y: graphic.center.y - graphic.radius },
        { x: graphic.center.x + graphic.radius, y: graphic.center.y + graphic.radius },
      ];
    case "text":
      return [graphic.at];
  }
}

const symbolBoundsCache = new WeakMap<SymbolDef, Rect>();

/** Bounds of a symbol in symbol space. */
export function symbolBounds(symbol: SymbolDef): Rect {
  const cached = symbolBoundsCache.get(symbol);
  if (cached) return cached;
  const points = [
    ...symbol.graphics.flatMap(graphicPoints),
    ...symbol.pins.map((pin) => pin.at),
  ];
  const bounds = boundsOf(points);
  symbolBoundsCache.set(symbol, bounds);
  return bounds;
}

export function boundsOf(points: Point[]): Rect {
  const rect: Rect = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
  for (const p of points) {
    rect.minX = Math.min(rect.minX, p.x);
    rect.minY = Math.min(rect.minY, p.y);
    rect.maxX = Math.max(rect.maxX, p.x);
    rect.maxY = Math.max(rect.maxY, p.y);
  }
  return rect;
}

export function componentBounds(symbol: SymbolDef, component: Component): Rect {
  const local = symbolBounds(symbol);
  return boundsOf(
    [
      { x: local.minX, y: local.minY },
      { x: local.maxX, y: local.minY },
      { x: local.minX, y: local.maxY },
      { x: local.maxX, y: local.maxY },
    ].map((corner) => place(corner, component.at, component.rotation, component.mirror)),
  );
}

export const rectsOverlap = (a: Rect, b: Rect) =>
  a.minX <= b.maxX && b.minX <= a.maxX && a.minY <= b.maxY && b.minY <= a.maxY;

const contains = (rect: Rect, p: Point) =>
  p.x >= rect.minX && p.x <= rect.maxX && p.y >= rect.minY && p.y <= rect.maxY;

/** Rough label text size, for selecting. */
export function labelBounds(label: { at: Point; name: string }): Rect {
  const width = (label.name.length + 1) * 0.8 * GRID;
  return {
    minX: label.at.x - GRID / 2,
    minY: label.at.y - 1.5 * GRID,
    maxX: label.at.x + width,
    maxY: label.at.y + GRID / 2,
  };
}

function distanceToSegment(segment: Segment, p: Point): number {
  const clampedX = Math.max(Math.min(segment.a.x, segment.b.x), Math.min(p.x, Math.max(segment.a.x, segment.b.x)));
  const clampedY = Math.max(Math.min(segment.a.y, segment.b.y), Math.min(p.y, Math.max(segment.a.y, segment.b.y)));
  return Math.hypot(p.x - clampedX, p.y - clampedY);
}

export function segmentContains(segment: Segment, p: Point): boolean {
  return distanceToSegment(segment, p) === 0;
}

/** The element under a point. Small things win over large ones. */
export function hitTest(
  sheet: Sheet,
  symbols: Record<string, SymbolDef>,
  p: Point,
  tolerance: number,
): number | null {
  for (const marker of [...sheet.no_connects, ...sheet.junctions]) {
    if (Math.hypot(marker.at.x - p.x, marker.at.y - p.y) <= tolerance) return marker.id;
  }
  for (const label of sheet.labels) {
    if (contains(labelBounds(label), p)) return label.id;
  }
  for (const wire of sheet.wires) {
    if (distanceToSegment(wire, p) <= tolerance) return wire.id;
  }
  for (const bus of sheet.buses) {
    if (distanceToSegment(bus, p) <= tolerance) return bus.id;
  }
  for (let i = sheet.components.length - 1; i >= 0; i--) {
    const component = sheet.components[i] as Component;
    const symbol = symbols[component.symbol];
    if (symbol && contains(componentBounds(symbol, component), p)) return component.id;
  }
  for (const block of sheet.instances) {
    if (
      contains(
        {
          minX: block.at.x,
          minY: block.at.y,
          maxX: block.at.x + block.size.x,
          maxY: block.at.y + block.size.y,
        },
        p,
      )
    ) {
      return block.id;
    }
  }
  return null;
}

/** All elements completely inside a rectangle. */
export function elementsInRect(sheet: Sheet, symbols: Record<string, SymbolDef>, rect: Rect): number[] {
  const inside = (points: Point[]) => points.every((p) => contains(rect, p));
  const ids: number[] = [];
  for (const component of sheet.components) {
    const symbol = symbols[component.symbol];
    if (!symbol) continue;
    const bounds = componentBounds(symbol, component);
    if (
      inside([
        { x: bounds.minX, y: bounds.minY },
        { x: bounds.maxX, y: bounds.maxY },
      ])
    ) {
      ids.push(component.id);
    }
  }
  for (const segment of [...sheet.wires, ...sheet.buses]) {
    if (inside([segment.a, segment.b])) ids.push(segment.id);
  }
  for (const item of [...sheet.labels, ...sheet.no_connects, ...sheet.junctions]) {
    if (inside([item.at])) ids.push(item.id);
  }
  for (const block of sheet.instances) {
    if (inside([block.at, { x: block.at.x + block.size.x, y: block.at.y + block.size.y }])) {
      ids.push(block.id);
    }
  }
  return ids;
}

export function allElementIds(sheet: Sheet): number[] {
  return [
    ...sheet.components,
    ...sheet.wires,
    ...sheet.buses,
    ...sheet.labels,
    ...sheet.no_connects,
    ...sheet.junctions,
    ...sheet.instances,
  ].map((element) => element.id);
}

/** Bounds of everything on a sheet, or null if it is empty. */
export function sheetBounds(sheet: Sheet, symbols: Record<string, SymbolDef>): Rect | null {
  const points: Point[] = [];
  for (const component of sheet.components) {
    const symbol = symbols[component.symbol];
    if (symbol) {
      const bounds = componentBounds(symbol, component);
      points.push({ x: bounds.minX, y: bounds.minY }, { x: bounds.maxX, y: bounds.maxY });
    }
  }
  for (const segment of [...sheet.wires, ...sheet.buses]) points.push(segment.a, segment.b);
  for (const item of [...sheet.labels, ...sheet.no_connects, ...sheet.junctions]) points.push(item.at);
  for (const block of sheet.instances) {
    points.push(block.at, { x: block.at.x + block.size.x, y: block.at.y + block.size.y });
  }
  return points.length ? boundsOf(points) : null;
}

/** A camera showing `rect` in a viewport, with some margin. */
export function fitCamera(rect: Rect | null, width: number, height: number): Camera {
  if (!rect) return { scale: 14, ox: width / 2, oy: height / 2 };
  const w = Math.max(toGrid(rect.maxX - rect.minX), 10) + 8;
  const h = Math.max(toGrid(rect.maxY - rect.minY), 10) + 8;
  const scale = Math.min(40, Math.max(0.5, Math.min(width / w, height / h)));
  return {
    scale,
    ox: width / 2 - (toGrid(rect.minX + rect.maxX) / 2) * scale,
    oy: height / 2 - (toGrid(rect.minY + rect.maxY) / 2) * scale,
  };
}

/** The route of a new wire: horizontal first, then vertical. */
export function route(from: Point, to: Point): Point[] {
  if (from.x === to.x && from.y === to.y) return [];
  if (from.x === to.x || from.y === to.y) return [to];
  return [{ x: to.x, y: from.y }, to];
}

/** Whether a wire ending here would connect to something. */
export function isConnectionPoint(sheet: Sheet, symbols: Record<string, SymbolDef>, p: Point): boolean {
  if (sheet.wires.some((wire) => segmentContains(wire, p))) return true;
  return sheet.components.some((component) => {
    const symbol = symbols[component.symbol];
    return symbol ? pinPoints(symbol, component).some((pin) => pin.x === p.x && pin.y === p.y) : false;
  });
}

/** The selected elements as clipboard content. */
export function clipFromSelection(sheet: Sheet, selection: ReadonlySet<number>): Clip {
  const selected = <T extends { id: number }>(list: T[]) => list.filter((item) => selection.has(item.id));
  return {
    components: selected(sheet.components),
    wires: selected(sheet.wires),
    buses: selected(sheet.buses),
    labels: selected(sheet.labels),
    no_connects: selected(sheet.no_connects),
    junctions: selected(sheet.junctions),
  };
}

export function clipOrigin(clip: Clip): Point | null {
  const points = [
    ...clip.components.map((item) => item.at),
    ...clip.wires.flatMap((item) => [item.a, item.b]),
    ...clip.buses.flatMap((item) => [item.a, item.b]),
    ...clip.labels.map((item) => item.at),
    ...clip.no_connects.map((item) => item.at),
    ...clip.junctions.map((item) => item.at),
  ];
  if (!points.length) return null;
  const bounds = boundsOf(points);
  return { x: bounds.minX, y: bounds.minY };
}

const PIN_KINDS: PinKind[] = [
  "input",
  "output",
  "bidirectional",
  "tri_state",
  "passive",
  "power_in",
  "power_out",
  "open_collector",
  "open_emitter",
  "not_connected",
];

export type ParsedPins =
  | { ok: true; pins: CustomPartSpec["pins"] }
  | { ok: false; line: number };

/**
 * Parses the pin list of a custom part. One pin per line:
 * `number name [kind] [left|right]`. Kind defaults to passive, side to left.
 */
export function parsePinList(text: string): ParsedPins {
  const pins: CustomPartSpec["pins"] = [];
  const lines = text.split(/\r?\n/);
  for (const [index, raw] of lines.entries()) {
    const fields = raw.trim().split(/\s+/).filter(Boolean);
    if (fields.length === 0) continue;
    const [number, name, ...rest] = fields;
    let kind: PinKind = "passive";
    let side: "left" | "right" = "left";
    if (!number || !name || rest.length > 2) return { ok: false, line: index + 1 };
    for (const field of rest) {
      if (field === "left" || field === "right") side = field;
      else if ((PIN_KINDS as string[]).includes(field)) kind = field as PinKind;
      else return { ok: false, line: index + 1 };
    }
    pins.push({ number, name, kind, side });
  }
  return pins.length ? { ok: true, pins } : { ok: false, line: 1 };
}
