import { FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { fromMm, MM, parseMm, toMm } from "./geometry";
import { BoardView, DesignRules, DrcViolation, PcbCommand } from "./types";

type Tab = "properties" | "drc" | "rules";

const RULE_KEYS: (keyof DesignRules)[] = [
  "min_clearance",
  "min_track_width",
  "track_width",
  "via_drill",
  "via_diameter",
  "min_drill",
  "max_drill",
  "min_annular_ring",
  "min_hole_spacing",
  "edge_clearance",
  "mask_margin",
];

function Properties(props: {
  board: BoardView;
  selection: ReadonlySet<number>;
  layerName: (layer: number) => string;
}) {
  const { t, i18n } = useTranslation();
  const { board, selection } = props;
  const only = selection.size === 1 ? [...selection][0] : undefined;
  const format = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 3 });
  const mm = (nm: number) => t("pcb.mmValue", { value: format.format(nm / MM) });
  const footprint = board.footprints.find((item) => item.id === only);
  const track = board.tracks.find((item) => item.id === only);
  const via = board.vias.find((item) => item.id === only);
  const zone = board.zones.find((item) => item.id === only);
  const net = track?.net ?? via?.net ?? zone?.net;
  return (
    <>
      {selection.size === 0 && <p className="muted">{t("schematic.properties.none")}</p>}
      {selection.size > 1 && <p className="muted">{t("schematic.properties.many", { count: selection.size })}</p>}
      {footprint && (
        <dl className="facts facts-narrow">
          <dt>{t("schematic.properties.reference")}</dt>
          <dd>{footprint.reference}</dd>
          <dt>{t("schematic.properties.value")}</dt>
          <dd>{footprint.value}</dd>
          <dt>{t("schematic.properties.footprint")}</dt>
          <dd>{footprint.footprint}</dd>
          <dt>{t("pcb.properties.position")}</dt>
          <dd>
            {mm(footprint.at.x)} / {mm(footprint.at.y)}
          </dd>
          <dt>{t("pcb.properties.rotation")}</dt>
          <dd>{footprint.rotation}°</dd>
          <dt>{t("pcb.properties.side")}</dt>
          <dd>{footprint.bottom ? t("pcb.layers.bottom") : t("pcb.layers.top")}</dd>
        </dl>
      )}
      {footprint && <p className="review-note">{t("pcb.properties.notReviewed")}</p>}
      {(track || via || zone) && (
        <dl className="facts facts-narrow">
          <dt>{t("schematic.properties.net")}</dt>
          <dd className="selectable">{net}</dd>
          {track && (
            <>
              <dt>{t("pcb.properties.layer")}</dt>
              <dd>{props.layerName(track.layer)}</dd>
              <dt>{t("pcb.properties.width")}</dt>
              <dd>{mm(track.width)}</dd>
              <dt>{t("pcb.properties.length")}</dt>
              <dd>{mm(Math.hypot(track.b.x - track.a.x, track.b.y - track.a.y))}</dd>
            </>
          )}
          {via && (
            <>
              <dt>{t("pcb.properties.drill")}</dt>
              <dd>{mm(via.drill)}</dd>
              <dt>{t("pcb.properties.diameter")}</dt>
              <dd>{mm(via.diameter)}</dd>
            </>
          )}
          {zone && (
            <>
              <dt>{t("pcb.properties.layer")}</dt>
              <dd>{props.layerName(zone.layer)}</dd>
            </>
          )}
          {net && board.net_lengths[net] !== undefined && (
            <>
              <dt>{t("pcb.properties.netLength")}</dt>
              <dd>{mm(board.net_lengths[net] as number)}</dd>
            </>
          )}
        </dl>
      )}
    </>
  );
}

function Rules(props: { board: BoardView; onCommand: (command: PcbCommand) => void }) {
  const { t, i18n } = useTranslation();
  const { board } = props;
  const text = (nm: number) => new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 3, useGrouping: false }).format(toMm(nm));
  const load = () =>
    Object.fromEntries(RULE_KEYS.map((key) => [key, text(board.rules[key])])) as Record<keyof DesignRules, string>;
  const [values, setValues] = useState(load);
  useEffect(() => setValues(load()), [board.rules, i18n.language]);
  const parsed = RULE_KEYS.map((key) => [key, parseMm(values[key])] as const);
  const valid = parsed.every(([, value]) => value !== null);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    const rules = { ...board.rules };
    for (const [key, value] of parsed) rules[key] = value ?? fromMm(0);
    props.onCommand({ type: "set_rules", rules });
  };
  return (
    <>
      <label className="field">
        <span>{t("pcb.rules.layerCount")}</span>
        <select
          value={board.copper_layers}
          onChange={(event) => props.onCommand({ type: "set_layer_count", layers: Number(event.target.value) })}
        >
          {[1, 2, 4, 6, 8].map((count) => (
            <option key={count} value={count}>
              {count}
            </option>
          ))}
        </select>
      </label>
      <form onSubmit={submit}>
        {RULE_KEYS.map((key) => (
          <label className="field" key={key}>
            <span>{t(`pcb.rules.${key}`)}</span>
            <input
              inputMode="decimal"
              value={values[key]}
              aria-invalid={parseMm(values[key]) === null}
              onChange={(event) => setValues({ ...values, [key]: event.target.value })}
            />
          </label>
        ))}
        <button type="submit" className="primary" disabled={!valid}>
          {t("pcb.rules.apply")}
        </button>
      </form>
      <p className="review-note">{t("pcb.rules.note")}</p>
    </>
  );
}

function DrcResults(props: {
  violations: DrcViolation[] | null;
  stale: boolean;
  onRun: () => void;
  onViolation: (violation: DrcViolation) => void;
}) {
  const { t } = useTranslation();
  const { violations } = props;
  if (!violations) {
    return (
      <>
        <p className="muted">{t("pcb.drc.notRun")}</p>
        <button type="button" onClick={props.onRun}>
          {t("pcb.runDrc")}
        </button>
      </>
    );
  }
  const errors = violations.filter((violation) => violation.severity === "error").length;
  const warnings = violations.length - errors;
  return (
    <>
      <p className="erc-summary" role="status" aria-label={t("pcb.drc.summaryLabel")}>
        {violations.length === 0 ? t("schematic.erc.clean") : t("schematic.erc.summary", { errors, warnings })}
      </p>
      {props.stale && <p className="review-note">{t("pcb.drc.stale")}</p>}
      <ul className="erc-list">
        {violations.map((violation, index) => (
          <li key={`${violation.code}-${violation.at.x}-${violation.at.y}-${index}`}>
            <button type="button" className={`erc-item erc-${violation.severity}`} onClick={() => props.onViolation(violation)}>
              <strong>{t(`schematic.erc.severity.${violation.severity}`)}</strong>
              <span>{t(`drc.${violation.code}`, violation.params)}</span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

export function PcbSidePanel(props: {
  tab: Tab;
  onTab: (tab: Tab) => void;
  board: BoardView;
  selection: ReadonlySet<number>;
  violations: DrcViolation[] | null;
  stale: boolean;
  layerName: (layer: number) => string;
  onRunDrc: () => void;
  onViolation: (violation: DrcViolation) => void;
  onCommand: (command: PcbCommand) => void;
}) {
  const { t } = useTranslation();
  const tabs: Tab[] = ["properties", "drc", "rules"];
  return (
    <aside className="side-panel">
      <div className="side-tabs" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab}
            type="button"
            role="tab"
            aria-selected={props.tab === tab}
            className={props.tab === tab ? "side-tab side-tab-active" : "side-tab"}
            onClick={() => props.onTab(tab)}
          >
            {t(`pcb.tabs.${tab}`)}
          </button>
        ))}
      </div>
      <div className="side-content">
        {props.tab === "properties" && (
          <Properties board={props.board} selection={props.selection} layerName={props.layerName} />
        )}
        {props.tab === "drc" && (
          <DrcResults violations={props.violations} stale={props.stale} onRun={props.onRunDrc} onViolation={props.onViolation} />
        )}
        {props.tab === "rules" && <Rules board={props.board} onCommand={props.onCommand} />}
      </div>
    </aside>
  );
}
