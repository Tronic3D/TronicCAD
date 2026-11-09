import { FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Modal } from "../components/dialogs";
import { errorMessage } from "../components/format";
import { parsePinList } from "./geometry";
import { CustomPartSpec, LabelKind } from "./types";

export function TextPromptDialog(props: {
  title: string;
  label: string;
  initial: string;
  onCancel: () => void;
  onConfirm: (value: string) => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState(props.initial);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (value.trim()) props.onConfirm(value);
  };
  return (
    <Modal title={props.title} onEscape={props.onCancel}>
      <form onSubmit={submit}>
        <label className="field">
          <span>{props.label}</span>
          <input autoFocus value={value} onChange={(event) => setValue(event.target.value)} />
        </label>
        <div className="modal-actions">
          <button type="button" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!value.trim()}>
            {t("common.ok")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function LabelDialog(props: {
  onCancel: () => void;
  onConfirm: (name: string, kind: LabelKind) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<LabelKind>("local");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim()) props.onConfirm(name, kind);
  };
  return (
    <Modal title={t("schematic.newLabel")} onEscape={props.onCancel}>
      <form onSubmit={submit}>
        <label className="field">
          <span>{t("schematic.labelName")}</span>
          <input autoFocus value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("schematic.labelKind")}</span>
          <select value={kind} onChange={(event) => setKind(event.target.value as LabelKind)}>
            <option value="local">{t("schematic.labelKinds.local")}</option>
            <option value="global">{t("schematic.labelKinds.global")}</option>
            <option value="hierarchical">{t("schematic.labelKinds.hierarchical")}</option>
          </select>
        </label>
        <p className="muted hint">{t("schematic.labelHint")}</p>
        <div className="modal-actions">
          <button type="button" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!name.trim()}>
            {t("common.ok")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function CustomPartDialog(props: {
  onCancel: () => void;
  onCreate: (spec: CustomPartSpec) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [prefix, setPrefix] = useState("U");
  const [pins, setPins] = useState("1 VCC power_in left\n2 GND power_in left\n3 IN input left\n4 OUT output right");
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = parsePinList(pins);
    if (!parsed.ok) {
      setError(t("schematic.customPart.badLine", { line: parsed.line }));
      return;
    }
    try {
      await props.onCreate({
        id: "",
        name,
        category: "custom",
        reference_prefix: prefix.trim(),
        value: name.trim(),
        pins: parsed.pins,
      });
    } catch (failure) {
      setError(errorMessage(t, failure));
    }
  };

  return (
    <Modal title={t("schematic.customPart.title")} onEscape={props.onCancel}>
      <form onSubmit={(event) => void submit(event)}>
        <label className="field">
          <span>{t("schematic.customPart.name")}</span>
          <input autoFocus value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("schematic.customPart.prefix")}</span>
          <input value={prefix} onChange={(event) => setPrefix(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("schematic.customPart.pins")}</span>
          <textarea rows={7} className="mono" spellCheck={false} value={pins} onChange={(event) => setPins(event.target.value)} />
        </label>
        <p className="muted hint">{t("schematic.customPart.pinsHint")}</p>
        {error && (
          <p className="field-error" role="alert">
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!name.trim() || !prefix.trim()}>
            {t("newProject.create")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
