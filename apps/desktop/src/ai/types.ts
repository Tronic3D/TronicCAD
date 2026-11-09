// Mirrors the Rust types of crates/tronic-ai as sent over IPC.

export type ProviderKind = "open_ai_compatible" | "anthropic";
export type Locality = "local" | "external";

export interface ProviderConfig {
  kind: ProviderKind;
  base_url: string;
  model: string;
  max_tokens: number | null;
}

export interface AiSettings {
  config: ProviderConfig | null;
  locality: Locality | null;
  has_key: boolean;
  key_store_available: boolean;
}

export interface AiStatus {
  reachable: boolean | null;
  error: { code: string; params: Record<string, string> } | null;
  models: string[];
  loaded: { name: string; bytes: number; gpu_bytes: number }[];
  memory_total: number;
  memory_available: number;
}

export interface Finding {
  code: string;
  level: "warning" | "info";
  params: Record<string, string>;
}

export interface CheckCounts {
  errors: number;
  warnings: number;
}

export interface AiAnalysis {
  findings: Finding[];
  checks: { erc: CheckCounts; drc: CheckCounts };
}

export type Task =
  | "free"
  | "create_circuit"
  | "analyze_schematic"
  | "recommend_parts"
  | "evaluate_datasheet"
  | "explain_pins"
  | "explain_errors"
  | "improve"
  | "analyze_board"
  | "manufacturing"
  | "documentation";

export interface Sections {
  schematic: boolean;
  erc: boolean;
  board: boolean;
  drc: boolean;
  library: boolean;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface AiAsk {
  task: Task;
  text: string;
  document: string;
  sections: Sections;
  language: string;
  history: ChatMessage[];
}

export interface AiPrepared {
  id: number;
  request: { system: string; messages: ChatMessage[] };
  locality: Locality;
  origin: string;
  model: string;
  needs_consent: boolean;
}

export type Change =
  | { type: "set_value"; reference: string; value: string }
  | { type: "set_reference"; reference: string; new_reference: string }
  | { type: "place"; symbol: string; reference: string; value: string | null }
  | { type: "connect"; reference: string; pin: string; net: string }
  | { type: "delete"; reference: string };

export interface Preview {
  changes: Change[];
  rejected: { code: string; detail: string }[];
  erc_before: CheckCounts;
  erc_after: CheckCounts;
}

export interface AiReply {
  id: number;
  text: string;
  stop: "finished" | "truncated" | "refused" | "cancelled";
  input_tokens: number | null;
  output_tokens: number | null;
  seconds: number;
  possibly_cut: boolean;
  context_length: number | null;
  proposal: Preview | null;
}

export type Consent = "none" | "once" | "project";
