// Mirrors the Rust types of crates/tronic-3d as sent over IPC.
import { Mechanics } from "../pcb/types";

export interface Mesh {
  positions: number[];
  normals: number[];
}

export interface ScenePart {
  id: number;
  reference: string;
  footprint: string;
  model_name: string | null;
  has_model: boolean;
  meshes: { color: [number, number, number]; mesh: Mesh }[];
}

export interface Finding {
  code: "collision" | "too_close" | "too_high" | "no_model";
  elements: number[];
  params: Record<string, string>;
}

export interface PartHeight {
  id: number;
  reference: string;
  bottom: boolean;
  height: number;
  has_model: boolean;
}

export interface MechanicsReport {
  findings: Finding[];
  heights: PartHeight[];
  max_top: number;
  max_bottom: number;
}

export interface Scene {
  thickness: number;
  rules: Mechanics;
  outline: [number, number][];
  board: Mesh;
  parts: ScenePart[];
  mechanics: MechanicsReport;
}
