// Draws a schematic with Canvas 2D (docs/adr/0001-schematic-rendering.md).
// Independent of React: it turns a scene into pixels and keeps no state.
import {
  Camera,
  componentBounds,
  GRID,
  labelBounds,
  place,
  pinInnerEnd,
  Rect,
  rectsOverlap,
  screenToWorld,
  toGrid,
} from "./geometry";
import { Component, Graphic, Label, PinDef, Point, Rotation, SchematicView, Segment, SymbolDef, Violation } from "./types";

const FONT = '"Segoe UI", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", sans-serif';

const color = {
  background: "#0f1a1f",
  grid: "#2b4149",
  wire: "#35c6a8",
  bus: "#6aa8ff",
  symbol: "#e6eef0",
  pin: "#93a8af",
  pinText: "#93a8af",
  reference: "#f0a35e",
  value: "#c4d3d7",
  label: "#7fe3cf",
  global: "#f0a35e",
  block: "#b49cff",
  selected: "#ffd166",
  highlight: "#ffffff",
  error: "#f0776c",
  warning: "#ffd166",
  ghost: "#7fb2bd",
  box: "#35c6a8",
  noConnect: "#f0776c",
};

export interface Overlay {
  selection: ReadonlySet<number>;
  highlight: { elements: ReadonlySet<number>; pins: Point[] } | null;
  violations: Violation[];
  /** Preview of the selection being moved, as an offset. */
  moveDelta: Point | null;
  /** Wire or bus being drawn. */
  routing: { points: Point[]; bus: boolean } | null;
  ghost: { symbol: SymbolDef; at: Point; rotation: Rotation; mirror: boolean } | null;
  box: Rect | null;
  cursor: Point | null;
}

interface Frame {
  ctx: CanvasRenderingContext2D;
  scale: number;
  visible: Rect;
}

function setFont(ctx: CanvasRenderingContext2D, size: number) {
  ctx.font = `${size}px ${FONT}`;
}

function polyline(ctx: CanvasRenderingContext2D, points: Point[]) {
  points.forEach((p, i) => {
    if (i === 0) ctx.moveTo(toGrid(p.x), toGrid(p.y));
    else ctx.lineTo(toGrid(p.x), toGrid(p.y));
  });
}

function drawGraphic(ctx: CanvasRenderingContext2D, graphic: Graphic) {
  ctx.beginPath();
  switch (graphic.type) {
    case "line":
      polyline(ctx, graphic.points);
      ctx.stroke();
      break;
    case "polygon":
      polyline(ctx, graphic.points);
      ctx.closePath();
      if (graphic.filled) ctx.fill();
      ctx.stroke();
      break;
    case "rect":
      ctx.rect(
        toGrid(graphic.a.x),
        toGrid(graphic.a.y),
        toGrid(graphic.b.x - graphic.a.x),
        toGrid(graphic.b.y - graphic.a.y),
      );
      ctx.stroke();
      break;
    case "circle":
      ctx.arc(toGrid(graphic.center.x), toGrid(graphic.center.y), toGrid(graphic.radius), 0, Math.PI * 2);
      if (graphic.filled) ctx.fill();
      ctx.stroke();
      break;
    case "arc":
      ctx.arc(
        toGrid(graphic.center.x),
        toGrid(graphic.center.y),
        toGrid(graphic.radius),
        (graphic.start_deg * Math.PI) / 180,
        (graphic.end_deg * Math.PI) / 180,
      );
      ctx.stroke();
      break;
    case "text":
      // Drawn upright afterwards.
      break;
  }
}

function drawPinText(
  frame: Frame,
  pin: PinDef,
  at: Point,
  rotation: Rotation,
  mirror: boolean,
) {
  const { ctx, scale } = frame;
  if (scale < 6) return;
  const inner = place(pinInnerEnd(pin), at, rotation, mirror);
  const outer = place(pin.at, at, rotation, mirror);
  const dx = Math.sign(inner.x - outer.x);
  const dy = Math.sign(inner.y - outer.y);
  ctx.fillStyle = color.pinText;
  setFont(ctx, 0.85);
  if (!pin.hide_number) {
    ctx.textAlign = "center";
    ctx.textBaseline = "bottom";
    const midX = toGrid(outer.x + inner.x) / 2;
    const midY = toGrid(outer.y + inner.y) / 2;
    if (dy === 0) ctx.fillText(pin.number, midX, midY - 0.1);
    else {
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(pin.number, midX - 0.2, midY);
    }
  }
  if (!pin.hide_name) {
    ctx.textBaseline = "middle";
    const x = toGrid(inner.x) + dx * 0.4;
    const y = toGrid(inner.y) + dy * 0.8;
    ctx.textAlign = dx > 0 ? "left" : dx < 0 ? "right" : "center";
    ctx.fillText(pin.name, x, y);
  }
}

export function drawSymbol(
  frame: Frame,
  symbol: SymbolDef,
  at: Point,
  rotation: Rotation,
  mirror: boolean,
  stroke: string,
) {
  const { ctx, scale } = frame;
  ctx.save();
  ctx.translate(toGrid(at.x), toGrid(at.y));
  ctx.rotate((rotation * Math.PI) / 180);
  if (mirror) ctx.scale(-1, 1);
  ctx.strokeStyle = stroke;
  ctx.fillStyle = stroke;
  ctx.lineWidth = 0.15;
  for (const graphic of symbol.graphics) drawGraphic(ctx, graphic);
  ctx.strokeStyle = color.pin;
  ctx.beginPath();
  for (const pin of symbol.pins) {
    const inner = pinInnerEnd(pin);
    ctx.moveTo(toGrid(pin.at.x), toGrid(pin.at.y));
    ctx.lineTo(toGrid(inner.x), toGrid(inner.y));
  }
  ctx.stroke();
  ctx.restore();

  // Text stays upright however the symbol is turned.
  for (const graphic of symbol.graphics) {
    if (graphic.type === "text") {
      const p = place(graphic.at, at, rotation, mirror);
      ctx.fillStyle = stroke;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      setFont(ctx, toGrid(graphic.size) * 1.2);
      ctx.fillText(graphic.text, toGrid(p.x), toGrid(p.y));
    }
  }
  if (scale >= 6) {
    for (const pin of symbol.pins) drawPinText(frame, pin, at, rotation, mirror);
  }
}

function drawComponent(
  frame: Frame,
  component: Component,
  symbol: SymbolDef,
  selected: boolean,
) {
  const { ctx, scale } = frame;
  drawSymbol(
    frame,
    symbol,
    component.at,
    component.rotation,
    component.mirror,
    selected ? color.selected : color.symbol,
  );
  if (scale < 3) return;
  const bounds = componentBounds(symbol, component);
  ctx.textBaseline = "middle";
  setFont(ctx, 1.1);
  if (symbol.kind === "supply_port") {
    const above = component.rotation === 0 && symbol.pins[0]?.direction === "up";
    ctx.textAlign = "center";
    ctx.fillStyle = selected ? color.selected : color.global;
    ctx.fillText(
      component.value,
      toGrid(component.at.x),
      above ? toGrid(bounds.minY) - 0.9 : toGrid(bounds.maxY) + 0.9,
    );
    return;
  }
  const x = toGrid(bounds.maxX) + 0.5;
  const y = toGrid(bounds.minY + bounds.maxY) / 2;
  ctx.textAlign = "left";
  ctx.fillStyle = selected ? color.selected : color.reference;
  ctx.fillText(component.reference, x, y - 0.8);
  ctx.fillStyle = selected ? color.selected : color.value;
  ctx.fillText(component.value, x, y + 0.8);
}

function drawLabel(
  frame: Frame,
  label: Label,
  crossReference: string[] | undefined,
  style: string,
) {
  const { ctx } = frame;
  const x = toGrid(label.at.x);
  const y = toGrid(label.at.y);
  ctx.save();
  ctx.translate(x, y);
  if (label.rotation === 90 || label.rotation === 270) ctx.rotate(-Math.PI / 2);
  ctx.strokeStyle = style;
  ctx.fillStyle = style;
  ctx.lineWidth = 0.12;
  setFont(ctx, 1.1);
  ctx.textBaseline = "bottom";
  ctx.textAlign = "left";
  const textWidth = ctx.measureText(label.name).width;
  if (label.kind === "local") {
    ctx.fillText(label.name, 0.2, -0.2);
  } else {
    const flagged = label.kind === "global";
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(0.8, -0.8);
    ctx.lineTo(1.4 + textWidth, -0.8);
    ctx.lineTo(flagged ? 2 + textWidth : 1.4 + textWidth, flagged ? 0 : 0.8);
    if (flagged) ctx.lineTo(1.4 + textWidth, 0.8);
    ctx.lineTo(0.8, 0.8);
    ctx.closePath();
    ctx.stroke();
    ctx.textBaseline = "middle";
    ctx.fillText(label.name, 1.1, 0.05);
    if (crossReference?.length) {
      ctx.globalAlpha = 0.8;
      setFont(ctx, 0.85);
      ctx.fillText(`[${crossReference.join(", ")}]`, 2.6 + textWidth, 0.05);
      ctx.globalAlpha = 1;
    }
  }
  ctx.restore();
}

function segmentBounds(segment: Segment, margin: number): Rect {
  return {
    minX: Math.min(segment.a.x, segment.b.x) - margin,
    minY: Math.min(segment.a.y, segment.b.y) - margin,
    maxX: Math.max(segment.a.x, segment.b.x) + margin,
    maxY: Math.max(segment.a.y, segment.b.y) + margin,
  };
}

function drawGrid(frame: Frame, width: number, height: number, camera: Camera) {
  const { ctx } = frame;
  // Dots at least 12 pixels apart, so that their count stays bounded.
  const step = [1, 2, 5, 10, 20, 50, 100].find((s) => s * camera.scale >= 12) ?? 200;
  const startX = Math.ceil(-camera.ox / camera.scale / step) * step;
  const startY = Math.ceil(-camera.oy / camera.scale / step) * step;
  const size = Math.max(1.4 / camera.scale, 0.05);
  ctx.fillStyle = color.grid;
  ctx.beginPath();
  for (let x = startX; x * camera.scale + camera.ox < width; x += step) {
    for (let y = startY; y * camera.scale + camera.oy < height; y += step) {
      ctx.rect(x - size / 2, y - size / 2, size, size);
    }
  }
  ctx.fill();
}

const offsetPoint = (p: Point, delta: Point | null): Point =>
  delta ? { x: p.x + delta.x, y: p.y + delta.y } : p;

/** Draws one frame. Sizes are in CSS pixels. */
export function drawScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  pixelRatio: number,
  camera: Camera,
  view: SchematicView,
  overlay: Overlay,
) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = color.background;
  ctx.fillRect(0, 0, width * pixelRatio, height * pixelRatio);
  ctx.setTransform(
    pixelRatio * camera.scale,
    0,
    0,
    pixelRatio * camera.scale,
    pixelRatio * camera.ox,
    pixelRatio * camera.oy,
  );
  const topLeft = screenToWorld(camera, 0, 0);
  const bottomRight = screenToWorld(camera, width, height);
  // A wide margin keeps symbols and text whose position lies just outside.
  const margin = 4 * GRID;
  const visible: Rect = {
    minX: topLeft.x - margin,
    minY: topLeft.y - margin,
    maxX: bottomRight.x + margin,
    maxY: bottomRight.y + margin,
  };
  const frame: Frame = { ctx, scale: camera.scale, visible };
  drawGrid(frame, width, height, camera);

  const { sheet, symbols } = view;
  const { selection, highlight, moveDelta } = overlay;
  const isMoved = (id: number) => moveDelta !== null && selection.has(id);
  const delta = (id: number) => (isMoved(id) ? moveDelta : null);

  // Blocks.
  for (const block of sheet.instances) {
    const at = offsetPoint(block.at, delta(block.id));
    const selected = selection.has(block.id);
    const blockSheet = view.sheets.find((entry) => entry.id === block.sheet);
    ctx.strokeStyle = selected ? color.selected : color.block;
    ctx.fillStyle = selected ? color.selected : color.block;
    ctx.lineWidth = 0.2;
    ctx.strokeRect(toGrid(at.x), toGrid(at.y), toGrid(block.size.x), toGrid(block.size.y));
    setFont(ctx, 1.2);
    ctx.textAlign = "left";
    ctx.textBaseline = "bottom";
    ctx.fillText(blockSheet?.name ?? "", toGrid(at.x), toGrid(at.y) - 0.3);
    setFont(ctx, 1);
    ctx.textBaseline = "middle";
    for (const port of block.ports) {
      const p = offsetPoint(at, port.at);
      ctx.fillRect(toGrid(p.x) - 0.25, toGrid(p.y) - 0.25, 0.5, 0.5);
      ctx.fillText(port.name, toGrid(p.x) + 0.6, toGrid(p.y));
    }
  }

  // Wires and buses.
  const drawSegments = (segments: Segment[], base: string, lineWidth: number) => {
    const groups = new Map<string, Segment[]>();
    for (const segment of segments) {
      const moved = delta(segment.id);
      const shown = moved
        ? { ...segment, a: offsetPoint(segment.a, moved), b: offsetPoint(segment.b, moved) }
        : segment;
      if (!rectsOverlap(segmentBounds(shown, 0), visible)) continue;
      const style = selection.has(segment.id)
        ? color.selected
        : highlight?.elements.has(segment.id)
          ? color.highlight
          : base;
      let group = groups.get(style);
      if (!group) groups.set(style, (group = []));
      group.push(shown);
    }
    ctx.lineWidth = lineWidth;
    ctx.lineCap = "round";
    for (const [style, group] of groups) {
      ctx.strokeStyle = style;
      ctx.beginPath();
      for (const segment of group) polyline(ctx, [segment.a, segment.b]);
      ctx.stroke();
    }
  };
  drawSegments(sheet.wires, color.wire, 0.2);
  drawSegments(sheet.buses, color.bus, 0.5);

  // Junction dots.
  ctx.fillStyle = color.wire;
  ctx.beginPath();
  for (const p of view.junctions) {
    ctx.moveTo(toGrid(p.x) + 0.35, toGrid(p.y));
    ctx.arc(toGrid(p.x), toGrid(p.y), 0.35, 0, Math.PI * 2);
  }
  ctx.fill();

  // Components.
  for (const component of sheet.components) {
    const symbol = symbols[component.symbol];
    if (!symbol) continue;
    const moved = delta(component.id);
    const shown = moved ? { ...component, at: offsetPoint(component.at, moved) } : component;
    if (!rectsOverlap(componentBounds(symbol, shown), visible)) continue;
    drawComponent(frame, shown, symbol, selection.has(component.id));
  }

  // Labels.
  const crossReferences = new Map(view.cross_references.map((entry) => [entry.label, entry.sheets]));
  for (const label of sheet.labels) {
    const moved = delta(label.id);
    const shown = moved ? { ...label, at: offsetPoint(label.at, moved) } : label;
    if (!rectsOverlap(labelBounds(shown), visible)) continue;
    const style = selection.has(label.id)
      ? color.selected
      : highlight?.elements.has(label.id)
        ? color.highlight
        : label.kind === "global"
          ? color.global
          : label.kind === "hierarchical"
            ? color.block
            : color.label;
    drawLabel(frame, shown, crossReferences.get(label.id), style);
  }

  // No-connect markers.
  ctx.lineWidth = 0.2;
  for (const marker of sheet.no_connects) {
    const p = offsetPoint(marker.at, delta(marker.id));
    const x = toGrid(p.x);
    const y = toGrid(p.y);
    ctx.strokeStyle = selection.has(marker.id) ? color.selected : color.noConnect;
    ctx.beginPath();
    ctx.moveTo(x - 0.5, y - 0.5);
    ctx.lineTo(x + 0.5, y + 0.5);
    ctx.moveTo(x + 0.5, y - 0.5);
    ctx.lineTo(x - 0.5, y + 0.5);
    ctx.stroke();
  }
  for (const marker of sheet.junctions) {
    if (selection.has(marker.id)) {
      ctx.strokeStyle = color.selected;
      ctx.beginPath();
      ctx.arc(toGrid(marker.at.x), toGrid(marker.at.y), 0.7, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  // Highlighted pins.
  if (highlight) {
    ctx.fillStyle = color.highlight;
    for (const p of highlight.pins) {
      ctx.beginPath();
      ctx.arc(toGrid(p.x), toGrid(p.y), 0.4, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  // Rule check findings, drawn directly into the graphic. Their size stays
  // readable at any zoom.
  const markerRadius = Math.max(0.8, 9 / camera.scale);
  for (const violation of overlay.violations) {
    if (violation.sheet !== sheet.id) continue;
    const x = toGrid(violation.at.x);
    const y = toGrid(violation.at.y);
    const style = violation.severity === "error" ? color.error : color.warning;
    ctx.strokeStyle = style;
    ctx.lineWidth = Math.max(0.15, 2 / camera.scale);
    ctx.beginPath();
    ctx.arc(x, y, markerRadius, 0, Math.PI * 2);
    ctx.stroke();
    ctx.globalAlpha = 0.25;
    ctx.fillStyle = style;
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  // Tool previews.
  if (overlay.routing) {
    ctx.strokeStyle = overlay.routing.bus ? color.bus : color.wire;
    ctx.lineWidth = overlay.routing.bus ? 0.5 : 0.2;
    ctx.setLineDash([0.6, 0.4]);
    ctx.beginPath();
    polyline(ctx, overlay.routing.points);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  if (overlay.ghost) {
    const { symbol, at, rotation, mirror } = overlay.ghost;
    ctx.globalAlpha = 0.7;
    drawSymbol(frame, symbol, at, rotation, mirror, color.ghost);
    ctx.globalAlpha = 1;
  }
  if (overlay.box) {
    const { minX, minY, maxX, maxY } = overlay.box;
    ctx.strokeStyle = color.box;
    ctx.lineWidth = 1 / camera.scale;
    ctx.fillStyle = "rgb(53 198 168 / 12%)";
    ctx.fillRect(toGrid(minX), toGrid(minY), toGrid(maxX - minX), toGrid(maxY - minY));
    ctx.strokeRect(toGrid(minX), toGrid(minY), toGrid(maxX - minX), toGrid(maxY - minY));
  }
  if (overlay.cursor) {
    const size = 8 / camera.scale;
    const x = toGrid(overlay.cursor.x);
    const y = toGrid(overlay.cursor.y);
    ctx.strokeStyle = color.highlight;
    ctx.lineWidth = 1 / camera.scale;
    ctx.beginPath();
    ctx.moveTo(x - size, y);
    ctx.lineTo(x + size, y);
    ctx.moveTo(x, y - size);
    ctx.lineTo(x, y + size);
    ctx.stroke();
  }
}
