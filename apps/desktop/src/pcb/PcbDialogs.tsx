import { FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Modal } from "../components/dialogs";
import { SyncReport } from "./types";

export function SyncDialog(props: { report: SyncReport; onClose: () => void }) {
  const { t } = useTranslation();
  const { report } = props;
  const problems = [
    ...report.no_footprint.map((reference) => t("pcb.syncResult.noFootprint", { reference })),
    ...report.unknown_footprint.map(([reference, footprint]) =>
      t("pcb.syncResult.unknownFootprint", { reference, footprint }),
    ),
    ...report.missing_pads.map(([reference, pin]) => t("pcb.syncResult.missingPad", { reference, pin })),
  ];
  const changes = [
    ["added", report.added],
    ["updated", report.updated],
    ["removed", report.removed],
  ] as const;
  const nothing = problems.length === 0 && changes.every(([, list]) => list.length === 0);
  return (
    <Modal title={t("pcb.syncResult.title")} onEscape={props.onClose}>
      {nothing && <p>{t("pcb.syncResult.nothing")}</p>}
      {changes.map(
        ([key, list]) =>
          list.length > 0 && (
            <p key={key}>
              <strong>{t(`pcb.syncResult.${key}`)}</strong>: {list.join(", ")}
            </p>
          ),
      )}
      {problems.length > 0 && (
        <>
          <p className="field-error">{t("pcb.syncResult.problems")}</p>
          <ul className="problem-list">
            {problems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        </>
      )}
      <div className="modal-actions">
        <button type="button" className="primary" autoFocus onClick={props.onClose}>
          {t("common.close")}
        </button>
      </div>
    </Modal>
  );
}

export function ZoneDialog(props: {
  nets: string[];
  layerName: string;
  onCancel: () => void;
  onConfirm: (net: string) => void;
}) {
  const { t } = useTranslation();
  const [net, setNet] = useState(props.nets.find((name) => name === "GND") ?? props.nets[0] ?? "");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (net) props.onConfirm(net);
  };
  return (
    <Modal title={t("pcb.zone.title")} onEscape={props.onCancel}>
      <form onSubmit={submit}>
        <label className="field">
          <span>{t("pcb.zone.net")}</span>
          <select autoFocus value={net} onChange={(event) => setNet(event.target.value)}>
            {props.nets.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
        <p className="muted">{t("pcb.zone.layer", { layer: props.layerName })}</p>
        <div className="modal-actions">
          <button type="button" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!net}>
            {t("common.ok")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function ExportDialog(props: {
  state: "ask" | string[];
  errors: number;
  onContinue: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  if (props.state === "ask") {
    return (
      <Modal title={t("pcb.exportDialog.errorsTitle")} onEscape={props.onClose}>
        <p>{t("pcb.exportDialog.errorsBody", { count: props.errors })}</p>
        <div className="modal-actions">
          <button type="button" className="primary" onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button type="button" className="danger" onClick={props.onContinue}>
            {t("pcb.exportDialog.anyway")}
          </button>
        </div>
      </Modal>
    );
  }
  return (
    <Modal title={t("pcb.exportDialog.doneTitle")} onEscape={props.onClose}>
      <p>{t("pcb.exportDialog.done", { count: props.state.length })}</p>
      <p className="review-note">{t("pcb.exportDialog.check")}</p>
      <ul className="problem-list mono">
        {props.state.map((name) => (
          <li key={name}>{name}</li>
        ))}
      </ul>
      <div className="modal-actions">
        <button type="button" className="primary" autoFocus onClick={props.onClose}>
          {t("common.close")}
        </button>
      </div>
    </Modal>
  );
}

export function RouterProgressDialog(props: { done: number; total: number; onCancel: () => void }) {
  const { t } = useTranslation();
  return (
    <Modal title={t("pcb.router.running")} onEscape={props.onCancel}>
      <p role="status">{t("pcb.router.progress", { done: props.done, total: props.total })}</p>
      <progress className="router-progress" max={Math.max(props.total, 1)} value={props.done} />
      <div className="modal-actions">
        <button type="button" onClick={props.onCancel}>
          {t("common.cancel")}
        </button>
      </div>
    </Modal>
  );
}
