import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { errorMessage } from "../components/format";
import { formatSi, toCsv, toDb } from "./format";
import { Plot, PlotTrace, TRACE_COLORS } from "./Plot";
import { AnalysisRequest, SimPreparation, SimRun } from "./types";

type Kind = AnalysisRequest["type"];
const KINDS: Kind[] = ["operating_point", "dc_sweep", "ac", "transient"];

// What the form starts with: a reasonable first look at most small circuits.
const DEFAULTS = {
  sweepStart: "0",
  sweepStop: "5",
  sweepStep: "0.1",
  acStart: "10",
  acStop: "1meg",
  ppd: "20",
  tranStop: "10m",
  tranStep: "10u",
};

function Field(props: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input value={props.value} spellCheck={false} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}

function Report({ preparation }: { preparation: SimPreparation }) {
  const { t } = useTranslation();
  const { report } = preparation;
  const list = (items: string[]) => items.join(", ");
  return (
    <>
      {!report.has_ground && <p className="sim-warning">{t("sim.noGround")}</p>}
      {report.bad_values.length > 0 && (
        <p className="sim-warning">
          {t("sim.badValues", {
            parts: list(report.bad_values.map(([part, value]) => `${part} (${value})`)),
            interpolation: { escapeValue: false },
          })}
        </p>
      )}
      {report.unsupported.length > 0 && (
        <p className="sim-warning">{t("sim.unsupported", { parts: list(report.unsupported), interpolation: { escapeValue: false } })}</p>
      )}
      {report.ignored.length > 0 && (
        <p className="muted">{t("sim.ignored", { parts: list(report.ignored), interpolation: { escapeValue: false } })}</p>
      )}
    </>
  );
}

function Results(props: { run: SimRun }) {
  const { t, i18n } = useTranslation();
  const { result } = props.run;
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const [db, setDb] = useState(true);
  const ac = result.x_unit === "Hz";

  const colored = useMemo(
    () => result.traces.map((trace, index) => ({ ...trace, color: TRACE_COLORS[index % TRACE_COLORS.length] as string })),
    [result],
  );

  const groups = useMemo(() => {
    const byUnit = new Map<string, PlotTrace[]>();
    for (const trace of colored) {
      if (hidden.has(trace.name)) continue;
      const asDb = ac && db && trace.unit !== "deg";
      const unit = asDb ? "dB" : trace.unit === "deg" ? "°" : trace.unit;
      const values = asDb ? trace.values.map(toDb) : trace.values;
      byUnit.set(unit, [...(byUnit.get(unit) ?? []), { name: trace.name, color: trace.color, values }]);
    }
    return [...byUnit];
  }, [colored, hidden, ac, db]);

  if (result.x_unit === "point") {
    return (
      <table className="sim-table">
        <thead>
          <tr>
            <th>{t("sim.net")}</th>
            <th>{t("sim.value")}</th>
          </tr>
        </thead>
        <tbody>
          {result.traces.map((trace) => (
            <tr key={trace.name}>
              <td>{trace.name}</td>
              <td data-trace={trace.name}>{formatSi(trace.values[0] ?? Number.NaN, trace.unit, i18n.language)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    );
  }
  return (
    <>
      <h3>{t("sim.traces")}</h3>
      <div className="sim-traces">
        {colored.map((trace) => (
          <label key={trace.name}>
            <input
              type="checkbox"
              checked={!hidden.has(trace.name)}
              onChange={(event) => {
                const next = new Set(hidden);
                if (event.target.checked) next.delete(trace.name);
                else next.add(trace.name);
                setHidden(next);
              }}
            />
            <span className="sim-swatch" style={{ background: trace.color }} />
            {trace.name}
          </label>
        ))}
        {ac && (
          <label>
            <input type="checkbox" checked={db} onChange={(event) => setDb(event.target.checked)} />
            {t("sim.db")}
          </label>
        )}
      </div>
      {groups.map(([unit, traces]) => (
        <Plot key={unit} x={result.x} xUnit={result.x_unit} log={ac} unit={unit} traces={traces} />
      ))}
    </>
  );
}

export function SimulationPanel(props: { onError: (error: unknown) => void }) {
  const { t, i18n } = useTranslation();
  const { onError } = props;
  const [kind, setKind] = useState<Kind>("operating_point");
  const [form, setForm] = useState(DEFAULTS);
  const [source, setSource] = useState("");
  const [supplies, setSupplies] = useState<Record<string, string>>({});
  const [preparation, setPreparation] = useState<SimPreparation | null>(null);
  const [run, setRun] = useState<SimRun | null>(null);
  const [running, setRunning] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  // Nets that ever asked for a voltage keep their field while it gets filled in.
  const [askedSupplies, setAskedSupplies] = useState<string[]>([]);

  const update = (key: keyof typeof DEFAULTS) => (value: string) => setForm((current) => ({ ...current, [key]: value }));

  const onSupplyError = useCallback((error: unknown) => setFailure(errorMessage(t, error)), [t]);

  useEffect(() => {
    let current = true;
    backend.sim
      .prepare(supplies)
      .then((next) => {
        if (!current) return;
        setFailure(null);
        setPreparation(next);
        setAskedSupplies((asked) => [...new Set([...asked, ...next.report.unknown_supplies])]);
        setSource((value) => (next.sources.includes(value) ? value : (next.sources[0] ?? "")));
      })
      .catch((error) => {
        if (current) onSupplyError(error);
      });
    return () => {
      current = false;
    };
  }, [supplies, onSupplyError]);

  const request = (): AnalysisRequest => {
    switch (kind) {
      case "operating_point":
        return { type: kind };
      case "dc_sweep":
        return { type: kind, source, start: form.sweepStart, stop: form.sweepStop, step: form.sweepStep };
      case "ac":
        return { type: kind, start: form.acStart, stop: form.acStop, points_per_decade: Number(form.ppd) };
      case "transient":
        return { type: kind, stop: form.tranStop, step: form.tranStep };
    }
  };

  const start = async (event: FormEvent) => {
    event.preventDefault();
    setRunning(true);
    setFailure(null);
    setNotice(null);
    try {
      const next = await backend.sim.run(supplies, request());
      setRun(next);
      setPreparation(next.preparation);
    } catch (error) {
      setRun(null);
      setFailure(errorMessage(t, error));
    } finally {
      setRunning(false);
    }
  };

  const save = async (filter: string, extension: string, name: string, text: string) => {
    try {
      const path = await backend.pickSaveFile(filter, extension, name);
      if (!path) return;
      await backend.sim.saveText(path, text);
      setNotice(t("sim.saved", { path, interpolation: { escapeValue: false } }));
    } catch (error) {
      onError(error);
    }
  };

  const report = preparation?.report;
  const blocked = !report || report.unknown_supplies.length > 0 || report.bad_values.length > 0;
  const validPoints = Number.isInteger(Number(form.ppd)) && Number(form.ppd) > 0;
  const canRun = !running && !blocked && (kind !== "dc_sweep" || source !== "") && (kind !== "ac" || validPoints);
  const fromNames = (report?.supplies ?? []).filter(([net]) => !askedSupplies.includes(net));

  return (
    <div className="sim">
      <form className="sim-form" onSubmit={(event) => void start(event)}>
        <label className="field">
          <span>{t("sim.analysis")}</span>
          <select value={kind} onChange={(event) => setKind(event.target.value as Kind)}>
            {KINDS.map((item) => (
              <option key={item} value={item}>
                {t(`sim.kinds.${item}`)}
              </option>
            ))}
          </select>
        </label>
        {kind === "dc_sweep" && (
          <>
            <label className="field">
              <span>{t("sim.source")}</span>
              <select value={source} onChange={(event) => setSource(event.target.value)}>
                {preparation?.sources.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            {preparation?.sources.length === 0 && <p className="sim-warning">{t("sim.noSources")}</p>}
            <Field label={t("sim.start")} value={form.sweepStart} onChange={update("sweepStart")} />
            <Field label={t("sim.stop")} value={form.sweepStop} onChange={update("sweepStop")} />
            <Field label={t("sim.step")} value={form.sweepStep} onChange={update("sweepStep")} />
          </>
        )}
        {kind === "ac" && (
          <>
            <Field label={t("sim.fstart")} value={form.acStart} onChange={update("acStart")} />
            <Field label={t("sim.fstop")} value={form.acStop} onChange={update("acStop")} />
            <Field label={t("sim.ppd")} value={form.ppd} onChange={update("ppd")} />
          </>
        )}
        {kind === "transient" && (
          <>
            <Field label={t("sim.tstop")} value={form.tranStop} onChange={update("tranStop")} />
            <Field label={t("sim.tstep")} value={form.tranStep} onChange={update("tranStep")} />
          </>
        )}
        {kind !== "operating_point" && <p className="muted hint">{t("sim.valueHint")}</p>}
        {kind === "transient" && <p className="muted hint">{t("sim.stepHint")}</p>}

        {(askedSupplies.length > 0 || fromNames.length > 0) && <h3>{t("sim.supplies")}</h3>}
        {askedSupplies.map((net) => (
          <label className="field" key={net}>
            <span>{net}</span>
            <input
              value={supplies[net] ?? ""}
              placeholder={t("sim.supplyPlaceholder")}
              spellCheck={false}
              onChange={(event) => setSupplies((current) => ({ ...current, [net]: event.target.value }))}
            />
          </label>
        ))}
        {fromNames.map(([net, value]) => (
          <p className="muted hint" key={net}>
            {t("sim.suppliesFromName", {
              net,
              value: new Intl.NumberFormat(i18n.language).format(value),
              interpolation: { escapeValue: false },
            })}
          </p>
        ))}
        {report && report.unknown_supplies.length > 0 && (
          <p className="sim-warning">
            {t("sim.missingSupplies", { nets: report.unknown_supplies.join(", "), interpolation: { escapeValue: false } })}
          </p>
        )}
        <button type="submit" className="primary" disabled={!canRun}>
          {running ? t("sim.running") : t("sim.run")}
        </button>
        <p className="muted hint">{t("sim.sourcesHint")}</p>
        <p className="review-note">{t("sim.limits")}</p>
      </form>

      <div className="sim-results">
        {failure && (
          <p className="sim-warning" role="alert">
            {failure}
          </p>
        )}
        {preparation && <Report preparation={preparation} />}
        {run ? (
          <>
            <h2>{t("sim.results")}</h2>
            <Results key={run.result.x_unit} run={run} />
            <div className="sim-actions">
              <button type="button" onClick={() => void save(t("fileFilter.csv"), "csv", "simulation.csv", toCsv(run.result))}>
                {t("sim.saveCsv")}
              </button>
            </div>
          </>
        ) : (
          !failure && <p className="muted">{t("sim.noResult")}</p>
        )}
        {notice && <p role="status">{notice}</p>}
        {preparation && (
          <>
            <h3>{t("sim.models")}</h3>
            <table className="sim-table">
              <thead>
                <tr>
                  <th>{t("sim.reference")}</th>
                  <th>{t("sim.model")}</th>
                </tr>
              </thead>
              <tbody>
                {preparation.report.models.map((entry) => (
                  <tr key={entry.reference}>
                    <td>{entry.reference}</td>
                    <td>
                      {t(`sim.modelKinds.${entry.kind}`)}, {entry.detail}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <h3>{t("sim.deck")}</h3>
            <pre className="sim-deck" tabIndex={0}>
              {preparation.deck}
            </pre>
            <div className="sim-actions">
              <button type="button" onClick={() => void save(t("fileFilter.spice"), "cir", "simulation.cir", preparation.deck)}>
                {t("sim.saveDeck")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
