// Draws a board with Canvas 2D, in millimetres (docs/adr/0001-schematic-rendering.md).
import { MM, PcbCamera, screenToBoard, toMm } from "./geometry";
import { BoardView, DrcViolation, PadView, Point, RouteResult } from "./types";

const FONT = '"Segoe UI", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", sans-serif';

const LAYER_COLORS = ["#d9534f", "#e6c229", "#4fc47f", "#c774e8", "#f08a3c", "#56c8d8", "#a0a0ff", "#4a90e2"];

/** Top red, bottom blue, inner layers in between, whatever the count. */
export function layerColor(layer: number, count: number): string {
  if (layer === 0) return LAYER_COLORS[0] as string;
  if (layer === count - 1) return LAYER_COLORS[7] as string;
  return LAYER_COLORS[layer] as string;
}

const color = {
  background: "#0b1418",
  board: "#12232a",
  outline: "#ffd166",
  grid: "#1f333b",
  silkTop: "#e6eef0",
  silkBottom: "#7fa0aa",
  through: "#d4a94e",
  hole: "#0b1418",
  selected: "#ffffff",
  airwire: "#7fe3cf",
  error: "#ff5d52",
  warning: "#ffd166",
  preview: "#ffffff",
  courtyard: "#47636d",
};

export interface PcbOverlay {
  selection: ReadonlySet<number>;
  visible: ReadonlySet<number>;
  activeLayer: number;
  violations: DrcViolation[];
  moveDelta: Point | null;
  /** Track being routed, with its layer. */
  routing: { points: Point[]; layer: number; width: number } | null;
  /** Polygon being drawn. */
  polygon: Point[] | null;
  box: { min: Point; max: Point } | null;
  cursor: Point | null;
  /** Router result, not yet accepted. */
  proposal: RouteResult | null;
}

function path(ctx: CanvasRenderingContext2D, points: Point[], close: boolean) {
  points.forEach((p, i) => {
    if (i === 0) ctx.moveTo(toMm(p.x), toMm(p.y));
    else ctx.lineTo(toMm(p.x), toMm(p.y));
  });
  if (close) ctx.closePath();
}

function padPath(ctx: CanvasRenderingContext2D, pad: PadView, at: Point) {
  const x = toMm(at.x);
  const y = toMm(at.y);
  const w = toMm(pad.size.x);
  const h = toMm(pad.size.y);
  ctx.beginPath();
  if (pad.shape === "rect") {
    ctx.rect(x - w / 2, y - h / 2, w, h);
  } else if (pad.shape === "round") {
    ctx.arc(x, y, w / 2, 0, Math.PI * 2);
  } else {
    const r = Math.min(w, h) / 2;
    ctx.roundRect(x - w / 2, y - h / 2, w, h, r);
  }
}

const shifted = (p: Point, delta: Point | null): Point =>
  delta ? { x: p.x + delta.x, y: p.y + delta.y } : p;

function drawGrid(ctx: CanvasRenderingContext2D, camera: PcbCamera, width: number, height: number) {
  const step = [0.5, 1, 2, 5, 10, 20, 50, 100].find((s) => s * camera.scale >= 14) ?? 200;
  const size = 1.4 / camera.scale;
  const startX = Math.ceil(-camera.ox / camera.scale / step) * step;
  const startY = Math.ceil(-camera.oy / camera.scale / step) * step;
  ctx.fillStyle = color.grid;
  ctx.beginPath();
  for (let x = startX; x * camera.scale + camera.ox < width; x += step) {
    for (let y = startY; y * camera.scale + camera.oy < height; y += step) {
      ctx.rect(x - size / 2, y - size / 2, size, size);
    }
  }
  ctx.fill();
}

/** Draws one frame. Sizes are in CSS pixels. */
export function drawBoard(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  ratio: number,
  camera: PcbCamera,
  board: BoardView,
  overlay: PcbOverlay,
) {
  const { selection, visible, moveDelta } = overlay;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = color.background;
  ctx.fillRect(0, 0, width * ratio, height * ratio);
  ctx.setTransform(ratio * camera.scale, 0, 0, ratio * camera.scale, ratio * camera.ox, ratio * camera.oy);
  drawGrid(ctx, camera, width, height);

  const pixel = 1 / camera.scale;
  const topLeft = screenToBoard(camera, 0, 0);
  const bottomRight = screenToBoard(camera, width, height);
  const onScreen = (a: Point, b: Point, margin: number) =>
    Math.max(a.x, b.x) + margin >= topLeft.x &&
    Math.min(a.x, b.x) - margin <= bottomRight.x &&
    Math.max(a.y, b.y) + margin >= topLeft.y &&
    Math.min(a.y, b.y) - margin <= bottomRight.y;
  const delta = (id: number) => (moveDelta && selection.has(id) ? moveDelta : null);

  // Board area.
  if (board.outline.length >= 3) {
    ctx.beginPath();
    path(ctx, board.outline, true);
    ctx.fillStyle = color.board;
    ctx.fill();
    ctx.strokeStyle = color.outline;
    ctx.lineWidth = Math.max(0.1, 1.5 * pixel);
    ctx.stroke();
  }

  // Copper from the far layers to the active one on top.
  const order = [...Array(board.copper_layers).keys()]
    .filter((layer) => visible.has(layer))
    .sort((a, b) => (a === overlay.activeLayer ? 1 : b === overlay.activeLayer ? -1 : b - a));

  for (const layer of order) {
    const style = layerColor(layer, board.copper_layers);
    ctx.globalAlpha = layer === overlay.activeLayer ? 0.9 : 0.55;
    ctx.fillStyle = style;
    ctx.strokeStyle = style;

    ctx.globalAlpha *= 0.55;
    for (const fill of board.fills) {
      if (fill.layer !== layer) continue;
      for (const shape of fill.shapes) {
        ctx.beginPath();
        for (const contour of shape) path(ctx, contour, true);
        ctx.fill("evenodd");
      }
    }
    ctx.globalAlpha = layer === overlay.activeLayer ? 0.9 : 0.55;

    for (const zone of board.zones) {
      if (zone.layer !== layer) continue;
      ctx.save();
      ctx.lineWidth = selection.has(zone.id) ? 3 * pixel : pixel;
      ctx.setLineDash([6 * pixel, 4 * pixel]);
      if (selection.has(zone.id)) ctx.strokeStyle = color.selected;
      const moved = delta(zone.id);
      ctx.beginPath();
      path(ctx, zone.outline.map((p) => shifted(p, moved)), true);
      ctx.stroke();
      ctx.restore();
    }

    ctx.lineCap = "round";
    for (const track of board.tracks) {
      if (track.layer !== layer) continue;
      const moved = delta(track.id);
      const a = shifted(track.a, moved);
      const b = shifted(track.b, moved);
      if (!onScreen(a, b, track.width)) continue;
      ctx.lineWidth = toMm(track.width);
      ctx.strokeStyle = selection.has(track.id) ? color.selected : style;
      ctx.beginPath();
      ctx.moveTo(toMm(a.x), toMm(a.y));
      ctx.lineTo(toMm(b.x), toMm(b.y));
      ctx.stroke();
    }

    for (const pad of board.pads) {
      if (pad.layer !== layer || !pad.plated) continue;
      const at = shifted(pad.at, delta(pad.instance));
      if (!onScreen(at, at, Math.max(pad.size.x, pad.size.y))) continue;
      padPath(ctx, pad, at);
      ctx.fillStyle = selection.has(pad.instance) ? color.selected : style;
      ctx.fill();
    }
    ctx.fillStyle = style;
  }
  ctx.globalAlpha = 1;

  // Through-hole pads and vias reach all layers.
  for (const pad of board.pads) {
    if (pad.layer !== null) continue;
    const at = shifted(pad.at, delta(pad.instance));
    if (!onScreen(at, at, Math.max(pad.size.x, pad.size.y))) continue;
    if (pad.plated) {
      padPath(ctx, pad, at);
      ctx.fillStyle = selection.has(pad.instance) ? color.selected : color.through;
      ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(toMm(at.x), toMm(at.y), toMm(pad.drill ?? 0) / 2, 0, Math.PI * 2);
    ctx.fillStyle = color.hole;
    ctx.fill();
    if (!pad.plated) {
      ctx.strokeStyle = selection.has(pad.instance) ? color.selected : color.through;
      ctx.lineWidth = pixel;
      ctx.stroke();
    }
  }
  for (const via of board.vias) {
    const at = shifted(via.at, delta(via.id));
    if (!onScreen(at, at, via.diameter)) continue;
    ctx.beginPath();
    ctx.arc(toMm(at.x), toMm(at.y), toMm(via.diameter) / 2, 0, Math.PI * 2);
    ctx.fillStyle = selection.has(via.id) ? color.selected : color.through;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(toMm(at.x), toMm(at.y), toMm(via.drill) / 2, 0, Math.PI * 2);
    ctx.fillStyle = color.hole;
    ctx.fill();
  }

  // Silkscreen, courtyards and references.
  ctx.lineCap = "butt";
  for (const footprint of board.footprints) {
    const moved = delta(footprint.id);
    const min = shifted(footprint.courtyard[0], moved);
    const max = shifted(footprint.courtyard[1], moved);
    if (!onScreen(min, max, 0)) continue;
    const selected = selection.has(footprint.id);
    ctx.strokeStyle = selected ? color.selected : color.courtyard;
    ctx.lineWidth = pixel;
    ctx.strokeRect(toMm(min.x), toMm(min.y), toMm(max.x - min.x), toMm(max.y - min.y));
    ctx.strokeStyle = footprint.bottom ? color.silkBottom : color.silkTop;
    ctx.lineWidth = 0.15;
    ctx.beginPath();
    for (const [a, b] of footprint.silk) {
      const p = shifted(a, moved);
      const q = shifted(b, moved);
      ctx.moveTo(toMm(p.x), toMm(p.y));
      ctx.lineTo(toMm(q.x), toMm(q.y));
    }
    ctx.stroke();
    if (camera.scale >= 3) {
      ctx.fillStyle = selected ? color.selected : footprint.bottom ? color.silkBottom : color.silkTop;
      ctx.font = `1px ${FONT}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      ctx.fillText(footprint.reference, toMm(min.x + max.x) / 2, toMm(min.y) - 0.2);
    }
  }

  // Pad numbers when zoomed in.
  if (camera.scale >= 18) {
    ctx.fillStyle = "#ffffff";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = `0.6px ${FONT}`;
    for (const pad of board.pads) {
      const at = shifted(pad.at, delta(pad.instance));
      if (onScreen(at, at, 0)) ctx.fillText(pad.number, toMm(at.x), toMm(at.y));
    }
  }

  // Missing connections, replaced by the proposal's own report while one is shown.
  ctx.strokeStyle = color.airwire;
  ctx.lineWidth = pixel;
  ctx.beginPath();
  for (const wire of overlay.proposal ? [] : board.airwires) {
    ctx.moveTo(toMm(wire.a.x), toMm(wire.a.y));
    ctx.lineTo(toMm(wire.b.x), toMm(wire.b.y));
  }
  ctx.stroke();

  // Findings, drawn directly into the graphic.
  const radius = 9 * pixel;
  for (const violation of overlay.violations) {
    const style = violation.severity === "error" ? color.error : color.warning;
    const x = toMm(violation.at.x);
    const y = toMm(violation.at.y);
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    ctx.globalAlpha = 0.25;
    ctx.fillStyle = style;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.strokeStyle = style;
    ctx.lineWidth = 2 * pixel;
    ctx.stroke();
  }

  // The router's proposal stands out from existing copper.
  if (overlay.proposal) {
    ctx.lineCap = "round";
    for (const track of overlay.proposal.tracks) {
      if (!visible.has(track.layer)) continue;
      ctx.strokeStyle = layerColor(track.layer, board.copper_layers);
      ctx.lineWidth = Math.max(toMm(track.width), 2 * pixel);
      ctx.beginPath();
      ctx.moveTo(toMm(track.a.x), toMm(track.a.y));
      ctx.lineTo(toMm(track.b.x), toMm(track.b.y));
      ctx.stroke();
    }
    for (const via of overlay.proposal.vias) {
      ctx.beginPath();
      ctx.arc(toMm(via.at.x), toMm(via.at.y), toMm(board.rules.via_diameter) / 2, 0, Math.PI * 2);
      ctx.fillStyle = color.through;
      ctx.fill();
      ctx.beginPath();
      ctx.arc(toMm(via.at.x), toMm(via.at.y), toMm(board.rules.via_drill) / 2, 0, Math.PI * 2);
      ctx.fillStyle = color.hole;
      ctx.fill();
    }
    ctx.strokeStyle = color.error;
    ctx.lineWidth = 1.5 * pixel;
    ctx.beginPath();
    for (const wire of overlay.proposal.remaining) {
      ctx.moveTo(toMm(wire.a.x), toMm(wire.a.y));
      ctx.lineTo(toMm(wire.b.x), toMm(wire.b.y));
    }
    ctx.stroke();
  }

  // Tool previews.
  if (overlay.routing && overlay.routing.points.length >= 2) {
    ctx.strokeStyle = layerColor(overlay.routing.layer, board.copper_layers);
    ctx.lineWidth = toMm(overlay.routing.width);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    path(ctx, overlay.routing.points, false);
    ctx.stroke();
  }
  if (overlay.polygon?.length) {
    ctx.strokeStyle = color.preview;
    ctx.lineWidth = 1.5 * pixel;
    ctx.setLineDash([6 * pixel, 4 * pixel]);
    ctx.beginPath();
    path(ctx, overlay.polygon, false);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  if (overlay.box) {
    const { min, max } = overlay.box;
    ctx.fillStyle = "rgb(53 198 168 / 12%)";
    ctx.strokeStyle = "#35c6a8";
    ctx.lineWidth = pixel;
    ctx.fillRect(toMm(min.x), toMm(min.y), toMm(max.x - min.x), toMm(max.y - min.y));
    ctx.strokeRect(toMm(min.x), toMm(min.y), toMm(max.x - min.x), toMm(max.y - min.y));
  }
  if (overlay.cursor) {
    const size = 8 * pixel;
    const x = overlay.cursor.x / MM;
    const y = overlay.cursor.y / MM;
    ctx.strokeStyle = color.preview;
    ctx.lineWidth = pixel;
    ctx.beginPath();
    ctx.moveTo(x - size, y);
    ctx.lineTo(x + size, y);
    ctx.moveTo(x, y - size);
    ctx.lineTo(x, y + size);
    ctx.stroke();
  }
}
