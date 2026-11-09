import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { errorMessage } from "../components/format";

export interface ExtensionView {
  id: string;
  folder: string;
  name: string;
  version: string;
  description: string;
  author: string;
  license: string;
  command: string;
  fingerprint: string;
  trusted: boolean;
  changed: boolean;
  problem: string | null;
}

export interface ExtensionsOverview {
  folder: string;
  api: number;
  extensions: ExtensionView[];
}

export interface RunResult {
  messages: { level: "info" | "warning" | "error"; text: string }[];
  folder: string | null;
  files: string[];
}

export function ExtensionsPanel(props: { hasProject: boolean }) {
  const { t } = useTranslation();
  const [overview, setOverview] = useState<ExtensionsOverview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [asking, setAsking] = useState<ExtensionView | null>(null);
  const [result, setResult] = useState<{ id: string; run: RunResult } | null>(null);
  const [running, setRunning] = useState(false);

  const load = useCallback(async () => {
    setOverview(await backend.extensions.list());
  }, []);

  const attempt = useCallback(
    async (work: () => Promise<void>) => {
      setProblem(null);
      try {
        await work();
      } catch (error) {
        setProblem(errorMessage(t, error));
      }
      await load().catch(() => undefined);
    },
    [t, load],
  );

  useEffect(() => {
    load().catch((error) => setProblem(errorMessage(t, error)));
  }, [load, t]);

  return (
    <div className="sim">
      <aside className="sim-form" aria-label={t("extensions.title")}>
        <h3>{t("extensions.title")}</h3>
        <p className="muted hint">{t("extensions.about")}</p>
        <p className="sim-warning">{t("extensions.warning")}</p>
        {overview && (
          <p className="muted hint" data-testid="extensions-folder">
            {t("extensions.folderHint", { folder: overview.folder })}
          </p>
        )}
        <div className="sim-actions">
          <button type="button" onClick={() => void attempt(() => backend.extensions.openFolder())}>
            {t("extensions.openFolder")}
          </button>
          <button type="button" onClick={() => void attempt(load)}>
            {t("extensions.reload")}
          </button>
        </div>
      </aside>
      <div className="sim-results">
        {problem && (
          <p className="sim-warning" role="alert">
            {problem}
          </p>
        )}
        {overview?.extensions.length === 0 && <p className="muted">{t("extensions.none")}</p>}
        {overview?.extensions.map((extension) => (
          <section key={extension.folder} className="license-card" data-testid="extension-card" aria-label={extension.name}>
            <h3>
              {extension.name} {extension.version}
            </h3>
            {extension.problem ? (
              <p className="sim-warning">{t("extensions.broken", { detail: extension.problem })}</p>
            ) : (
              <>
                <p>{extension.description}</p>
                <p className="muted hint">
                  {t("extensions.details", { author: extension.author, license: extension.license })}
                </p>
                <p className="muted hint">
                  {t("extensions.runs")} <code>{extension.command}</code>
                </p>
                {extension.changed && <p className="sim-warning">{t("extensions.changedHint")}</p>}
                <div className="sim-actions">
                  {extension.trusted ? (
                    <>
                      <button
                        type="button"
                        disabled={!props.hasProject || running}
                        onClick={() => {
                          setRunning(true);
                          void attempt(async () => {
                            setResult(null);
                            setResult({ id: extension.id, run: await backend.extensions.run(extension.id) });
                          }).finally(() => setRunning(false));
                        }}
                      >
                        {t("extensions.run")}
                      </button>
                      <button
                        type="button"
                        onClick={() => void attempt(() => backend.extensions.untrust(extension.id))}
                      >
                        {t("extensions.untrust")}
                      </button>
                    </>
                  ) : (
                    <button type="button" onClick={() => setAsking(extension)}>
                      {t("extensions.trust")}
                    </button>
                  )}
                </div>
                {!props.hasProject && extension.trusted && <p className="muted hint">{t("extensions.needsProject")}</p>}
                {asking?.folder === extension.folder && (
                  <div className="sim-warning" role="group" aria-label={t("extensions.trust")}>
                    <p>{t("extensions.ask", { name: extension.name })}</p>
                    <div className="sim-actions">
                      <button
                        type="button"
                        onClick={() => {
                          setAsking(null);
                          void attempt(() => backend.extensions.trust(extension.id, extension.fingerprint));
                        }}
                      >
                        {t("extensions.confirm")}
                      </button>
                      <button type="button" onClick={() => setAsking(null)}>
                        {t("extensions.cancel")}
                      </button>
                    </div>
                  </div>
                )}
                {result?.id === extension.id && (
                  <div data-testid="extension-result">
                    <ul className="ai-findings">
                      {result.run.messages.map((message, index) => (
                        <li key={index} className={message.level === "info" ? undefined : "sim-warning"}>
                          {message.text}
                        </li>
                      ))}
                    </ul>
                    {result.run.folder && (
                      <p className="muted hint">
                        {t("extensions.saved", { files: result.run.files.join(", "), folder: result.run.folder })}
                      </p>
                    )}
                  </div>
                )}
              </>
            )}
          </section>
        ))}
      </div>
    </div>
  );
}
