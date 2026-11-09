import { useMemo, useState } from "react";
import { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { SymbolDef } from "./types";

// Same order as CATEGORIES in crates/tronic-library/src/builtin.rs.
export const CATEGORY_ORDER = [
  "resistors",
  "capacitors",
  "inductors",
  "diodes",
  "leds",
  "transistors",
  "mosfets",
  "opamps",
  "microcontrollers",
  "processors",
  "regulators",
  "relays",
  "sensors",
  "displays",
  "connectors",
  "logic",
  "supply",
  "simulation",
  "custom",
];

/** Generic parts have translated names, real part types keep theirs. */
export function partName(t: TFunction, symbol: Pick<SymbolDef, "id" | "name">): string {
  return t(`library.parts.${symbol.id.replace(":", "_")}`, { defaultValue: symbol.name });
}

export function LibraryPanel(props: {
  symbols: SymbolDef[];
  activeSymbol: string | null;
  onPick: (symbol: string) => void;
  onNewPart: () => void;
  onImport: () => void;
  onExport: () => void;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const groups = useMemo(() => {
    const term = search.trim().toLowerCase();
    const matching = props.symbols.filter(
      (symbol) =>
        !term ||
        partName(t, symbol).toLowerCase().includes(term) ||
        symbol.name.toLowerCase().includes(term) ||
        symbol.id.toLowerCase().includes(term),
    );
    return CATEGORY_ORDER.map((category) => ({
      category,
      symbols: matching.filter((symbol) => symbol.category === category || (category === "custom" && !CATEGORY_ORDER.includes(symbol.category))),
    })).filter((group) => group.symbols.length > 0);
  }, [props.symbols, search, t]);

  return (
    <aside className="library" aria-label={t("schematic.library")}>
      <h2>{t("schematic.library")}</h2>
      <input
        className="library-search"
        type="search"
        aria-label={t("schematic.searchParts")}
        placeholder={t("schematic.searchParts")}
        value={search}
        onChange={(event) => setSearch(event.target.value)}
      />
      <div className="library-list">
        {groups.map(({ category, symbols }) => (
          <section key={category}>
            <h3>{t(`library.categories.${category}`)}</h3>
            <ul>
              {symbols.map((symbol) => (
                <li key={symbol.id}>
                  <button
                    type="button"
                    className={symbol.id === props.activeSymbol ? "part part-active" : "part"}
                    onClick={() => props.onPick(symbol.id)}
                  >
                    {partName(t, symbol)}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}
        {groups.length === 0 && <p className="muted">{t("schematic.noParts")}</p>}
      </div>
      <div className="library-actions">
        <button type="button" onClick={props.onNewPart}>
          {t("schematic.customPart.title")}
        </button>
        <button type="button" onClick={props.onImport}>
          {t("schematic.importLibrary")}
        </button>
        <button type="button" onClick={props.onExport}>
          {t("schematic.exportLibrary")}
        </button>
      </div>
    </aside>
  );
}
