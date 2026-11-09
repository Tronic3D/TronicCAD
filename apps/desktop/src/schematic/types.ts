// Mirrors the Rust types of crates/tronic-schematic as sent over IPC.

export interface Point {
  x: number;
  y: number;
}

export type Rotation = 0 | 90 | 180 | 270;

export type PinKind =
  | "input"
  | "output"
  | "bidirectional"
  | "tri_state"
  | "passive"
  | "power_in"
  | "power_out"
  | "open_collector"
  | "open_emitter"
  | "not_connected"
  | "supply_port";

export type PinDirection = "left" | "right" | "up" | "down";

export interface PinDef {
  number: string;
  name: string;
  kind: PinKind;
  at: Point;
  direction: PinDirection;
  length: number;
  hide_name: boolean;
  hide_number: boolean;
}

export type Graphic =
  | { type: "line"; points: Point[] }
  | { type: "polygon"; points: Point[]; filled: boolean }
  | { type: "rect"; a: Point; b: Point }
  | { type: "circle"; center: Point; radius: number; filled: boolean }
  | { type: "arc"; center: Point; radius: number; start_deg: number; end_deg: number }
  | { type: "text"; at: Point; text: string; size: number };

export interface SymbolDef {
  id: string;
  name: string;
  category: string;
  kind: "part" | "supply_port";
  reference_prefix: string;
  default_value: string;
  description: string;
  graphics: Graphic[];
  pins: PinDef[];
  part: {
    footprint: string | null;
    model_3d: string | null;
    manufacturer: string | null;
    part_number: string | null;
    datasheet: string | null;
    limits: { parameter: string; value: string; unit: string }[];
  };
  provenance: { source: string; author: string; license: string; reviewed: boolean };
}

export interface Component {
  id: number;
  symbol: string;
  reference: string;
  value: string;
  at: Point;
  rotation: Rotation;
  mirror: boolean;
  footprint?: string | null;
}

export interface Segment {
  id: number;
  a: Point;
  b: Point;
}

export type LabelKind = "local" | "global" | "hierarchical";

export interface Label {
  id: number;
  at: Point;
  name: string;
  kind: LabelKind;
  rotation: Rotation;
}

export interface Marker {
  id: number;
  at: Point;
}

export interface SheetInstance {
  id: number;
  sheet: number;
  at: Point;
  size: Point;
  ports: { name: string; at: Point }[];
}

export interface Sheet {
  id: number;
  name: string;
  parent: number | null;
  components: Component[];
  wires: Segment[];
  buses: Segment[];
  labels: Label[];
  no_connects: Marker[];
  junctions: Marker[];
  instances: SheetInstance[];
}

export interface SheetEntry {
  id: number;
  name: string;
  parent: number | null;
  depth: number;
}

export interface SchematicView {
  sheets: SheetEntry[];
  sheet: Sheet;
  junctions: Point[];
  cross_references: { label: number; sheets: string[] }[];
  symbols: Record<string, SymbolDef>;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
}

export interface ApplyResult {
  view: SchematicView;
  created: number[];
}

export interface Violation {
  code: string;
  severity: "error" | "warning";
  sheet: number;
  at: Point;
  elements: number[];
  params: Record<string, string>;
}

export interface NetHighlight {
  name: string;
  elements: number[];
  pins: Point[];
}

export interface Clip {
  components: Component[];
  wires: Segment[];
  buses: Segment[];
  labels: Label[];
  no_connects: Marker[];
  junctions: Marker[];
}

export type EditCommand =
  | { type: "place_component"; sheet: number; symbol: string; at: Point; rotation: Rotation; mirror: boolean }
  | { type: "add_wires"; sheet: number; points: Point[] }
  | { type: "add_buses"; sheet: number; points: Point[] }
  | { type: "add_label"; sheet: number; at: Point; name: string; kind: LabelKind }
  | { type: "add_no_connect"; sheet: number; at: Point }
  | { type: "add_junction"; sheet: number; at: Point }
  | { type: "add_sheet"; name: string }
  | { type: "add_block"; sheet: number; at: Point; name: string }
  | { type: "rename_sheet"; sheet: number; name: string }
  | { type: "delete_sheet"; sheet: number }
  | { type: "move"; sheet: number; ids: number[]; delta: Point }
  | { type: "rotate"; sheet: number; ids: number[] }
  | { type: "mirror"; sheet: number; ids: number[] }
  | { type: "delete"; sheet: number; ids: number[] }
  | { type: "set_component"; sheet: number; id: number; reference: string; value: string }
  | { type: "set_footprint"; sheet: number; id: number; footprint: string | null }
  | { type: "set_label"; sheet: number; id: number; name: string }
  | { type: "paste"; sheet: number; clip: Clip; delta: Point }
  | { type: "annotate" };

export interface CustomPartSpec {
  id: string;
  name: string;
  category: string;
  reference_prefix: string;
  value: string;
  pins: { number: string; name: string; kind: PinKind; side: "left" | "right" }[];
}
