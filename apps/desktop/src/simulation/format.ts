// Number handling of simulation results. Pure functions, tested separately.
import { SimResult } from "./types";

const PREFIXES: [number, string][] = [
  [1e9, "G"],
  [1e6, "M"],
  [1e3, "k"],
  [1, ""],
  [1e-3, "m"],
  [1e-6, "µ"],
  [1e-9, "n"],
  [1e-12, "p"],
  [1e-15, "f"],
];

/** Formats with an SI prefix: 0.0047 and "A" give "4.7 mA". */
export function formatSi(value: number, unit: string, language: string, digits = 4): string {
  if (!Number.isFinite(value)) return `${value} ${unit}`.trim();
  const size = Math.abs(value);
  // Numerically zero next to volts and amperes of normal size.
  const [factor, prefix] = size < 1e-15 ? [1, ""] : (PREFIXES.find(([f]) => size >= f * 0.9999999) ?? [1e-15, "f"]);
  const text = new Intl.NumberFormat(language, { maximumSignificantDigits: digits }).format(size < 1e-15 ? 0 : value / factor);
  return `${text} ${prefix}${unit}`.trim();
}

/** Results as a table, with a dot as the decimal mark, so that any program reads it. */
export function toCsv(result: SimResult): string {
  const header = [`x [${result.x_unit}]`, ...result.traces.map((trace) => `${trace.name} [${trace.unit}]`)];
  const quote = (text: string) => (/[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text);
  const rows = result.x.map((x, index) =>
    [x, ...result.traces.map((trace) => trace.values[index] ?? "")].join(","),
  );
  return [header.map(quote).join(","), ...rows].join("\n") + "\n";
}

/** Round axis marks covering a range: 0, 0.5, 1 … */
export function linearTicks(min: number, max: number, count = 6): number[] {
  if (!(max > min)) return [min];
  const rough = (max - min) / count;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = ([1, 2, 5, 10].find((m) => m * magnitude >= rough) ?? 10) * magnitude;
  const ticks: number[] = [];
  for (let i = Math.ceil(min / step); i * step <= max + step * 1e-9; i++) {
    ticks.push(Math.abs(i) < 1e-12 ? 0 : i * step);
  }
  return ticks;
}

/** Decades covering a range, for a logarithmic axis. */
export function logTicks(min: number, max: number): number[] {
  const ticks: number[] = [];
  for (let e = Math.ceil(Math.log10(min) - 1e-9); e <= Math.floor(Math.log10(max) + 1e-9); e++) {
    ticks.push(10 ** e);
  }
  return ticks;
}

/** Index of the sample closest to `x` in a rising list. */
export function nearestIndex(values: number[], x: number): number {
  let low = 0;
  let high = values.length - 1;
  while (high - low > 1) {
    const middle = (low + high) >> 1;
    if ((values[middle] as number) < x) low = middle;
    else high = middle;
  }
  return Math.abs((values[low] as number) - x) <= Math.abs((values[high] as number) - x) ? low : high;
}

/** Magnitude to decibels, with a floor so that a true zero stays drawable. */
export const toDb = (value: number): number => 20 * Math.log10(Math.max(Math.abs(value), 1e-15));
