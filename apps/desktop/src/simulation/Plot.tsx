import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatSi, linearTicks, logTicks, nearestIndex } from "./format";

export const TRACE_COLORS = ["#2f9e8f", "#d9534f", "#4a90e2", "#d4a02a", "#a05fd0", "#3fae5a", "#e0713c", "#7a8a94"];

export interface PlotTrace {
  name: string;
  color: string;
  values: number[];
}

const MARGIN = { left: 70, right: 16, top: 10, bottom: 28 };

function themed(element: Element) {
  const style = getComputedStyle(element);
  return {
    text: style.getPropertyValue("color") || "#888",
    line: style.getPropertyValue("--line").trim() || "#888",
  };
}

/** One diagram: all traces share the x axis and one unit. */
export function Plot(props: { x: number[]; xUnit: string; log: boolean; unit: string; traces: PlotTrace[] }) {
  const { t, i18n } = useTranslation();
  const { x, xUnit, log, unit, traces } = props;
  const canvas = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const [resized, setResized] = useState(0);

  const xMin = x[0] ?? 0;
  const xMax = x[x.length - 1] ?? 1;
  const toAxis = (value: number) => (log ? Math.log10(Math.max(value, 1e-30)) : value);

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setResized((count) => count + 1));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = canvas.current;
    const ctx = element?.getContext("2d");
    if (!element || !ctx) return;
    const ratio = window.devicePixelRatio || 1;
    const width = element.clientWidth;
    const height = element.clientHeight;
    element.width = Math.round(width * ratio);
    element.height = Math.round(height * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const colors = themed(element);
    ctx.font = "11px sans-serif";

    let yMin = Infinity;
    let yMax = -Infinity;
    for (const trace of traces) {
      for (const value of trace.values) {
        if (Number.isFinite(value)) {
          yMin = Math.min(yMin, value);
          yMax = Math.max(yMax, value);
        }
      }
    }
    if (!Number.isFinite(yMin)) return;
    if (yMax - yMin < Math.max(Math.abs(yMax), 1e-12) * 1e-9) {
      const pad = Math.abs(yMax) * 0.1 || 1;
      yMin -= pad;
      yMax += pad;
    } else {
      const pad = (yMax - yMin) * 0.05;
      yMin -= pad;
      yMax += pad;
    }
    const plotWidth = width - MARGIN.left - MARGIN.right;
    const plotHeight = height - MARGIN.top - MARGIN.bottom;
    const axisMin = toAxis(xMin);
    const axisMax = toAxis(xMax);
    const px = (value: number) =>
      MARGIN.left + (axisMax > axisMin ? ((toAxis(value) - axisMin) / (axisMax - axisMin)) * plotWidth : plotWidth / 2);
    const py = (value: number) => MARGIN.top + (1 - (value - yMin) / (yMax - yMin)) * plotHeight;

    ctx.lineWidth = 1;
    ctx.strokeStyle = colors.line;
    ctx.fillStyle = colors.text;
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    for (const tick of linearTicks(yMin, yMax)) {
      const y = Math.round(py(tick)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(MARGIN.left, y);
      ctx.lineTo(width - MARGIN.right, y);
      ctx.stroke();
      ctx.fillText(formatSi(tick, unit, i18n.language, 3), MARGIN.left - 6, y);
    }
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const xTicks = log ? logTicks(xMin, xMax) : linearTicks(xMin, xMax);
    for (const tick of xTicks) {
      const xp = Math.round(px(tick)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(xp, MARGIN.top);
      ctx.lineTo(xp, height - MARGIN.bottom);
      ctx.stroke();
      ctx.fillText(formatSi(tick, xUnit, i18n.language, 3), xp, height - MARGIN.bottom + 6);
    }

    ctx.save();
    ctx.beginPath();
    ctx.rect(MARGIN.left, MARGIN.top, plotWidth, plotHeight);
    ctx.clip();
    ctx.lineWidth = 1.5;
    for (const trace of traces) {
      ctx.strokeStyle = trace.color;
      ctx.beginPath();
      let drawing = false;
      trace.values.forEach((value, index) => {
        if (!Number.isFinite(value)) {
          drawing = false;
          return;
        }
        const xp = px(x[index] as number);
        const yp = py(value);
        if (drawing) ctx.lineTo(xp, yp);
        else ctx.moveTo(xp, yp);
        drawing = true;
      });
      ctx.stroke();
    }
    if (hover !== null && x[hover] !== undefined) {
      const xp = Math.round(px(x[hover] as number)) + 0.5;
      ctx.strokeStyle = colors.text;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(xp, MARGIN.top);
      ctx.lineTo(xp, height - MARGIN.bottom);
      ctx.stroke();
    }
    ctx.restore();
  }, [x, xUnit, log, unit, traces, hover, resized, i18n.language]);

  const onMove = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const element = event.currentTarget;
    const rect = element.getBoundingClientRect();
    const plotWidth = rect.width - MARGIN.left - MARGIN.right;
    if (plotWidth <= 0 || x.length === 0) return;
    const share = Math.min(1, Math.max(0, (event.clientX - rect.left - MARGIN.left) / plotWidth));
    const axis = toAxis(xMin) + share * (toAxis(xMax) - toAxis(xMin));
    setHover(nearestIndex(x, log ? 10 ** axis : axis));
  };

  const readout =
    hover !== null && x[hover] !== undefined
      ? t("sim.readout", {
          x: formatSi(x[hover] as number, xUnit, i18n.language),
          values: traces
            .map((trace) => `${trace.name} = ${formatSi(trace.values[hover] ?? Number.NaN, unit, i18n.language)}`)
            .join(", "),
          interpolation: { escapeValue: false },
        })
      : " ";

  return (
    <>
      <canvas
        ref={canvas}
        className="sim-plot"
        role="img"
        aria-label={t("sim.plot", { unit })}
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
      />
      <div className="sim-readout muted">{readout}</div>
    </>
  );
}
