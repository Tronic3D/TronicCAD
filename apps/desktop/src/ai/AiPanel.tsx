import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { Modal } from "../components/dialogs";
import { errorMessage } from "../components/format";
import {
  AiAnalysis,
  AiPrepared,
  AiReply,
  AiSettings,
  AiStatus,
  Change,
  ChatMessage,
  Consent,
  Preview,
  ProviderConfig,
  ProviderKind,
  Sections,
  Task,
} from "./types";

const TASKS: Task[] = [
  "free",
  "create_circuit",
  "analyze_schematic",
  "recommend_parts",
  "evaluate_datasheet",
  "explain_pins",
  "explain_errors",
  "improve",
  "analyze_board",
  "manufacturing",
  "documentation",
];

const NONE: Sections = { schematic: false, erc: false, board: false, drc: false, library: false };

/** What usually goes along. The user sees it and can change it before sending. */
const SECTIONS_FOR: Record<Task, Sections> = {
  free: { ...NONE, schematic: true },
  create_circuit: { ...NONE, library: true },
  analyze_schematic: { ...NONE, schematic: true, erc: true },
  recommend_parts: { ...NONE, schematic: true },
  evaluate_datasheet: NONE,
  explain_pins: { ...NONE, schematic: true, library: true },
  explain_errors: { ...NONE, schematic: true, erc: true, board: true, drc: true },
  improve: { ...NONE, schematic: true, erc: true },
  analyze_board: { ...NONE, board: true, drc: true },
  manufacturing: { ...NONE, board: true, drc: true },
  documentation: { ...NONE, schematic: true, board: true },
};

const SECTION_KEYS = Object.keys(NONE) as (keyof Sections)[];

const PRESETS: Record<string, ProviderConfig | null> = {
  rules: null,
  local: { kind: "open_ai_compatible", base_url: "http://localhost:11434", model: "", max_tokens: null },
  anthropic: { kind: "anthropic", base_url: "https://api.anthropic.com", model: "claude-opus-5-5", max_tokens: null },
  custom: { kind: "open_ai_compatible", base_url: "https://", model: "", max_tokens: null },
};

const gigabytes = (bytes: number, language: string) =>
  new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(bytes / 1024 ** 3);

function ProviderForm(props: {
  settings: AiSettings;
  onChanged: (settings: AiSettings) => void;
  onFailure: (message: string | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const { settings, onChanged, onFailure } = props;
  const [preset, setPreset] = useState(() => {
    const config = settings.config;
    if (!config) return "rules";
    if (settings.locality === "local") return "local";
    return config.kind === "anthropic" ? "anthropic" : "custom";
  });
  const [form, setForm] = useState<ProviderConfig>(settings.config ?? (PRESETS.local as ProviderConfig));
  const [limit, setLimit] = useState(settings.config?.max_tokens?.toString() ?? "");
  const [key, setKey] = useState("");
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [saved, setSaved] = useState(false);

  const run = async (action: () => Promise<AiSettings>) => {
    try {
      onFailure(null);
      onChanged(await action());
      return true;
    } catch (error) {
      onFailure(errorMessage(t, error));
      return false;
    }
  };

  const choosePreset = (name: string) => {
    setPreset(name);
    setSaved(false);
    setStatus(null);
    const next = PRESETS[name];
    if (next) setForm(next);
    else void run(() => backend.ai.setProvider(null));
  };

  const parsedLimit = limit.trim() === "" ? null : Number(limit);
  const limitValid = parsedLimit === null || (Number.isInteger(parsedLimit) && parsedLimit > 0);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!limitValid) return;
    setSaved(await run(() => backend.ai.setProvider({ ...form, max_tokens: parsedLimit })));
  };

  const probe = async () => {
    try {
      onFailure(null);
      setStatus(await backend.ai.status());
    } catch (error) {
      onFailure(errorMessage(t, error));
    }
  };

  const update = (change: Partial<ProviderConfig>) => {
    setSaved(false);
    setForm({ ...form, ...change });
  };

  return (
    <>
      <label className="field">
        <span>{t("ai.provider.mode")}</span>
        <select value={preset} onChange={(event) => choosePreset(event.target.value)}>
          {Object.keys(PRESETS).map((name) => (
            <option key={name} value={name}>
              {t(`ai.provider.presets.${name}`)}
            </option>
          ))}
        </select>
      </label>
      {preset === "rules" && <p className="muted hint">{t("ai.provider.rulesHint")}</p>}
      {preset === "local" && <p className="muted hint">{t("ai.provider.localHint")}</p>}
      {preset !== "rules" && (
        <form onSubmit={(event) => void submit(event)}>
          {preset === "custom" && (
            <label className="field">
              <span>{t("ai.provider.protocol")}</span>
              <select value={form.kind} onChange={(event) => update({ kind: event.target.value as ProviderKind })}>
                <option value="open_ai_compatible">{t("ai.provider.openAi")}</option>
                <option value="anthropic">{t("ai.provider.anthropic")}</option>
              </select>
            </label>
          )}
          <label className="field">
            <span>{t("ai.provider.url")}</span>
            <input value={form.base_url} spellCheck={false} onChange={(event) => update({ base_url: event.target.value })} />
          </label>
          <label className="field">
            <span>{t("ai.provider.model")}</span>
            <input
              value={form.model}
              list="ai-models"
              spellCheck={false}
              onChange={(event) => update({ model: event.target.value })}
            />
            <datalist id="ai-models">
              {status?.models.map((model) => <option key={model} value={model} />)}
            </datalist>
          </label>
          <label className="field">
            <span>{t("ai.provider.maxTokens")}</span>
            <input
              value={limit}
              inputMode="numeric"
              placeholder={t("ai.provider.maxTokensDefault")}
              aria-invalid={!limitValid}
              onChange={(event) => {
                setSaved(false);
                setLimit(event.target.value);
              }}
            />
          </label>
          <div className="sim-actions">
            <button type="submit" className="primary" disabled={!limitValid}>
              {t("ai.provider.save")}
            </button>
            <button type="button" disabled={!settings.config} onClick={() => void probe()}>
              {t("ai.provider.test")}
            </button>
          </div>
          {saved && <p role="status" className="muted hint">{t("ai.provider.saved")}</p>}
        </form>
      )}
      {settings.config && settings.locality && (
        <p className={settings.locality === "local" ? "ai-badge ai-badge-local" : "ai-badge ai-badge-external"}>
          {t(`ai.provider.locality.${settings.locality}`)}
        </p>
      )}
      {settings.config && preset !== "rules" && (
        <div className="ai-key">
          <h3>{t("ai.key.title")}</h3>
          {!settings.key_store_available && <p className="sim-warning">{t("ai.key.unavailable")}</p>}
          <p className="muted hint">{settings.has_key ? t("ai.key.stored") : t("ai.key.none")}</p>
          <label className="field">
            <span>{t("ai.key.label")}</span>
            <input
              type="password"
              value={key}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setKey(event.target.value)}
            />
          </label>
          <div className="sim-actions">
            <button
              type="button"
              disabled={key.trim() === ""}
              onClick={() =>
                void run(() => backend.ai.setKey(key)).then((done) => {
                  if (done) setKey("");
                })
              }
            >
              {t("ai.key.save")}
            </button>
            {settings.has_key && (
              <button type="button" onClick={() => void run(() => backend.ai.deleteKey())}>
                {t("ai.key.delete")}
              </button>
            )}
          </div>
        </div>
      )}
      {status && (
        <div className="ai-status" role="status">
          {status.reachable === true && <p>{t("ai.status.reachable", { count: status.models.length })}</p>}
          {status.reachable === false && status.error && <p className="sim-warning">{errorMessage(t, status.error)}</p>}
          <p className="muted hint">
            {t("ai.status.memory", {
              available: gigabytes(status.memory_available, i18n.language),
              total: gigabytes(status.memory_total, i18n.language),
            })}
          </p>
          {status.loaded.map((model) => (
            <p className="muted hint" key={model.name}>
              {t("ai.status.loaded", {
                name: model.name,
                size: gigabytes(model.bytes, i18n.language),
                gpu: gigabytes(model.gpu_bytes, i18n.language),
                interpolation: { escapeValue: false },
              })}
            </p>
          ))}
          {status.reachable === true && status.loaded.length === 0 && settings.locality === "local" && (
            <p className="muted hint">{t("ai.status.notReported")}</p>
          )}
        </div>
      )}
    </>
  );
}

function ChangeLine({ change }: { change: Change }) {
  const { t } = useTranslation();
  const params = { ...change, value: "value" in change ? (change.value ?? "") : "", interpolation: { escapeValue: false } };
  return <>{t(`ai.proposal.change.${change.type}`, params)}</>;
}

function ProposalPanel(props: {
  preview: Preview;
  onApply: (indices: number[]) => void;
  onDiscard: () => void;
}) {
  const { t } = useTranslation();
  const { preview } = props;
  const [chosen, setChosen] = useState<ReadonlySet<number>>(() => new Set(preview.changes.map((_, index) => index)));
  return (
    <section className="ai-proposal" aria-label={t("ai.proposal.title")}>
      <h3>{t("ai.proposal.title")}</h3>
      <p className="muted hint">{t("ai.proposal.hint")}</p>
      <ul className="ai-changes">
        {preview.changes.map((change, index) => (
          <li key={index}>
            <label>
              <input
                type="checkbox"
                checked={chosen.has(index)}
                onChange={(event) => {
                  const next = new Set(chosen);
                  if (event.target.checked) next.add(index);
                  else next.delete(index);
                  setChosen(next);
                }}
              />
              <span>
                <ChangeLine change={change} />
              </span>
            </label>
          </li>
        ))}
      </ul>
      {preview.rejected.length > 0 && (
        <>
          <h3>{t("ai.proposal.rejected")}</h3>
          <ul className="ai-rejected">
            {preview.rejected.map((item, index) => (
              <li key={index} className="sim-warning">
                {t(`ai.proposal.reason.${item.code}`, { detail: item.detail, interpolation: { escapeValue: false } })}
              </li>
            ))}
          </ul>
        </>
      )}
      <p aria-label={t("ai.proposal.ercLabel")}>
        {t("ai.proposal.erc", {
          beforeErrors: preview.erc_before.errors,
          beforeWarnings: preview.erc_before.warnings,
          afterErrors: preview.erc_after.errors,
          afterWarnings: preview.erc_after.warnings,
        })}
      </p>
      <div className="sim-actions">
        <button type="button" className="primary" disabled={chosen.size === 0} onClick={() => props.onApply([...chosen])}>
          {t("ai.proposal.apply", { count: chosen.size })}
        </button>
        <button type="button" onClick={props.onDiscard}>
          {t("ai.proposal.discard")}
        </button>
      </div>
    </section>
  );
}

function ConsentDialog(props: {
  prepared: AiPrepared;
  onAnswer: (consent: Consent | null) => void;
}) {
  const { t } = useTranslation();
  const { prepared } = props;
  return (
    <Modal title={t("ai.consent.title")} onEscape={() => props.onAnswer(null)}>
      <p>{t("ai.consent.body", { origin: prepared.origin, model: prepared.model, interpolation: { escapeValue: false } })}</p>
      <p className="review-note">{t("ai.consent.note")}</p>
      <PayloadView prepared={prepared} />
      <div className="modal-actions">
        <button type="button" autoFocus onClick={() => props.onAnswer(null)}>
          {t("common.cancel")}
        </button>
        <button type="button" onClick={() => props.onAnswer("project")}>
          {t("ai.consent.project")}
        </button>
        <button type="button" className="primary" onClick={() => props.onAnswer("once")}>
          {t("ai.consent.once")}
        </button>
      </div>
    </Modal>
  );
}

function PayloadView({ prepared }: { prepared: AiPrepared }) {
  const { t } = useTranslation();
  const text = [
    `[system]\n${prepared.request.system}`,
    ...prepared.request.messages.map((message) => `[${message.role}]\n${message.content}`),
  ].join("\n\n");
  return (
    <>
      <h3>{t("ai.consent.payload")}</h3>
      <pre className="sim-deck ai-payload" tabIndex={0} data-testid="ai-payload">
        {text}
      </pre>
    </>
  );
}

export function AiPanel(props: { onDirty: (dirty: boolean) => void; onError: (error: unknown) => void }) {
  const { t, i18n } = useTranslation();
  const { onDirty, onError } = props;
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [analysis, setAnalysis] = useState<AiAnalysis | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [task, setTask] = useState<Task>("free");
  const [sections, setSections] = useState<Sections>(SECTIONS_FOR.free);
  const [text, setText] = useState("");
  const [document, setDocument] = useState("");
  // What the model gets again with each turn, and what the conversation shows.
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [transcript, setTranscript] = useState<ChatMessage[]>([]);
  const [lastReply, setLastReply] = useState<AiReply | null>(null);
  const [lastSent, setLastSent] = useState<AiPrepared | null>(null);
  const [streaming, setStreaming] = useState<string | null>(null);
  const [consentFor, setConsentFor] = useState<AiPrepared | null>(null);
  const [proposal, setProposal] = useState<Preview | null>(null);
  const [applied, setApplied] = useState(false);
  const [skipped, setSkipped] = useState<{ code: string; detail: string }[]>([]);
  const activeId = useRef<number | null>(null);

  const refreshAnalysis = useCallback(() => {
    backend.ai.analysis().then(setAnalysis).catch(onError);
  }, [onError]);

  useEffect(() => {
    backend.ai.settings().then(setSettings).catch(onError);
    refreshAnalysis();
  }, [onError, refreshAnalysis]);

  useEffect(() => {
    const pending = backend.ai.onText((id, piece) => {
      if (id === activeId.current) setStreaming((current) => (current ?? "") + piece);
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const busy = streaming !== null;
  const canAsk = settings?.config != null && !busy && (text.trim() !== "" || task !== "free");

  const send = async (prepared: AiPrepared, consent: Consent) => {
    const typed = text.trim() || t(`ai.tasks.${task}`);
    activeId.current = prepared.id;
    setStreaming("");
    setFailure(null);
    setLastSent(prepared);
    try {
      const reply = await backend.ai.send(prepared.id, consent);
      // Stopped by the user: the answer no longer counts.
      if (activeId.current !== prepared.id) return;
      const question = prepared.request.messages[prepared.request.messages.length - 1] as ChatMessage;
      if (reply.stop !== "cancelled") {
        setHistory([...prepared.request.messages.slice(0, -1), question, { role: "assistant", content: reply.text }]);
        setTranscript((current) => [
          ...current,
          { role: "user", content: typed },
          { role: "assistant", content: reply.text },
        ]);
      }
      setLastReply(reply);
      setProposal(reply.proposal);
      setApplied(false);
      setText("");
      setDocument("");
    } catch (error) {
      if (activeId.current === prepared.id) setFailure(errorMessage(t, error));
    } finally {
      if (activeId.current === prepared.id) {
        activeId.current = null;
        setStreaming(null);
      }
    }
  };

  const ask = async (event: FormEvent) => {
    event.preventDefault();
    if (!canAsk) return;
    try {
      setFailure(null);
      const prepared = await backend.ai.prepare({ task, text, document, sections, language: i18n.language, history });
      if (prepared.needs_consent) setConsentFor(prepared);
      else await send(prepared, "none");
    } catch (error) {
      setFailure(errorMessage(t, error));
    }
  };

  const stop = () => {
    activeId.current = null;
    setStreaming(null);
    void backend.ai.cancel();
  };

  const newConversation = () => {
    setHistory([]);
    setTranscript([]);
    setLastReply(null);
    setLastSent(null);
    setProposal(null);
  };

  const applyProposal = async (indices: number[]) => {
    try {
      const result = await backend.ai.apply(indices);
      setProposal(null);
      setApplied(true);
      setSkipped(result.skipped);
      onDirty(result.dirty);
      refreshAnalysis();
    } catch (error) {
      setProposal(null);
      setFailure(errorMessage(t, error));
    }
  };

  const saveAnswer = async () => {
    if (!lastReply) return;
    try {
      const path = await backend.pickSaveFile(t("fileFilter.markdown"), "md", "assistant.md");
      if (path) await backend.sim.saveText(path, `${lastReply.text}\n`);
    } catch (error) {
      onError(error);
    }
  };

  const speed =
    lastReply?.output_tokens && lastReply.seconds > 0
      ? new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(lastReply.output_tokens / lastReply.seconds)
      : null;

  return (
    <div className="ai">
      <aside className="sim-form" aria-label={t("ai.provider.title")}>
        <h2>{t("ai.provider.title")}</h2>
        {settings && <ProviderForm settings={settings} onChanged={setSettings} onFailure={setFailure} />}
      </aside>

      <div className="sim-results">
        {failure && (
          <p className="sim-warning" role="alert">
            {failure}
          </p>
        )}
        <section aria-label={t("ai.checks.title")} className="ai-checks">
          <h2>{t("ai.checks.title")}</h2>
          {analysis && (
            <p>
              {t("ai.checks.erc", { errors: analysis.checks.erc.errors, warnings: analysis.checks.erc.warnings })}
              {" · "}
              {t("ai.checks.drc", { errors: analysis.checks.drc.errors, warnings: analysis.checks.drc.warnings })}
            </p>
          )}
          <p className="muted hint">{t("ai.checks.hint")}</p>
        </section>

        <section aria-label={t("ai.rules.title")}>
          <div className="ai-heading">
            <h2>{t("ai.rules.title")}</h2>
            <button type="button" onClick={refreshAnalysis}>
              {t("ai.rules.refresh")}
            </button>
          </div>
          <p className="muted hint">{t("ai.rules.hint")}</p>
          {analysis?.findings.length === 0 && <p>{t("ai.rules.none")}</p>}
          <ul className="ai-findings">
            {analysis?.findings.map((finding, index) => (
              <li key={`${finding.code}-${index}`} className={`ai-finding ai-finding-${finding.level}`}>
                {t(`ai.rules.findings.${finding.code}`, { ...finding.params, interpolation: { escapeValue: false } })}
              </li>
            ))}
          </ul>
        </section>

        <section aria-label={t("ai.chat.title")}>
          <div className="ai-heading">
            <h2>{t("ai.chat.title")}</h2>
            {transcript.length > 0 && (
              <button type="button" onClick={newConversation}>
                {t("ai.chat.new")}
              </button>
            )}
          </div>
          {!settings?.config && <p className="muted">{t("ai.chat.noProvider")}</p>}
          {transcript.map((message, index) => (
            <div key={index} className={`ai-message ai-message-${message.role}`}>
              <small>{message.role === "assistant" ? t("ai.chat.answerLabel") : t("ai.chat.youLabel")}</small>
              <pre className="ai-text">{message.content}</pre>
            </div>
          ))}
          {streaming !== null && (
            <div className="ai-message ai-message-assistant" role="status">
              <small>{t("ai.chat.answerLabel")}</small>
              <pre className="ai-text">{streaming || t("ai.chat.waiting")}</pre>
            </div>
          )}
          {lastReply?.possibly_cut && (
            <p className="sim-warning" role="alert">
              {lastReply.context_length
                ? t("ai.chat.cutKnown", { limit: lastReply.context_length })
                : t("ai.chat.cut")}
            </p>
          )}
          {lastReply?.stop === "truncated" && <p className="sim-warning">{t("ai.chat.truncated")}</p>}
          {lastReply?.stop === "refused" && <p className="sim-warning">{t("ai.chat.refused")}</p>}
          {lastReply && !busy && (
            <p className="muted hint">
              {t("ai.chat.usage", {
                seconds: new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(lastReply.seconds),
                input: lastReply.input_tokens ?? "?",
                output: lastReply.output_tokens ?? "?",
              })}
              {speed && ` ${t("ai.chat.speed", { speed })}`}
            </p>
          )}
          {lastReply && !busy && (
            <button type="button" onClick={() => void saveAnswer()}>
              {t("ai.chat.save")}
            </button>
          )}
          {applied && <p role="status">{t("ai.proposal.applied")}</p>}
          {applied &&
            skipped.map((item, index) => (
              <p key={index} className="sim-warning">
                {t("ai.proposal.skipped", {
                  reason: t(`ai.proposal.reason.${item.code}`, { detail: item.detail, interpolation: { escapeValue: false } }),
                  interpolation: { escapeValue: false },
                })}
              </p>
            ))}
          <form onSubmit={(event) => void ask(event)}>
            <label className="field">
              <span>{t("ai.chat.task")}</span>
              <select
                value={task}
                onChange={(event) => {
                  const next = event.target.value as Task;
                  setTask(next);
                  setSections(SECTIONS_FOR[next]);
                }}
              >
                {TASKS.map((item) => (
                  <option key={item} value={item}>
                    {t(`ai.tasks.${item}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("ai.chat.question")}</span>
              <textarea value={text} rows={3} onChange={(event) => setText(event.target.value)} />
            </label>
            {task === "evaluate_datasheet" && (
              <label className="field">
                <span>{t("ai.chat.document")}</span>
                <textarea value={document} rows={5} onChange={(event) => setDocument(event.target.value)} />
              </label>
            )}
            <fieldset className="ai-sections">
              <legend>{t("ai.chat.sections")}</legend>
              {SECTION_KEYS.map((key) => (
                <label key={key}>
                  <input
                    type="checkbox"
                    checked={sections[key]}
                    onChange={(event) => setSections({ ...sections, [key]: event.target.checked })}
                  />
                  {t(`ai.sections.${key}`)}
                </label>
              ))}
            </fieldset>
            <div className="sim-actions">
              {busy ? (
                // The key keeps this from becoming the submit button during its own
                // click, which would send the request again.
                <button key="stop" type="button" onClick={stop}>
                  {t("ai.chat.stop")}
                </button>
              ) : (
                <button key="send" type="submit" className="primary" disabled={!canAsk}>
                  {t("ai.chat.send")}
                </button>
              )}
            </div>
          </form>
          {lastSent && (
            <details className="ai-sent">
              <summary>{t("ai.chat.showSent")}</summary>
              <PayloadView prepared={lastSent} />
            </details>
          )}
        </section>
      </div>

      <aside className="sim-form ai-side" aria-label={t("ai.proposal.title")}>
        {proposal ? (
          <ProposalPanel
            key={lastReply?.id}
            preview={proposal}
            onApply={(indices) => void applyProposal(indices)}
            onDiscard={() => {
              setProposal(null);
              void backend.ai.discard();
            }}
          />
        ) : (
          <>
            <h3>{t("ai.proposal.title")}</h3>
            <p className="muted hint">{t("ai.proposal.empty")}</p>
          </>
        )}
        <p className="review-note">{t("ai.note")}</p>
      </aside>

      {consentFor && (
        <ConsentDialog
          prepared={consentFor}
          onAnswer={(consent) => {
            const prepared = consentFor;
            setConsentFor(null);
            if (consent) void send(prepared, consent);
          }}
        />
      )}
    </div>
  );
}
