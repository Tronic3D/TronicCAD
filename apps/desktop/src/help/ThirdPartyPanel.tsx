import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { errorMessage } from "../components/format";
import programs from "./programs.json";

export interface Component {
  name: string;
  version: string;
  license: string;
  author: string;
}

// Shows what TronicCAD is built from: programs, source components and their full notices.
export function ThirdPartyPanel() {
  const { t } = useTranslation();
  const [components, setComponents] = useState<Component[]>([]);
  const [query, setQuery] = useState("");
  const [text, setText] = useState<{ title: string; body: string } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    backend.thirdParty.components().then(setComponents).catch((error) => setProblem(errorMessage(t, error)));
  }, [t]);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle
      ? components.filter((component) => `${component.name} ${component.license}`.toLowerCase().includes(needle))
      : components;
  }, [components, query]);

  const open = (title: string, load: () => Promise<string>) => {
    load()
      .then((body) => setText({ title, body }))
      .catch((error) => setProblem(errorMessage(t, error)));
  };

  return (
    <article>
      <h2>{t("help.thirdParty")}</h2>
      {problem && (
        <p className="sim-warning" role="alert">
          {problem}
        </p>
      )}
      <p>{t("help.openSource")}</p>
      <p>{t("help.thirdPartyAbout")}</p>
      <div className="sim-actions">
        <button type="button" onClick={() => open(t("help.ownLicense"), backend.thirdParty.ownLicense)}>
          {t("help.ownLicense")}
        </button>
        <button type="button" onClick={() => open(t("help.fullNotices"), backend.thirdParty.notices)}>
          {t("help.fullNotices")}
        </button>
      </div>
      {text && (
        <section aria-label={text.title}>
          <h3>{text.title}</h3>
          <pre className="help-long" data-testid="third-party-text">
            {text.body}
          </pre>
        </section>
      )}
      <h3>{t("help.programs")}</h3>
      <table className="sim-table" data-testid="third-party-programs">
        <thead>
          <tr>
            <th>{t("help.name")}</th>
            <th>{t("help.usedFor")}</th>
            <th>{t("help.license")}</th>
          </tr>
        </thead>
        <tbody>
          {programs.map((program) => (
            <tr key={program.name}>
              <td>{program.name}</td>
              <td>{t(`help.used.${program.used}`)}</td>
              <td>{program.license}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3>{t("help.components", { count: components.length })}</h3>
      <label className="field">
        <span>{t("help.search")}</span>
        <input value={query} onChange={(event) => setQuery(event.target.value)} />
      </label>
      <table className="sim-table" data-testid="third-party-components">
        <thead>
          <tr>
            <th>{t("help.name")}</th>
            <th>{t("help.version")}</th>
            <th>{t("help.license")}</th>
          </tr>
        </thead>
        <tbody>
          {shown.map((component) => (
            <tr key={`${component.name}@${component.version}`}>
              <td>{component.name}</td>
              <td>{component.version}</td>
              <td>{component.license}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </article>
  );
}
