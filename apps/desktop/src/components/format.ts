import { TFunction } from "i18next";
import { isCommandError } from "../api/backend";

/** Formats an RFC 3339 timestamp according to the active language. */
export function formatDate(value: string, language: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/** Turns anything a backend call rejects with into text in the active language. */
export function errorMessage(t: TFunction, error: unknown): string {
  if (isCommandError(error)) {
    const key = `errors.${error.code}`;
    return t(key, { ...error.params, defaultValue: t("errors.unknown", { detail: error.code }) });
  }
  return t("errors.unknown", { detail: String(error) });
}
