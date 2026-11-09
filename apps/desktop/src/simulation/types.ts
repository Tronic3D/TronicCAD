// Mirrors the Rust types of crates/tronic-sim as sent over IPC.

export type AnalysisRequest =
  | { type: "operating_point" }
  | { type: "dc_sweep"; source: string; start: string; stop: string; step: string }
  | { type: "ac"; start: string; stop: string; points_per_decade: number }
  | { type: "transient"; stop: string; step: string };

export interface BuildReport {
  models: { reference: string; kind: string; detail: string }[];
  unsupported: string[];
  ignored: string[];
  bad_values: [string, string][];
  unknown_supplies: string[];
  supplies: [string, number][];
  has_ground: boolean;
}

export interface SimPreparation {
  report: BuildReport;
  complete: boolean;
  deck: string;
  sources: string[];
}

export interface Trace {
  name: string;
  unit: string;
  values: number[];
}

export interface SimResult {
  x_unit: "point" | "V" | "Hz" | "s";
  x: number[];
  traces: Trace[];
}

export interface SimRun {
  preparation: SimPreparation;
  result: SimResult;
}
