import { FormEvent, ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend, RecoveryEntry } from "../api/backend";
import { errorMessage, formatDate } from "./format";

export function Modal(props: { title: string; onEscape: () => void; children: ReactNode }) {
  const { onEscape } = props;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onEscape();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onEscape]);
  return (
    <div className="modal-backdrop">
      <div className="modal" role="dialog" aria-modal="true" aria-label={props.title}>
        <h2>{props.title}</h2>
        {props.children}
      </div>
    </div>
  );
}

export function NewProjectDialog(props: {
  onCancel: () => void;
  onCreate: (path: string, name: string, description: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [path, setPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const chooseLocation = async () => {
    try {
      const chosen = await backend.pickSavePath(t("fileFilter.project"), name.trim() || undefined);
      if (chosen) setPath(chosen);
    } catch (failure) {
      setError(errorMessage(t, failure));
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!path) {
      setError(t("errors.validation.locationMissing"));
      return;
    }
    setBusy(true);
    try {
      await props.onCreate(path, name, description);
    } catch (failure) {
      setError(errorMessage(t, failure));
      setBusy(false);
    }
  };

  return (
    <Modal title={t("newProject.title")} onEscape={props.onCancel}>
      <form onSubmit={(event) => void submit(event)}>
        <label className="field">
          <span>{t("project.name")}</span>
          <input autoFocus value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("project.description")}</span>
          <textarea
            rows={3}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
        <div className="field">
          <span>{t("newProject.location")}</span>
          <div className="location-row">
            <span className={path ? "selectable" : "muted"}>{path ?? t("newProject.noLocation")}</span>
            <button type="button" onClick={() => void chooseLocation()}>
              {t("newProject.chooseLocation")}
            </button>
          </div>
        </div>
        {error && (
          <p className="field-error" role="alert">
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={busy}>
            {t("newProject.create")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function UnsavedDialog(props: {
  name: string;
  canSave: boolean;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Modal title={t("unsaved.title")} onEscape={props.onCancel}>
      <p>{t("unsaved.body", { name: props.name })}</p>
      <div className="modal-actions">
        <button type="button" onClick={props.onCancel}>
          {t("common.cancel")}
        </button>
        <button type="button" onClick={props.onDiscard}>
          {t("unsaved.discard")}
        </button>
        <button type="button" className="primary" disabled={!props.canSave} onClick={props.onSave}>
          {t("unsaved.save")}
        </button>
      </div>
    </Modal>
  );
}

export function ConflictDialog(props: {
  path: string;
  onSaveAs: () => void;
  onOverwrite: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Modal title={t("conflict.title")} onEscape={props.onCancel}>
      <p>{t("conflict.body", { path: props.path })}</p>
      <div className="modal-actions">
        <button type="button" onClick={props.onCancel}>
          {t("common.cancel")}
        </button>
        <button type="button" className="danger" onClick={props.onOverwrite}>
          {t("conflict.overwrite")}
        </button>
        <button type="button" className="primary" onClick={props.onSaveAs}>
          {t("conflict.saveAs")}
        </button>
      </div>
    </Modal>
  );
}

export function ErrorDialog(props: { error: unknown; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Modal title={t("errors.title")} onEscape={props.onClose}>
      <p role="alert">{errorMessage(t, props.error)}</p>
      <div className="modal-actions">
        <button type="button" className="primary" autoFocus onClick={props.onClose}>
          {t("common.close")}
        </button>
      </div>
    </Modal>
  );
}

export function RecoveryDialog(props: {
  entries: RecoveryEntry[];
  onRestore: (entry: RecoveryEntry) => void;
  onDiscard: (entry: RecoveryEntry) => void;
  onLater: () => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <Modal title={t("recovery.title")} onEscape={props.onLater}>
      <p>{t("recovery.body")}</p>
      <ul className="recovery-list">
        {props.entries.map((entry) => (
          <li key={entry.path}>
            <strong>{entry.name}</strong>
            <span className="muted selectable">{entry.path}</span>
            <span className="muted">
              {t("recovery.savedAt", { date: formatDate(entry.savedAt, i18n.language) })}
            </span>
            {entry.diskState !== "unchanged" && (
              <span className="review-note">{t(`recovery.disk.${entry.diskState}`)}</span>
            )}
            <span className="modal-actions">
              <button type="button" className="danger" onClick={() => props.onDiscard(entry)}>
                {t("recovery.discard")}
              </button>
              <button type="button" className="primary" onClick={() => props.onRestore(entry)}>
                {t("recovery.restore")}
              </button>
            </span>
          </li>
        ))}
      </ul>
      <div className="modal-actions">
        <button type="button" onClick={props.onLater}>
          {t("recovery.later")}
        </button>
      </div>
    </Modal>
  );
}
