import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AppInfo, AutosaveEvent, backend, isCommandError, ProjectView, RecentEntry, RecoveryEntry } from "./api/backend";
import { AiPanel } from "./ai/AiPanel";
import { CloudPanel } from "./cloud/CloudPanel";
import { Module, ModuleRail } from "./components/ModuleRail";
import { ConflictDialog, ErrorDialog, NewProjectDialog, RecoveryDialog, UnsavedDialog } from "./components/dialogs";
import { errorMessage, formatDate } from "./components/format";
import { ProjectPanel } from "./components/ProjectPanel";
import { TeamPanel } from "./team/TeamPanel";
import { Toolbar } from "./components/Toolbar";
import { WelcomePanel } from "./components/WelcomePanel";
import { matchLanguage } from "./i18n";
import { PcbEditor } from "./pcb/PcbEditor";
import { SchematicEditor } from "./schematic/SchematicEditor";
import { SimulationPanel } from "./simulation/SimulationPanel";
import { View3D } from "./view3d/View3D";
import { UpdatePanel, UpdateStatus } from "./update/UpdatePanel";
import { HelpPanel } from "./help/HelpPanel";
import { ExtensionsPanel } from "./extensions/ExtensionsPanel";

type Dialog =
  | { kind: "new" }
  | { kind: "unsaved"; proceed: () => void | Promise<void> }
  | { kind: "conflict"; path: string }
  | { kind: "recovery"; entries: RecoveryEntry[] }
  | { kind: "error"; error: unknown };

const AUTO_SYNC_MS = 60_000;

type StatusKey = "ready" | "saved" | "created" | "opened" | "recovered";

export default function App() {
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const { t, i18n } = useTranslation();
  const [project, setProject] = useState<ProjectView | null>(null);
  const [recent, setRecent] = useState<RecentEntry[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [status, setStatus] = useState<StatusKey>("ready");
  const [module, setModule] = useState<Module>("project");
  const [autosave, setAutosave] = useState<AutosaveEvent | null>(null);
  const [metadataInvalid, setMetadataInvalid] = useState(false);
  // Identifies one opened document. The project id cannot do this: copies made
  // with "save as" share it, and the same file can be reopened with new content.
  const [documentKey, setDocumentKey] = useState(0);

  const showError = useCallback((error: unknown) => setDialog({ kind: "error", error }), []);

  const refreshRecent = useCallback(async () => {
    try {
      setRecent(await backend.recentProjects());
    } catch (error) {
      showError(error);
    }
  }, [showError]);

  const loadUpdate = useCallback(() => {
    backend.update.status().then(setUpdate).catch(() => setUpdate(null));
  }, []);

  // Tells a program that was just installed, and is waited for, that it works.
  useEffect(() => {
    backend.update.confirm().catch(() => undefined);
    loadUpdate();
    let stop: (() => void) | undefined;
    void backend.update.onChanged(loadUpdate).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop?.();
  }, [loadUpdate]);

  useEffect(() => {
    backend.appInfo().then(setInfo).catch(showError);
    backend
      .getLanguage()
      .then((stored) => {
        if (stored) void i18n.changeLanguage(matchLanguage(stored));
      })
      .catch(showError);
    void refreshRecent();
    backend
      .recoveryList()
      .then((entries) => {
        if (entries.length) setDialog({ kind: "recovery", entries });
      })
      .catch(showError);
  }, [i18n, refreshRecent, showError]);

  useEffect(() => {
    const pending = backend.onAutosave(setAutosave);
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const adopt = useCallback(
    (view: ProjectView, nextStatus: StatusKey) => {
      setProject(view);
      setMetadataInvalid(false);
      setStatus(nextStatus);
      setAutosave(null);
      if (nextStatus !== "saved") setDocumentKey((key) => key + 1);
      void refreshRecent();
    },
    [refreshRecent],
  );

  const projectRef = useRef(project);
  projectRef.current = project;

  /** Runs `action`, first asking what to do with unsaved changes. */
  const guarded = useCallback((action: () => void | Promise<void>) => {
    if (projectRef.current?.dirty) setDialog({ kind: "unsaved", proceed: action });
    else void action();
  }, []);

  const save = useCallback(
    async (overwrite = false): Promise<boolean> => {
      try {
        adopt(await backend.saveProject(overwrite), "saved");
        return true;
      } catch (error) {
        if (isCommandError(error) && error.code === "project.conflict") {
          setDialog({ kind: "conflict", path: error.params.path ?? "" });
        } else {
          showError(error);
        }
        return false;
      }
    },
    [adopt, showError],
  );

  const saveAs = useCallback(async () => {
    try {
      const path = await backend.pickSavePath(t("fileFilter.project"), projectRef.current?.path);
      if (path) adopt(await backend.saveProjectAs(path), "saved");
    } catch (error) {
      showError(error);
    }
  }, [adopt, showError, t]);

  const openPath = useCallback(
    async (path: string) => {
      try {
        const opened = await backend.openProject(path);
        adopt(opened, "opened");
        if (opened.recovery) setDialog({ kind: "recovery", entries: [opened.recovery] });
      } catch (error) {
        showError(error);
      }
    },
    [adopt, showError],
  );

  const openProject = useCallback(() => {
    guarded(async () => {
      try {
        const path = await backend.pickOpenPath(t("fileFilter.project"));
        if (path) await openPath(path);
      } catch (error) {
        showError(error);
      }
    });
  }, [guarded, openPath, showError, t]);

  const newProject = useCallback(() => guarded(() => setDialog({ kind: "new" })), [guarded]);

  const closeProject = useCallback(() => {
    guarded(async () => {
      try {
        await backend.closeProject();
        setProject(null);
        setAutosave(null);
        setModule("project");
        setMetadataInvalid(false);
        setStatus("ready");
      } catch (error) {
        showError(error);
      }
    });
  }, [guarded, showError]);

  const onSchematicDirty = useCallback((dirty: boolean) => {
    setProject((current) => (current && current.dirty !== dirty ? { ...current, dirty } : current));
  }, []);

  const changeLanguage = useCallback(
    (language: string) => {
      void i18n.changeLanguage(language);
      backend.setLanguage(language).catch(showError);
    },
    [i18n, showError],
  );

  // Automatic sync of links that ask for it: after saving and now and then. Only
  // saved states travel, and a project with unsaved changes is left alone.
  const [cloudStatus, setCloudStatus] = useState<"offline" | "conflict" | null>(null);
  const modifiedAt = project?.modifiedAt;
  const hasProject = project !== null;
  useEffect(() => {
    if (!hasProject) {
      setCloudStatus(null);
      return;
    }
    let active = true;
    const run = () => {
      if (projectRef.current?.dirty) return;
      backend.cloud
        .sync(true)
        .then((report) => {
          if (!active) return;
          if (report.project) adopt(report.project, "opened");
          const conflict = report.results.some((result) => result.outcome?.kind === "conflict");
          const offline = report.results.some((result) => result.error?.code === "storage.unreachable");
          setCloudStatus(conflict ? "conflict" : offline ? "offline" : null);
        })
        .catch(() => undefined);
    };
    run();
    const timer = window.setInterval(run, AUTO_SYNC_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [hasProject, modifiedAt, adopt]);

  // Keyboard shortcuts.
  const canSave = project !== null && !metadataInvalid;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || dialog) return;
      const key = event.key.toLowerCase();
      if (key === "n") newProject();
      else if (key === "o") openProject();
      else if (key === "s" && canSave) {
        if (event.shiftKey) void saveAs();
        else void save();
      } else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [canSave, dialog, newProject, openProject, save, saveAs]);

  // Window title.
  useEffect(() => {
    const title = project ? `${project.dirty ? "• " : ""}${project.name} – TronicCAD` : "TronicCAD";
    backend.setWindowTitle(title).catch(() => undefined);
  }, [project]);

  // Unsaved work must survive closing the window.
  useEffect(() => {
    const pending = backend.onCloseRequested(() => {
      if (!projectRef.current?.dirty) return true;
      setDialog({
        kind: "unsaved",
        proceed: async () => {
          // Closing the project removes its snapshot: the user chose to leave.
          try {
            await backend.closeProject();
          } finally {
            await backend.destroyWindow();
          }
        },
      });
      return false;
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const statusText = !project
    ? t("status.noProject")
    : project.dirty
      ? t("status.unsaved")
      : t(`status.${status}`);

  return (
    <div className="app">
      <Toolbar
        hasProject={project !== null}
        canSave={canSave}
        onNew={newProject}
        onOpen={openProject}
        onSave={() => void save()}
        onSaveAs={() => void saveAs()}
        onClose={closeProject}
        language={i18n.language}
        onLanguage={changeLanguage}
      />
      <div className="workspace">
        <ModuleRail active={module} hasProject={project !== null} onSelect={setModule} />
        <main className={(project && module !== "project") || module === "update" || module === "extensions" || module === "help" ? "content content-flush" : "content"}>
          {module === "extensions" ? (
            <ExtensionsPanel hasProject={project !== null} />
          ) : module === "help" ? (
            <HelpPanel key={i18n.language} />
          ) : module === "update" ? (
            <UpdatePanel onChanged={loadUpdate} />
          ) : project && module === "team" ? (
            <TeamPanel
              key={documentKey}
              project={project}
              onProject={(view) => adopt(view, "opened")}
              guarded={guarded}
            />
          ) : project && module === "cloud" ? (
            <CloudPanel
              project={project}
              onProject={(view) => adopt(view, "opened")}
              onError={showError}
            />
          ) : project && module === "ai" ? (
            <AiPanel key={documentKey} onDirty={onSchematicDirty} onError={showError} />
          ) : project && module === "view3d" ? (
            <View3D key={documentKey} onDirty={onSchematicDirty} onError={showError} />
          ) : project && module === "simulation" ? (
            <SimulationPanel key={documentKey} onError={showError} />
          ) : project && module === "pcb" ? (
            <PcbEditor key={documentKey} active={dialog === null} onDirty={onSchematicDirty} onError={showError} />
          ) : project && module === "schematic" ? (
            <SchematicEditor
              key={documentKey}
              active={dialog === null}
              onDirty={onSchematicDirty}
              onError={showError}
            />
          ) : project ? (
            <ProjectPanel
              key={documentKey}
              project={project}
              onChanged={setProject}
              onValidity={(valid) => setMetadataInvalid(!valid)}
              onOpenSchematic={() => setModule("schematic")}
            />
          ) : (
            <WelcomePanel
              recent={recent}
              onNew={newProject}
              onOpen={openProject}
              onOpenRecent={(path) => guarded(() => openPath(path))}
              onRemoveRecent={(path) => {
                backend.removeRecent(path).then(refreshRecent).catch(showError);
              }}
            />
          )}
        </main>
      </div>
      <footer className="statusbar">
        <span role="status" className={project?.dirty ? "status-dirty" : undefined}>
          {statusText}
        </span>
        <span className="statusbar-path" title={project?.path}>
          {project?.path}
        </span>
        {project && autosave?.savedAt && (
          <span>{t("status.autosaved", { time: formatDate(autosave.savedAt, i18n.language) })}</span>
        )}
        {project && autosave?.error && (
          <span className="status-warning">
            {t("status.autosaveFailed", { detail: errorMessage(t, autosave.error) })}
          </span>
        )}
        {update?.available && (
          <button
            type="button"
            className={`status-link${update.available.kind === "security" ? " status-warning" : ""}`}
            onClick={() => setModule("update")}
          >
            {t(update.available.kind === "security" ? "update.statusSecurity" : "update.statusFeature", {
              version: update.available.version,
            })}
          </button>
        )}
        {cloudStatus && <span className="status-warning">{t(`cloud.status.${cloudStatus}`)}</span>}
        {info && !info.autosaveAvailable && (
          <span className="status-warning">{t("app.autosaveUnavailable")}</span>
        )}
        {info && !info.settingsPersistent && (
          <span className="status-warning">{t("app.settingsVolatile")}</span>
        )}
        {info && <span>{info.beta ? t("app.betaBuild", { label: info.beta, version: info.version }) : t("app.devBuild", { version: info.version })}</span>}
      </footer>

      {dialog?.kind === "new" && (
        <NewProjectDialog
          onCancel={() => setDialog(null)}
          onCreate={async (path, name, description) => {
            adopt(await backend.createProject(path, name, description), "created");
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === "unsaved" && project && (
        <UnsavedDialog
          name={project.name}
          canSave={canSave}
          onCancel={() => setDialog(null)}
          onDiscard={() => {
            const { proceed } = dialog;
            setDialog(null);
            void proceed();
          }}
          onSave={async () => {
            const { proceed } = dialog;
            setDialog(null);
            // After a failed save a conflict or error dialog is showing and
            // the pending action is dropped, so no work can get lost.
            if (await save()) await proceed();
          }}
        />
      )}
      {dialog?.kind === "conflict" && (
        <ConflictDialog
          path={dialog.path}
          onCancel={() => setDialog(null)}
          onSaveAs={() => {
            setDialog(null);
            void saveAs();
          }}
          onOverwrite={() => {
            setDialog(null);
            void save(true);
          }}
        />
      )}
      {dialog?.kind === "recovery" && (
        <RecoveryDialog
          entries={dialog.entries}
          onLater={() => setDialog(null)}
          onRestore={async (entry) => {
            setDialog(null);
            try {
              adopt(await backend.recoverProject(entry.path), "recovered");
            } catch (error) {
              showError(error);
            }
          }}
          onDiscard={async (entry) => {
            try {
              await backend.discardRecovery(entry.path);
              const entries = dialog.entries.filter((item) => item.path !== entry.path);
              setDialog(entries.length ? { kind: "recovery", entries } : null);
            } catch (error) {
              showError(error);
            }
          }}
        />
      )}
      {dialog?.kind === "error" && (
        <ErrorDialog error={dialog.error} onClose={() => setDialog(null)} />
      )}
    </div>
  );
}
