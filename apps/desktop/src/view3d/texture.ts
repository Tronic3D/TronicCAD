// Draws what is visible on one face of the board into a canvas, used as a texture.
import { BoardView, PadView, Point } from "../pcb/types";
import { MM } from "../pcb/geometry";

const MASK = "#0d5a3a";
const COPPER_UNDER_MASK = "#1b7a52";
const PAD = "#d4b45a";
const SILK = "#f0f0e8";
const HOLE = "#15181a";

const MAX_SIZE = 4096;
const PIXELS_PER_MM = 24;

export interface FaceRect {
  minX: number;
  minY: number;
  width: number;
  height: number;
}

/** The outline's bounding box in millimetres, Y as on the board (pointing down). */
export function faceRect(board: BoardView): FaceRect | null {
  if (board.outline.length < 3) return null;
  const xs = board.outline.map((p) => p.x / MM);
  const ys = board.outline.map((p) => p.y / MM);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  return { minX, minY, width: Math.max(...xs) - minX, height: Math.max(...ys) - minY };
}

export function textureScale(rect: FaceRect): number {
  return Math.min(PIXELS_PER_MM, MAX_SIZE / Math.max(rect.width, rect.height, 1));
}

function polygon(ctx: CanvasRenderingContext2D, points: Point[]) {
  points.forEach((p, i) => {
    if (i === 0) ctx.moveTo(p.x / MM, p.y / MM);
    else ctx.lineTo(p.x / MM, p.y / MM);
  });
  ctx.closePath();
}

function padShape(ctx: CanvasRenderingContext2D, pad: PadView) {
  const x = pad.at.x / MM;
  const y = pad.at.y / MM;
  const w = pad.size.x / MM;
  const h = pad.size.y / MM;
  ctx.beginPath();
  if (pad.shape === "rect") ctx.rect(x - w / 2, y - h / 2, w, h);
  else if (pad.shape === "round") ctx.arc(x, y, w / 2, 0, Math.PI * 2);
  else ctx.roundRect(x - w / 2, y - h / 2, w, h, Math.min(w, h) / 2);
}

/** Paints one side; `bottom` selects the last copper layer and the mirrored silk. */
export function paintFace(board: BoardView, rect: FaceRect, bottom: boolean): HTMLCanvasElement {
  const scale = textureScale(rect);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(Math.ceil(rect.width * scale), 1);
  canvas.height = Math.max(Math.ceil(rect.height * scale), 1);
  const ctx = canvas.getContext("2d");
  if (!ctx) return canvas;
  ctx.fillStyle = MASK;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.setTransform(scale, 0, 0, scale, -rect.minX * scale, -rect.minY * scale);
  const layer = bottom ? board.copper_layers - 1 : 0;

  ctx.fillStyle = COPPER_UNDER_MASK;
  for (const fill of board.fills) {
    if (fill.layer !== layer) continue;
    for (const shape of fill.shapes) {
      ctx.beginPath();
      for (const ring of shape) polygon(ctx, ring);
      ctx.fill("evenodd");
    }
  }
  ctx.strokeStyle = COPPER_UNDER_MASK;
  ctx.lineCap = "round";
  for (const track of board.tracks) {
    if (track.layer !== layer) continue;
    ctx.lineWidth = track.width / MM;
    ctx.beginPath();
    ctx.moveTo(track.a.x / MM, track.a.y / MM);
    ctx.lineTo(track.b.x / MM, track.b.y / MM);
    ctx.stroke();
  }

  ctx.fillStyle = PAD;
  for (const via of board.vias) {
    ctx.beginPath();
    ctx.arc(via.at.x / MM, via.at.y / MM, via.diameter / MM / 2, 0, Math.PI * 2);
    ctx.fill();
  }
  for (const pad of board.pads) {
    // A pad without a layer goes through the whole board.
    if (!pad.plated) continue;
    if (pad.layer === null || pad.layer === layer) {
      padShape(ctx, pad);
      ctx.fill();
    }
  }

  ctx.strokeStyle = SILK;
  ctx.lineWidth = 0.15;
  ctx.lineCap = "round";
  for (const footprint of board.footprints) {
    if (footprint.bottom !== bottom) continue;
    ctx.beginPath();
    for (const [a, b] of footprint.silk) {
      ctx.moveTo(a.x / MM, a.y / MM);
      ctx.lineTo(b.x / MM, b.y / MM);
    }
    ctx.stroke();
  }

  ctx.fillStyle = HOLE;
  for (const via of board.vias) {
    ctx.beginPath();
    ctx.arc(via.at.x / MM, via.at.y / MM, via.drill / MM / 2, 0, Math.PI * 2);
    ctx.fill();
  }
  for (const pad of board.pads) {
    if (pad.drill === null) continue;
    ctx.beginPath();
    ctx.arc(pad.at.x / MM, pad.at.y / MM, pad.drill / MM / 2, 0, Math.PI * 2);
    ctx.fill();
  }
  return canvas;
}
