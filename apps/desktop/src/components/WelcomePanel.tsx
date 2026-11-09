import { useTranslation } from "react-i18next";
import { RecentEntry } from "../api/backend";
import { formatDate } from "./format";
import { icons } from "./icons";

export function WelcomePanel(props: {
  recent: RecentEntry[];
  onNew: () => void;
  onOpen: () => void;
  onOpenRecent: (path: string) => void;
  onRemoveRecent: (path: string) => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <div className="welcome">
      <h1>{t("welcome.title")}</h1>
      <p className="muted">{t("welcome.subtitle")}</p>
      <div className="start-actions">
        <button type="button" className="start-card" onClick={props.onNew}>
          {icons.newFile}
          <strong>{t("welcome.newProject")}</strong>
          <span>{t("welcome.newProjectHint")}</span>
        </button>
        <button type="button" className="start-card" onClick={props.onOpen}>
          {icons.open}
          <strong>{t("welcome.openProject")}</strong>
          <span>{t("welcome.openProjectHint")}</span>
        </button>
      </div>
      <section aria-labelledby="recent-heading">
        <h2 id="recent-heading">{t("welcome.recent")}</h2>
        {props.recent.length === 0 ? (
          <p className="muted">{t("welcome.noRecent")}</p>
        ) : (
          <ul className="recent-list">
            {props.recent.map((entry) => (
              <li key={entry.path}>
                <button type="button" className="recent-open" onClick={() => props.onOpenRecent(entry.path)}>
                  <strong>{entry.name}</strong>
                  <span className="recent-path">{entry.path}</span>
                  <span className="muted">
                    {t("welcome.openedAt", { date: formatDate(entry.openedAt, i18n.language) })}
                  </span>
                </button>
                <button
                  type="button"
                  className="icon-button"
                  aria-label={t("welcome.removeRecent", { name: entry.name })}
                  title={t("welcome.removeRecent", { name: entry.name })}
                  onClick={() => props.onRemoveRecent(entry.path)}
                >
                  {icons.close}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
