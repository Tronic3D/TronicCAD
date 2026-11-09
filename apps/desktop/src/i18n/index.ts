import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import deDE from "./locales/de-DE.json";
import enUS from "./locales/en-US.json";
import frFR from "./locales/fr-FR.json";
import zhCN from "./locales/zh-CN.json";

export const LANGUAGES = [
  { code: "de-DE", label: "Deutsch" },
  { code: "en-US", label: "English" },
  { code: "zh-CN", label: "简体中文" },
  { code: "fr-FR", label: "Français" },
] as const;

const FALLBACK = "en-US";

/** Maps a browser or OS locale such as `de-AT` to a supported language. */
export function matchLanguage(locale: string | undefined): string {
  const base = locale?.toLowerCase().split("-")[0];
  return LANGUAGES.find(({ code }) => code.toLowerCase().split("-")[0] === base)?.code ?? FALLBACK;
}

function applyDocumentLanguage(language: string) {
  document.documentElement.lang = language;
}

void i18n.use(initReactI18next).init({
  resources: {
    "de-DE": { translation: deDE },
    "en-US": { translation: enUS },
    "fr-FR": { translation: frFR },
    "zh-CN": { translation: zhCN },
  },
  lng: matchLanguage(typeof navigator === "undefined" ? undefined : navigator.language),
  fallbackLng: FALLBACK,
  // React escapes output itself.
  interpolation: { escapeValue: false },
  returnNull: false,
});

applyDocumentLanguage(i18n.language);
i18n.on("languageChanged", applyDocumentLanguage);

export default i18n;
