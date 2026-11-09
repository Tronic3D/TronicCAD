import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { errorMessage } from "../components/format";

export interface UpdateStatus {
  version: string;
  channel: "stable" | "beta" | "developer";
  automatic: boolean;
  configured: boolean;
  can_install: boolean;
  last_check: number | null;
  available: {
    version: string;
    kind: "security" | "feature";
    notes: Record<string, string>;
    size: number;
    backup_first: boolean;
    downloaded: boolean;
    failed_before: boolean;
  } | null;
  last_error: { code: string; params: Record<string, string> } | null;
  previous_version: string | null;
  backups: { path: string; from: string; to: string; created: number; projects: number; skipped: number }[];
}

const CHANNELS = ["stable", "beta", "developer"] as const;

function when(seconds: number | null, language: string): string {
  return seconds ? new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(seconds * 1000) : "";
}

export function UpdatePanel(props: { onChanged: () => void }) {
  const { t, i18n } = useTranslation();
  const { onChanged } = props;
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    setStatus(await backend.update.status());
  }, []);

  useEffect(() => {
    reload().catch((error) => setProblem(errorMessage(t, error)));
  }, [reload, t]);

  useEffect(() => {
    setProblem(null);
    setNotice(null);
  }, [i18n.language]);

  const run = useCallback(
    async (work: () => Promise<void>, done?: () => string) => {
      setProblem(null);
      setNotice(null);
      setBusy(true);
      try {
        await work();
        if (done) setNotice(done());
      } catch (error) {
        setProblem(errorMessage(t, error));
      }
      setBusy(false);
      await reload().catch(() => undefined);
      onChanged();
    },
    [t, reload, onChanged],
  );

  if (!status) {
    return (
      <div className="panel">
        {problem && (
          <p className="sim-warning" role="alert">
            {problem}
          </p>
        )}
      </div>
    );
  }
  const available = status.available;

  return (
    <div className="sim">
      <aside className="sim-form" aria-label={t("update.title")}>
        <h3>{t("update.title")}</h3>
        <p data-testid="update-version" className="license-edition">
          {t("update.version", { version: status.version })}
        </p>
        {!status.configured && <p className="sim-warning">{t("update.notConfigured")}</p>}
        <label className="field">
          <span>{t("update.channel")}</span>
          <select
            value={status.channel}
            onChange={(event) =>
              void run(() => backend.update.setChannel(event.target.value as UpdateStatus["channel"]))
            }
          >
            {CHANNELS.map((channel) => (
              <option key={channel} value={channel}>
                {t(`update.channels.${channel}`)}
              </option>
            ))}
          </select>
        </label>
        <p className="muted hint">{t("update.channelHint")}</p>
        <label className="sim-traces">
          <input
            type="checkbox"
            checked={status.automatic}
            onChange={(event) => {
              const on = event.target.checked;
              void run(() => backend.update.setAutomatic(on));
            }}
          />
          {t("update.automatic")}
        </label>
        <p className="muted hint">{t("update.privacy")}</p>
        <button
          type="button"
          disabled={!status.configured || busy}
          onClick={() =>
            void run(
              () => backend.update.check(),
              () => t("update.checked"),
            )
          }
        >
          {t("update.check")}
        </button>
        {status.last_check !== null && (
          <p className="muted hint">{t("update.lastCheck", { date: when(status.last_check, i18n.language) })}</p>
        )}
      </aside>

      <div className="sim-results">
        {problem && (
          <p className="sim-warning" role="alert">
            {problem}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}

        {available ? (
          <section aria-labelledby="update-available" className="license-card">
            <h3 id="update-available">{t("update.available", { version: available.version })}</h3>
            <p data-testid="update-kind" className={available.kind === "security" ? "sim-warning" : "muted"}>
              {t(`update.kinds.${available.kind}`)}
            </p>
            <p data-testid="update-notes" className="update-notes">
              {available.notes[i18n.language] ?? available.notes["en-US"] ?? ""}
            </p>
            {available.failed_before && <p className="sim-warning">{t("update.failedBefore")}</p>}
            {available.backup_first && <p className="muted hint">{t("update.backupHint")}</p>}
            {!status.can_install && <p className="sim-warning">{t("update.installUnsupported")}</p>}
            <div className="sim-actions">
              {!available.downloaded ? (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () => backend.update.download(),
                      () => t("update.downloaded"),
                    )
                  }
                >
                  {t("update.download", { size: (available.size / (1024 * 1024)).toFixed(1) })}
                </button>
              ) : (
                <button
                  type="button"
                  disabled={busy || !status.can_install}
                  onClick={() => void run(() => backend.update.install())}
                >
                  {t("update.install")}
                </button>
              )}
            </div>
          </section>
        ) : (
          status.configured && status.last_check !== null && !status.last_error && <p data-testid="update-current">{t("update.upToDate")}</p>
        )}

        {status.previous_version && (
          <section aria-labelledby="update-back">
            <h3 id="update-back">{t("update.backTitle")}</h3>
            <p className="muted hint">{t("update.backHint", { version: status.previous_version })}</p>
            <button type="button" disabled={busy} onClick={() => void run(() => backend.update.rollback())}>
              {t("update.back", { version: status.previous_version })}
            </button>
          </section>
        )}

        {status.backups.length > 0 && (
          <section aria-labelledby="update-backups">
            <h3 id="update-backups">{t("update.backups")}</h3>
            <ul className="ai-findings">
              {status.backups.map((backup) => (
                <li key={backup.path} className="license-device" data-testid="update-backup">
                  <span>
                    {t("update.backupEntry", {
                      date: when(backup.created, i18n.language),
                      from: backup.from,
                      to: backup.to,
                      projects: backup.projects,
                    })}
                    {backup.skipped > 0 && ` ${t("update.backupSkipped", { count: backup.skipped })}`}
                  </span>
                  <button type="button" onClick={() => void run(() => backend.update.openBackup(backup.path))}>
                    {t("update.openBackup")}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
