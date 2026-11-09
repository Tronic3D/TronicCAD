// Mirrors the Rust types of crates/tronic-pcb as sent over IPC.
import { Point, Rotation } from "../schematic/types";

export type { Point };

export interface DesignRules {
  min_clearance: number;
  min_track_width: number;
  track_width: number;
  via_drill: number;
  via_diameter: number;
  min_drill: number;
  max_drill: number;
  min_annular_ring: number;
  min_hole_spacing: number;
  edge_clearance: number;
  mask_margin: number;
}

export interface FootprintView {
  id: number;
  reference: string;
  value: string;
  footprint: string;
  at: Point;
  rotation: Rotation;
  bottom: boolean;
  silk: [Point, Point][];
  courtyard: [Point, Point];
}

export interface PadView {
  instance: number;
  reference: string;
  number: string;
  net: string | null;
  at: Point;
  size: Point;
  shape: "rect" | "round" | "oblong";
  drill: number | null;
  plated: boolean;
  layer: number | null;
  bottom: boolean;
}

export interface TrackView {
  id: number;
  net: string;
  layer: number;
  a: Point;
  b: Point;
  width: number;
}

export interface ViaView {
  id: number;
  net: string;
  at: Point;
  drill: number;
  diameter: number;
}

export interface ZoneView {
  id: number;
  net: string;
  layer: number;
  outline: Point[];
}

export interface ZoneFillView {
  zone: number;
  net: string;
  layer: number;
  shapes: Point[][][];
}

export interface BoardView {
  copper_layers: number;
  rules: DesignRules;
  outline: Point[];
  footprints: FootprintView[];
  pads: PadView[];
  tracks: TrackView[];
  vias: ViaView[];
  zones: ZoneView[];
  fills: ZoneFillView[];
  airwires: { net: string; a: Point; b: Point }[];
  net_lengths: Record<string, number>;
  nets: string[];
}

export interface PcbView {
  board: BoardView;
  deviations: { kind: string }[];
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
}

export interface SyncReport {
  added: string[];
  removed: string[];
  updated: string[];
  no_footprint: string[];
  unknown_footprint: [string, string][];
  missing_pads: [string, string][];
}

export interface DrcViolation {
  code: string;
  severity: "error" | "warning";
  at: Point;
  layer: number | null;
  elements: number[];
  params: Record<string, string>;
}

export interface Mechanics {
  thickness: number;
  part_clearance: number;
  max_height_top: number | null;
  max_height_bottom: number | null;
}

export type PcbCommand =
  | { type: "set_mechanics"; mechanics: Mechanics }
  | { type: "set_footprint_model"; footprint: string; model: null }
  | { type: "set_outline"; points: Point[] }
  | { type: "add_tracks"; net: string; layer: number; points: Point[]; width: number }
  | { type: "add_via"; net: string; at: Point }
  | { type: "add_zone"; net: string; layer: number; outline: Point[] }
  | { type: "move"; ids: number[]; delta: Point }
  | { type: "rotate"; ids: number[] }
  | { type: "flip"; ids: number[] }
  | { type: "delete"; ids: number[] }
  | { type: "set_rules"; rules: DesignRules }
  | { type: "set_layer_count"; layers: number };

export interface RouteResult {
  tracks: { net: string; layer: number; a: Point; b: Point; width: number }[];
  vias: { net: string; at: Point }[];
  total: number;
  remaining: { net: string; a: Point; b: Point }[];
  cancelled: boolean;
  timed_out: boolean;
  rip_ups: number;
  elapsed_ms: number;
}
