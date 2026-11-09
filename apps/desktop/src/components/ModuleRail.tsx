import { useTranslation } from "react-i18next";
import { icons } from "./icons";

export type Module = "project" | "schematic" | "pcb" | "view3d" | "simulation" | "ai" | "cloud" | "team" | "update" | "extensions" | "help";

const PROJECT_MODULES = ["schematic", "pcb", "view3d", "simulation", "ai", "cloud", "team"] as const;

export function ModuleRail(props: {
  active: Module;
  hasProject: boolean;
  onSelect: (module: Module) => void;
}) {
  const { t } = useTranslation();
  return (
    <nav className="rail" aria-label={t("modules.label")}>
      <button
        type="button"
        className={`rail-item${props.active === "project" ? " rail-item-active" : ""}`}
        aria-current={props.active === "project" ? "page" : undefined}
        onClick={() => props.onSelect("project")}
      >
        {icons.project}
        <span className="rail-text">{t("modules.project")}</span>
      </button>
      {PROJECT_MODULES.map((id) => (
          <button
            key={id}
            type="button"
            className={`rail-item${props.active === id ? " rail-item-active" : ""}`}
            aria-current={props.active === id ? "page" : undefined}
            disabled={!props.hasProject}
            onClick={() => props.onSelect(id)}
          >
            {icons[id]}
            <span className="rail-text">{t(`modules.${id}`)}</span>
          </button>
      ))}
      <button
        type="button"
        className={`rail-item${props.active === "update" ? " rail-item-active" : ""}`}
        aria-current={props.active === "update" ? "page" : undefined}
        onClick={() => props.onSelect("update")}
      >
        {icons.update}
        <span className="rail-text">{t("modules.update")}</span>
      </button>
      <button
        type="button"
        className={`rail-item${props.active === "extensions" ? " rail-item-active" : ""}`}
        aria-current={props.active === "extensions" ? "page" : undefined}
        onClick={() => props.onSelect("extensions")}
      >
        {icons.extensions}
        <span className="rail-text">{t("modules.extensions")}</span>
      </button>
      <button
        type="button"
        className={`rail-item${props.active === "help" ? " rail-item-active" : ""}`}
        aria-current={props.active === "help" ? "page" : undefined}
        onClick={() => props.onSelect("help")}
      >
        {icons.help}
        <span className="rail-text">{t("modules.help")}</span>
      </button>
    </nav>
  );
}
