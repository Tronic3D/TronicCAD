import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { errorMessage } from "../components/format";
import handbook from "./handbook.json";
import { ThirdPartyPanel } from "./ThirdPartyPanel";

export interface SupportInfo {
  version: string;
  platform: string;
  settings_persistent: boolean;
  contact: string | null;
}

interface Section {
  id: string;
  title: string;
  text: string[];
}

const SECTIONS = handbook as Record<string, Section[]>;

// Only **bold** exists in the handbook, and it is rendered as elements, never as HTML.
function Paragraph(props: { text: string }) {
  const parts = props.text.split("**");
  return <p>{parts.map((part, index) => (index % 2 === 1 ? <strong key={index}>{part}</strong> : part))}</p>;
}

export function HelpPanel() {
  const { t, i18n } = useTranslation();
  const sections = SECTIONS[i18n.language] ?? SECTIONS["en-US"] ?? [];
  const [active, setActive] = useState(sections[0]?.id ?? "");
  const [info, setInfo] = useState<SupportInfo | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const shown = sections.find((section) => section.id === active);

  useEffect(() => {
    backend.supportInfo().then(setInfo).catch((error) => setProblem(errorMessage(t, error)));
  }, [t]);

  const report = info
    ? [`TronicCAD ${info.version}`, `${t("help.platform")}: ${info.platform}`].join("\n")
    : "";

  return (
    <div className="sim">
      <nav className="sim-form" aria-label={t("help.contents")}>
        <h3>{t("help.title")}</h3>
        {sections.map((section) => (
          <button
            key={section.id}
            type="button"
            className={`help-link${section.id === active ? " help-link-active" : ""}`}
            aria-current={section.id === active ? "true" : undefined}
            onClick={() => setActive(section.id)}
          >
            {section.title}
          </button>
        ))}
        <button
          type="button"
          className={`help-link${active === "support" ? " help-link-active" : ""}`}
          aria-current={active === "support" ? "true" : undefined}
          onClick={() => setActive("support")}
        >
          {t("help.support")}
        </button>
        <button
          type="button"
          className={`help-link${active === "third-party" ? " help-link-active" : ""}`}
          aria-current={active === "third-party" ? "true" : undefined}
          onClick={() => setActive("third-party")}
        >
          {t("help.thirdParty")}
        </button>
      </nav>
      <div className="sim-results help-text">
        {shown && (
          <article>
            <h2>{shown.title}</h2>
            {shown.text.map((text) => (
              <Paragraph key={text} text={text} />
            ))}
          </article>
        )}
        {active === "third-party" && <ThirdPartyPanel />}
        {active === "support" && (
          <article>
            <h2>{t("help.support")}</h2>
            {problem && (
              <p className="sim-warning" role="alert">
                {problem}
              </p>
            )}
            {info?.contact ? (
              <p data-testid="support-contact">{t("help.contact", { contact: info.contact })}</p>
            ) : (
              <p className="sim-warning" data-testid="support-contact">
                {t("help.noContact")}
              </p>
            )}
            <p>{t("help.reportHint")}</p>
            <pre data-testid="support-report">{report}</pre>
            <p className="muted hint">{t("help.reportPrivacy")}</p>
            <p className="muted hint">{t("help.notices")}</p>
            <div className="sim-actions">
              <button
                type="button"
                disabled={!info}
                onClick={() => {
                  void navigator.clipboard
                    .writeText(report)
                    .then(() => setCopied(true))
                    .catch((error) => setProblem(String(error)));
                }}
              >
                {copied ? t("help.copied") : t("help.copy")}
              </button>
            </div>
          </article>
        )}
      </div>
    </div>
  );
}
