import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { partName } from "./LibraryPanel";
import { Component, EditCommand, Label, NetHighlight, SchematicView, SymbolDef, Violation } from "./types";

type Tab = "properties" | "erc" | "netlist";

function ComponentProperties(props: {
  component: Component;
  symbol: SymbolDef | undefined;
  sheet: number;
  onCommand: (command: EditCommand) => void;
}) {
  const { t } = useTranslation();
  const { component, symbol, sheet } = props;
  const [reference, setReference] = useState(component.reference);
  const [value, setValue] = useState(component.value);
  const [footprints, setFootprints] = useState<{ id: string; name: string }[]>([]);
  useEffect(() => {
    backend.schematic
      .footprints()
      .then(setFootprints)
      .catch(() => setFootprints([]));
  }, []);
  useEffect(() => {
    setReference(component.reference);
    setValue(component.value);
  }, [component.reference, component.value]);

  const commit = () => {
    if (reference !== component.reference || value !== component.value) {
      props.onCommand({ type: "set_component", sheet, id: component.id, reference, value });
    }
  };
  const missing = t("schematic.properties.notAssigned");
  return (
    <>
      <label className="field">
        <span>{t("schematic.properties.reference")}</span>
        <input
          value={reference}
          onChange={(event) => setReference(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
        />
      </label>
      <label className="field">
        <span>{t("schematic.properties.value")}</span>
        <input
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
        />
      </label>
      {symbol && symbol.kind !== "supply_port" && (
        <label className="field">
          <span>{t("schematic.properties.footprint")}</span>
          <select
            value={component.footprint ?? symbol.part.footprint ?? ""}
            onChange={(event) =>
              props.onCommand({
                type: "set_footprint",
                sheet,
                id: component.id,
                footprint: event.target.value || null,
              })
            }
          >
            <option value="">{missing}</option>
            {footprints.map((footprint) => (
              <option key={footprint.id} value={footprint.id}>
                {footprint.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {symbol && (
        <dl className="facts facts-narrow">
          <dt>{t("schematic.properties.symbol")}</dt>
          <dd>{partName(t, symbol)}</dd>
          <dt>{t("schematic.properties.pins")}</dt>
          <dd>{symbol.pins.length}</dd>
          <dt>{t("schematic.properties.model3d")}</dt>
          <dd>{symbol.part.model_3d ?? missing}</dd>
          <dt>{t("schematic.properties.manufacturer")}</dt>
          <dd>{symbol.part.manufacturer ?? missing}</dd>
          <dt>{t("schematic.properties.partNumber")}</dt>
          <dd>{symbol.part.part_number ?? missing}</dd>
          <dt>{t("schematic.properties.datasheet")}</dt>
          <dd>{symbol.part.datasheet ?? missing}</dd>
          <dt>{t("schematic.properties.source")}</dt>
          <dd>{symbol.provenance.source}</dd>
          <dt>{t("schematic.properties.license")}</dt>
          <dd>{symbol.provenance.license}</dd>
        </dl>
      )}
      {symbol && !symbol.provenance.reviewed && (
        <p className="review-note">{t("schematic.properties.notReviewed")}</p>
      )}
    </>
  );
}

function LabelProperties(props: { label: Label; sheet: number; onCommand: (command: EditCommand) => void }) {
  const { t } = useTranslation();
  const { label, sheet } = props;
  const [name, setName] = useState(label.name);
  useEffect(() => setName(label.name), [label.name]);
  const commit = () => {
    if (name !== label.name) props.onCommand({ type: "set_label", sheet, id: label.id, name });
  };
  return (
    <>
      <label className="field">
        <span>{t("schematic.labelName")}</span>
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
        />
      </label>
      <p className="muted">{t(`schematic.labelKinds.${label.kind}`)}</p>
    </>
  );
}

function Properties(props: {
  view: SchematicView;
  selection: ReadonlySet<number>;
  highlight: NetHighlight | null;
  onCommand: (command: EditCommand) => void;
}) {
  const { t } = useTranslation();
  const { view, selection } = props;
  const only = selection.size === 1 ? [...selection][0] : undefined;
  const component = view.sheet.components.find((item) => item.id === only);
  const label = view.sheet.labels.find((item) => item.id === only);
  return (
    <>
      {selection.size === 0 && <p className="muted">{t("schematic.properties.none")}</p>}
      {selection.size > 1 && <p className="muted">{t("schematic.properties.many", { count: selection.size })}</p>}
      {component && (
        <ComponentProperties
          key={component.id}
          component={component}
          symbol={view.symbols[component.symbol]}
          sheet={view.sheet.id}
          onCommand={props.onCommand}
        />
      )}
      {label && <LabelProperties key={label.id} label={label} sheet={view.sheet.id} onCommand={props.onCommand} />}
      {props.highlight && (
        <p className="net-name">
          {t("schematic.properties.net")}: <strong className="selectable">{props.highlight.name}</strong>
        </p>
      )}
    </>
  );
}

function ErcResults(props: {
  violations: Violation[] | null;
  stale: boolean;
  sheets: SchematicView["sheets"];
  onRun: () => void;
  onViolation: (violation: Violation) => void;
}) {
  const { t } = useTranslation();
  const { violations } = props;
  if (!violations) {
    return (
      <>
        <p className="muted">{t("schematic.erc.notRun")}</p>
        <button type="button" onClick={props.onRun}>
          {t("schematic.runErc")}
        </button>
      </>
    );
  }
  const errors = violations.filter((violation) => violation.severity === "error").length;
  const warnings = violations.length - errors;
  return (
    <>
      <p className="erc-summary" role="status" aria-label={t("schematic.erc.summaryLabel")}>
        {violations.length === 0
          ? t("schematic.erc.clean")
          : t("schematic.erc.summary", { errors, warnings })}
      </p>
      {props.stale && <p className="review-note">{t("schematic.erc.stale")}</p>}
      <ul className="erc-list">
        {violations.map((violation, index) => (
          <li key={`${violation.code}-${violation.at.x}-${violation.at.y}-${index}`}>
            <button
              type="button"
              className={`erc-item erc-${violation.severity}`}
              onClick={() => props.onViolation(violation)}
            >
              <strong>{t(`schematic.erc.severity.${violation.severity}`)}</strong>
              <span>{t(`erc.${violation.code}`, violation.params)}</span>
              <small>{props.sheets.find((sheet) => sheet.id === violation.sheet)?.name}</small>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

function NetlistView(props: { revision: number; onError: (error: unknown) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState<string>("");
  const { onError } = props;
  useEffect(() => {
    backend.schematic
      .netlist()
      .then((result) => setText(result.text))
      .catch(onError);
  }, [props.revision, onError]);
  const exportFile = async () => {
    try {
      const path = await backend.pickSaveFile(t("fileFilter.netlist"), "net");
      if (path) await backend.schematic.exportNetlist(path);
    } catch (error) {
      onError(error);
    }
  };
  return (
    <>
      <button type="button" onClick={() => void exportFile()}>
        {t("schematic.exportNetlist")}
      </button>
      <pre className="netlist selectable" aria-label={t("schematic.tabs.netlist")}>
        {text}
      </pre>
    </>
  );
}

export function SidePanel(props: {
  tab: Tab;
  onTab: (tab: Tab) => void;
  view: SchematicView;
  selection: ReadonlySet<number>;
  highlight: NetHighlight | null;
  violations: Violation[] | null;
  ercStale: boolean;
  netlistRevision: number;
  onRunErc: () => void;
  onViolation: (violation: Violation) => void;
  onCommand: (command: EditCommand) => void;
  onError: (error: unknown) => void;
}) {
  const { t } = useTranslation();
  const tabs: Tab[] = ["properties", "erc", "netlist"];
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
            {t(`schematic.tabs.${tab}`)}
          </button>
        ))}
      </div>
      <div className="side-content">
        {props.tab === "properties" && (
          <Properties view={props.view} selection={props.selection} highlight={props.highlight} onCommand={props.onCommand} />
        )}
        {props.tab === "erc" && (
          <ErcResults
            violations={props.violations}
            stale={props.ercStale}
            sheets={props.view.sheets}
            onRun={props.onRunErc}
            onViolation={props.onViolation}
          />
        )}
        {props.tab === "netlist" && <NetlistView revision={props.netlistRevision} onError={props.onError} />}
      </div>
    </aside>
  );
}
