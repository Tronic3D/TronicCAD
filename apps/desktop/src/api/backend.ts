// The only module that talks to the Rust backend and the native window.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { DrcViolation, PcbCommand, PcbView, RouteResult, SyncReport } from "../pcb/types";
import { AiAnalysis, AiAsk, AiPrepared, AiReply, AiSettings, AiStatus, Consent, ProviderConfig } from "../ai/types";
import { Choice, Comparison, LinkView, Mode, ResolveReport, SyncReport as CloudSyncReport, Target, TargetKind, Version } from "../cloud/types";
import type { UpdateStatus } from "../update/UpdatePanel";
import type { SupportInfo } from "../help/HelpPanel";
import type { Component } from "../help/ThirdPartyPanel";
import type { ExtensionsOverview, RunResult } from "../extensions/ExtensionsPanel";
import { AnalysisRequest, SimPreparation, SimRun } from "../simulation/types";
import { Scene } from "../view3d/types";
import { ApplyResult, CustomPartSpec, EditCommand, NetHighlight, SchematicView, SymbolDef, Violation } from "../schematic/types";

export interface ProjectView {
  id: string;
  path: string;
  name: string;
  description: string;
  createdAt: string;
  modifiedAt: string;
  dirty: boolean;
  formatVersion: number;
  recovery: RecoveryEntry | null;
}

export interface RecoveryEntry {
  path: string;
  name: string;
  savedAt: string;
  diskState: "unchanged" | "changed" | "missing";
}

export interface AutosaveEvent {
  savedAt: string | null;
  error: CommandError | null;
}

export interface RecentEntry {
  path: string;
  name: string;
  openedAt: string;
}

export interface AppInfo {
  version: string;
  beta?: string | null;
  settingsPersistent: boolean;
  autosaveAvailable: boolean;
}

export interface CommandError {
  code: string;
  params: Record<string, string>;
}

export function isCommandError(value: unknown): value is CommandError {
  return typeof value === "object" && value !== null && "code" in value && "params" in value;
}

// Backend commands run on a thread pool. Project commands go through this
// queue so that an edit followed by a save reaches the backend in that order.
let queue: Promise<unknown> = Promise.resolve();

function ordered<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const run = () => invoke<T>(command, args);
  const result = queue.then(run, run);
  queue = result.catch(() => undefined);
  return result;
}

export const backend = {
  appInfo: () => invoke<AppInfo>("app_info"),
  createProject: (path: string, name: string, description: string) =>
    ordered<ProjectView>("project_create", { path, name, description }),
  openProject: (path: string) => ordered<ProjectView>("project_open", { path }),
  updateProject: (name: string, description: string) =>
    ordered<ProjectView>("project_update", { name, description }),
  saveProject: (overwrite: boolean) => ordered<ProjectView>("project_save", { overwrite }),
  saveProjectAs: (path: string) => ordered<ProjectView>("project_save_as", { path }),
  closeProject: () => ordered<void>("project_close"),
  recoveryList: () => invoke<RecoveryEntry[]>("recovery_list"),
  recoverProject: (path: string) => ordered<ProjectView>("project_recover", { path }),
  discardRecovery: (path: string) => ordered<void>("recovery_discard", { path }),
  onAutosave: (handler: (event: AutosaveEvent) => void) =>
    listen<AutosaveEvent>("autosave", (event) => handler(event.payload)),
  recentProjects: () => invoke<RecentEntry[]>("recent_list"),
  removeRecent: (path: string) => invoke<void>("recent_remove", { path }),
  getLanguage: () => invoke<string | null>("language_get"),
  setLanguage: (language: string) => invoke<void>("language_set", { language }),

  async pickSavePath(filterName: string, defaultPath?: string): Promise<string | null> {
    return save({ defaultPath, filters: [{ name: filterName, extensions: ["tronic"] }] });
  },
  async pickOpenPath(filterName: string): Promise<string | null> {
    return open({ multiple: false, filters: [{ name: filterName, extensions: ["tronic"] }] });
  },

  async pickSaveFile(filterName: string, extension: string, defaultPath?: string): Promise<string | null> {
    return save({ defaultPath, filters: [{ name: filterName, extensions: [extension] }] });
  },
  async pickOpenFile(filterName: string, extension: string): Promise<string | null> {
    return open({ multiple: false, filters: [{ name: filterName, extensions: [extension] }] });
  },

  async pickModelFile(filterName: string): Promise<string | null> {
    return open({ multiple: false, filters: [{ name: filterName, extensions: ["stl", "obj", "wrl", "vrml", "step", "stp"] }] });
  },

  async pickDirectory(): Promise<string | null> {
    return open({ multiple: false, directory: true });
  },

  pcb: {
    view: () => ordered<PcbView>("pcb_view"),
    apply: (command: PcbCommand) =>
      ordered<{ view: PcbView; created: number[] }>("pcb_apply", { command }),
    undo: () => ordered<PcbView>("pcb_undo"),
    redo: () => ordered<PcbView>("pcb_redo"),
    sync: () => ordered<{ view: PcbView; report: SyncReport }>("pcb_sync"),
    drc: () => ordered<DrcViolation[]>("pcb_drc"),
    export: (directory: string) => ordered<string[]>("pcb_export", { directory }),
    // Not queued: cancelling must get through while the router runs.
    routeStart: () => invoke<RouteResult>("pcb_route_start"),
    routeCancel: () => invoke<void>("pcb_route_cancel"),
    routeAccept: () => ordered<{ view: PcbView; created: number[] }>("pcb_route_accept"),
    routeDiscard: () => ordered<void>("pcb_route_discard"),
    onRouteProgress: (handler: (done: number, total: number) => void) =>
      listen<{ done: number; total: number }>("router-progress", (event) =>
        handler(event.payload.done, event.payload.total),
      ),
  },

  // Not queued: a long simulation must not hold up editing commands.
  sim: {
    prepare: (supplies: Record<string, string>) => invoke<SimPreparation>("sim_prepare", { supplies }),
    run: (supplies: Record<string, string>, analysis: AnalysisRequest) =>
      invoke<SimRun>("sim_run", { supplies, analysis }),
    saveText: (path: string, text: string) => invoke<void>("sim_save_text", { path, text }),
  },

  // Not queued: a long answer must not hold up editing commands, and stopping
  // must get through while it runs.
  ai: {
    settings: () => invoke<AiSettings>("ai_settings"),
    setProvider: (config: ProviderConfig | null) => invoke<AiSettings>("ai_set_provider", { config }),
    setKey: (key: string) => invoke<AiSettings>("ai_set_key", { key }),
    deleteKey: () => invoke<AiSettings>("ai_delete_key"),
    status: () => invoke<AiStatus>("ai_status"),
    analysis: () => ordered<AiAnalysis>("ai_analysis"),
    prepare: (ask: AiAsk) => ordered<AiPrepared>("ai_prepare", { ask }),
    send: (id: number, consent: Consent) => invoke<AiReply>("ai_send", { id, consent }),
    cancel: () => invoke<void>("ai_cancel"),
    apply: (indices: number[]) => ordered<{ dirty: boolean; skipped: { code: string; detail: string }[] }>("ai_apply", { indices }),
    discard: () => invoke<void>("ai_discard"),
    onText: (handler: (id: number, text: string) => void) =>
      listen<{ id: number; text: string }>("ai-text", (event) => handler(event.payload.id, event.payload.text)),
  },

  // Not queued: transfers can take long, and local work must not wait for them.
  supportInfo: () => invoke<SupportInfo>("support_info"),
  thirdParty: {
    components: () => invoke<Component[]>("third_party_components"),
    notices: () => invoke<string>("third_party_notices"),
    ownLicense: () => invoke<string>("own_license"),
  },
  extensions: {
    list: () => invoke<ExtensionsOverview>("extensions_list"),
    trust: (id: string, shownFingerprint: string) => invoke<void>("extensions_trust", { id, shownFingerprint }),
    untrust: (id: string) => invoke<void>("extensions_untrust", { id }),
    run: (id: string) => invoke<RunResult>("extensions_run", { id }),
    openFolder: () => invoke<void>("extensions_open_folder"),
  },
  update: {
    status: () => invoke<UpdateStatus>("update_status"),
    onChanged: (handler: () => void) => listen("update-changed", () => handler()),
    setChannel: (channel: UpdateStatus["channel"]) => invoke<void>("update_set_channel", { channel }),
    setAutomatic: (automatic: boolean) => invoke<void>("update_set_automatic", { automatic }),
    check: () => invoke<void>("update_check"),
    download: () => invoke<void>("update_download"),
    install: () => invoke<void>("update_install"),
    confirm: () => invoke<void>("update_confirm"),
    rollback: () => invoke<void>("update_rollback"),
    openBackup: (path: string) => invoke<void>("update_open_backup", { path }),
  },
  cloud: {
    overview: () => invoke<{ targets: Target[]; device: string }>("cloud_overview"),
    setDevice: (device: string) => invoke<void>("cloud_set_device", { device }),
    addTarget: (name: string, kind: TargetKind, secret: string) =>
      invoke<Target[]>("cloud_add_target", { name, kind, secret }),
    account: (name: string, url: string, email: string, password: string, create: boolean, invitation: string | null) =>
      invoke<Target[]>("cloud_account", { name, url, email, password, create, invitation }),
    removeTarget: (target: string) => invoke<Target[]>("cloud_remove_target", { target }),
    links: () => invoke<LinkView[]>("cloud_links"),
    link: (target: string, request: { mode: Mode; auto: boolean; confirmed: boolean }) =>
      invoke<LinkView[]>("cloud_link", { target, request }),
    unlink: (target: string) => invoke<LinkView[]>("cloud_unlink", { target }),
    sync: (onlyAuto: boolean) => invoke<CloudSyncReport>("cloud_sync", { onlyAuto }),
    resolve: (target: string, head: string, choice: Choice) =>
      invoke<ResolveReport>("cloud_resolve", { target, head, choice }),
    versions: (target: string) => invoke<Version[]>("cloud_versions", { target }),
    compare: (target: string, commit: string) => invoke<Comparison>("cloud_compare", { target, commit }),
    restore: (target: string, commit: string) => invoke<ProjectView>("cloud_restore", { target, commit }),
  },

  team: {
    call: <T>(target: string, method: string, path: string, body: unknown) =>
      invoke<T>("team_call", { target, method, path, body }),
    clone: (target: string, repo: string, path: string) =>
      invoke<ProjectView>("team_clone", { target, repo, path }),
  },

  view3d: {
    scene: () => ordered<Scene>("view3d_scene"),
    importModel: (footprint: string, path: string, scale: number, zOffset: number) =>
      ordered<Scene>("view3d_import_model", { footprint, path, scale, zOffset }),
  },

  schematic: {
    footprints: () => invoke<{ id: string; name: string }[]>("library_footprints"),
    view: (sheet: number) => ordered<SchematicView>("schematic_view", { sheet }),
    apply: (viewSheet: number, command: EditCommand) =>
      ordered<ApplyResult>("schematic_apply", { viewSheet, command }),
    undo: (sheet: number) => ordered<SchematicView>("schematic_undo", { sheet }),
    redo: (sheet: number) => ordered<SchematicView>("schematic_redo", { sheet }),
    erc: () => ordered<Violation[]>("schematic_erc"),
    netlist: () => ordered<{ text: string }>("schematic_netlist"),
    exportNetlist: (path: string) => ordered<void>("schematic_export_netlist", { path }),
    netOf: (sheet: number, element: number) =>
      ordered<NetHighlight | null>("schematic_net_of", { sheet, element }),
    library: () => invoke<SymbolDef[]>("library_list"),
    createPart: (sheet: number, spec: CustomPartSpec) =>
      ordered<SchematicView>("library_create_part", { sheet, spec }),
    importLibrary: (sheet: number, path: string) =>
      ordered<SchematicView>("library_import", { sheet, path }),
    exportLibrary: (path: string) => ordered<number>("library_export", { path }),
  },

  setWindowTitle: (title: string) => getCurrentWindow().setTitle(title),
  destroyWindow: () => getCurrentWindow().destroy(),
  /** Calls `shouldClose` on a close request; returning false keeps the window open. */
  onCloseRequested: (shouldClose: () => boolean) =>
    getCurrentWindow().onCloseRequested((event) => {
      if (!shouldClose()) event.preventDefault();
    }),
};
