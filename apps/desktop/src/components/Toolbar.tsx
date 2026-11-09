import { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { LANGUAGES } from "../i18n";
import { icons } from "./icons";

function ToolButton(props: {
  icon: ReactNode;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="tool-button"
      disabled={props.disabled}
      onClick={props.onClick}
      title={props.shortcut ? `${props.label} (${props.shortcut})` : props.label}
    >
      {props.icon}
      <span>{props.label}</span>
    </button>
  );
}

export function Toolbar(props: {
  hasProject: boolean;
  canSave: boolean;
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onClose: () => void;
  language: string;
  onLanguage: (language: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <header className="toolbar">
      <div className="brand">
        {icons.logo}
        <span className="brand-name">TronicCAD</span>
      </div>
      <div className="tool-group">
        <ToolButton icon={icons.newFile} label={t("toolbar.new")} shortcut="Ctrl+N" onClick={props.onNew} />
        <ToolButton icon={icons.open} label={t("toolbar.open")} shortcut="Ctrl+O" onClick={props.onOpen} />
      </div>
      <div className="tool-group">
        <ToolButton
          icon={icons.save}
          label={t("toolbar.save")}
          shortcut="Ctrl+S"
          disabled={!props.canSave}
          onClick={props.onSave}
        />
        <ToolButton
          icon={icons.saveAs}
          label={t("toolbar.saveAs")}
          shortcut="Ctrl+Shift+S"
          disabled={!props.canSave}
          onClick={props.onSaveAs}
        />
        <ToolButton
          icon={icons.close}
          label={t("toolbar.close")}
          disabled={!props.hasProject}
          onClick={props.onClose}
        />
      </div>
      <label className="language-picker">
        <span>{t("toolbar.language")}</span>
        <select value={props.language} onChange={(event) => props.onLanguage(event.target.value)}>
          {LANGUAGES.map(({ code, label }) => (
            <option key={code} value={code}>
              {label}
            </option>
          ))}
        </select>
      </label>
    </header>
  );
}
