// Mirrors the Rust types of crates/tronic-storage and src-tauri/src/cloud.rs.
import { CommandError, ProjectView } from "../api/backend";

export type TargetKind =
  | { kind: "folder"; path: string }
  | { kind: "web_dav"; url: string; user: string }
  | { kind: "s3"; endpoint: string; bucket: string; region: string; access_key: string }
  | { kind: "cloud"; url: string; email: string };

export type Target = { id: string; name: string } & TargetKind;

export type Mode = "sync" | "backup";

export interface LinkView {
  target: string;
  mode: Mode;
  auto: boolean;
  versions: number;
  stored_bytes: number;
  base: string | null;
}

export type Outcome =
  | { kind: "up_to_date"; commit: string }
  | { kind: "pushed"; commit: string }
  | { kind: "pulled"; commit: string }
  | { kind: "conflict"; heads: string[]; local_changed: boolean };

export interface SyncResult {
  target: string;
  outcome: Outcome | null;
  merged: { from_here: string[]; from_there: string[]; deviations: number } | null;
  error: CommandError | null;
}

export interface SyncReport {
  results: SyncResult[];
  project: ProjectView | null;
  links: LinkView[];
}

export interface ResolveReport {
  project: ProjectView | null;
  copy: string | null;
  links: LinkView[];
}

export interface Version {
  id: string;
  commit: { parents: string[]; blob: string; size: number; device: string; created: string };
  is_head: boolean;
}

export interface Comparison {
  parts_added: string[];
  parts_removed: string[];
  values_changed: { reference: string; before: string; after: string }[];
  nets_added: string[];
  nets_removed: string[];
  nets_changed: string[];
  footprints: [number, number];
  tracks: [number, number];
  vias: [number, number];
  board_changed: boolean;
  identical: boolean;
}

export type Choice = "keep_local" | "take_remote" | "keep_both";
