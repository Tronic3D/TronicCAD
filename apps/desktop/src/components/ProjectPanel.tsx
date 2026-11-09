import { useState } from "react";
import { useTranslation } from "react-i18next";
import { backend, ProjectView } from "../api/backend";
import { errorMessage, formatDate } from "./format";

export function ProjectPanel(props: {
  project: ProjectView;
  onChanged: (project: ProjectView) => void;
  onValidity: (valid: boolean) => void;
  onOpenSchematic: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { project } = props;
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description);
  const [error, setError] = useState<string | null>(null);

  const update = async (nextName: string, nextDescription: string) => {
    setName(nextName);
    setDescription(nextDescription);
    try {
      props.onChanged(await backend.updateProject(nextName, nextDescription));
      setError(null);
      props.onValidity(true);
    } catch (failure) {
      setError(errorMessage(t, failure));
      props.onValidity(false);
    }
  };

  return (
    <div className="project-layout">
      <section className="panel" aria-labelledby="details-heading">
        <h2 id="details-heading">{t("project.details")}</h2>
        <label className="field">
          <span>{t("project.name")}</span>
          <input
            value={name}
            aria-invalid={error !== null}
            onChange={(event) => void update(event.target.value, description)}
          />
        </label>
        {error && (
          <p className="field-error" role="alert">
            {error}
          </p>
        )}
        <label className="field">
          <span>{t("project.description")}</span>
          <textarea
            rows={5}
            value={description}
            onChange={(event) => void update(name, event.target.value)}
          />
        </label>
        <dl className="facts">
          <dt>{t("project.file")}</dt>
          <dd className="selectable">{project.path}</dd>
          <dt>{t("project.created")}</dt>
          <dd>{formatDate(project.createdAt, i18n.language)}</dd>
          <dt>{t("project.modified")}</dt>
          <dd>{formatDate(project.modifiedAt, i18n.language)}</dd>
          <dt>{t("project.formatVersion")}</dt>
          <dd>{t("project.formatVersionValue", { version: project.formatVersion })}</dd>
          <dt>{t("project.id")}</dt>
          <dd className="selectable mono">{project.id}</dd>
        </dl>
      </section>
      <section className="panel panel-empty">
        <h2>{t("modules.schematic")}</h2>
        <p className="muted">{t("project.schematicBody")}</p>
        <button type="button" className="primary" onClick={props.onOpenSchematic}>
          {t("project.openSchematic")}
        </button>
      </section>
    </div>
  );
}
