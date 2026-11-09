import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { parseMm, toMm } from "../pcb/geometry";
import { BoardView, Mechanics } from "../pcb/types";
import { boundsOf, Camera, eyePosition, fitted, panned, Preset, PRESETS, radiusOf, rotated, viewProjection, zoomed } from "./camera";
import { Renderer } from "./renderer";
import { faceRect, paintFace } from "./texture";
import { Scene, ScenePart } from "./types";

type Tab = "check" | "parts" | "rules";
const TABS: Tab[] = ["check", "parts", "rules"];

function CheckTab(props: { scene: Scene; onPick: (id: number) => void }) {
  const { t, i18n } = useTranslation();
  const { mechanics } = props.scene;
  const number = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 });
  return (
    <>
      <p className="erc-summary" role="status" aria-label={t("view3d.tabs.check")}>
        {mechanics.findings.length === 0 ? t("view3d.clean") : t("view3d.summary", { count: mechanics.findings.length })}
      </p>
      <p className="muted">{t("view3d.tallestTop", { value: number.format(mechanics.max_top) })}</p>
      <p className="muted">{t("view3d.tallestBottom", { value: number.format(mechanics.max_bottom) })}</p>
      <ul className="erc-list">
        {mechanics.findings.map((finding, index) => (
          <li key={`${finding.code}-${index}`}>
            <button
              type="button"
              className={`erc-item ${finding.code === "no_model" ? "erc-warning" : "erc-error"}`}
              onClick={() => finding.elements[0] !== undefined && props.onPick(finding.elements[0])}
            >
              <span>{t(`view3d.findings.${finding.code}`, { ...finding.params, interpolation: { escapeValue: false } })}</span>
            </button>
          </li>
        ))}
      </ul>
      <p className="review-note">{t("view3d.checkNote")}</p>
    </>
  );
}

function PartsTab(props: {
  scene: Scene;
  selected: number | null;
  onPick: (id: number) => void;
  onImport: (part: ScenePart, scale: number, zOffset: number) => void;
  onRemove: (part: ScenePart) => void;
}) {
  const { t, i18n } = useTranslation();
  const { scene, selected } = props;
  const [scale, setScale] = useState("1");
  const [zOffset, setZOffset] = useState("0");
  const number = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 });
  const read = (text: string) => {
    const value = Number(text.trim().replace(",", "."));
    return text.trim() !== "" && Number.isFinite(value) ? value : null;
  };
  const part = scene.parts.find((item) => item.id === selected);
  const scaleValue = read(scale);
  const offsetValue = read(zOffset);
  const valid = scaleValue !== null && scaleValue > 0 && offsetValue !== null;
  return (
    <>
      <table className="sim-table view3d-parts">
        <thead>
          <tr>
            <th>{t("view3d.part")}</th>
            <th>{t("view3d.side")}</th>
            <th>{t("view3d.height")}</th>
            <th>{t("view3d.modelCol")}</th>
          </tr>
        </thead>
        <tbody>
          {scene.mechanics.heights.map((row) => {
            const scenePart = scene.parts.find((item) => item.id === row.id);
            return (
              <tr key={row.id} aria-selected={row.id === selected} onClick={() => props.onPick(row.id)}>
                <td>{row.reference}</td>
                <td>{row.bottom ? t("view3d.bottom") : t("view3d.top")}</td>
                <td>{number.format(row.height)}</td>
                <td>{scenePart?.model_name ?? (row.has_model ? t("view3d.builtin") : t("view3d.none"))}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {part ? (
        <div>
          <p className="muted">{t("view3d.footprint", { id: part.footprint, interpolation: { escapeValue: false } })}</p>
          <p className="muted">{t("view3d.footprintNote")}</p>
          <label className="field">
            <span>{t("view3d.scale")}</span>
            <input value={scale} aria-invalid={scaleValue === null || scaleValue <= 0} onChange={(event) => setScale(event.target.value)} />
          </label>
          <label className="field">
            <span>{t("view3d.zOffset")}</span>
            <input value={zOffset} aria-invalid={offsetValue === null} onChange={(event) => setZOffset(event.target.value)} />
          </label>
          <div className="sim-actions">
            <button type="button" disabled={!valid} onClick={() => props.onImport(part, scaleValue as number, offsetValue as number)}>
              {t("view3d.importModel")}
            </button>
            {part.model_name && (
              <button type="button" onClick={() => props.onRemove(part)}>
                {t("view3d.removeModel")}
              </button>
            )}
          </div>
        </div>
      ) : (
        <p className="muted">{t("view3d.selectHint")}</p>
      )}
      <p className="review-note">{t("view3d.importNote")}</p>
      <p className="review-note">{t("view3d.bodiesNote")}</p>
    </>
  );
}

function RulesTab(props: { mechanics: Mechanics; onApply: (mechanics: Mechanics) => void }) {
  const { t, i18n } = useTranslation();
  const { mechanics } = props;
  const text = useCallback(
    (nm: number | null) =>
      nm === null ? "" : new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 3, useGrouping: false }).format(toMm(nm)),
    [i18n.language],
  );
  const load = useCallback(
    () => ({
      thickness: text(mechanics.thickness),
      clearance: text(mechanics.part_clearance),
      top: text(mechanics.max_height_top),
      bottom: text(mechanics.max_height_bottom),
    }),
    [mechanics, text],
  );
  const [values, setValues] = useState(load);
  useEffect(() => setValues(load()), [load]);
  const thickness = parseMm(values.thickness);
  const clearance = parseMm(values.clearance);
  const optional = (value: string) => (value.trim() === "" ? { ok: true, nm: null } : { ok: parseMm(value) !== null, nm: parseMm(value) });
  const top = optional(values.top);
  const bottom = optional(values.bottom);
  const valid = thickness !== null && thickness > 0 && clearance !== null && top.ok && bottom.ok;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || thickness === null || clearance === null) return;
    props.onApply({
      thickness,
      part_clearance: clearance,
      max_height_top: top.nm,
      max_height_bottom: bottom.nm,
    });
  };
  return (
    <form onSubmit={submit}>
      <label className="field">
        <span>{t("view3d.thickness")}</span>
        <input value={values.thickness} aria-invalid={thickness === null} onChange={(event) => setValues({ ...values, thickness: event.target.value })} />
      </label>
      <label className="field">
        <span>{t("view3d.clearance")}</span>
        <input value={values.clearance} aria-invalid={clearance === null} onChange={(event) => setValues({ ...values, clearance: event.target.value })} />
      </label>
      <label className="field">
        <span>{t("view3d.maxTop")}</span>
        <input value={values.top} placeholder={t("view3d.noLimit")} aria-invalid={!top.ok} onChange={(event) => setValues({ ...values, top: event.target.value })} />
      </label>
      <label className="field">
        <span>{t("view3d.maxBottom")}</span>
        <input value={values.bottom} placeholder={t("view3d.noLimit")} aria-invalid={!bottom.ok} onChange={(event) => setValues({ ...values, bottom: event.target.value })} />
      </label>
      <button type="submit" className="primary" disabled={!valid}>
        {t("view3d.apply")}
      </button>
    </form>
  );
}

export function View3D(props: { onDirty: (dirty: boolean) => void; onError: (error: unknown) => void }) {
  const { t } = useTranslation();
  const { onDirty, onError } = props;
  const canvas = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<Renderer | null>(null);
  const [scene, setScene] = useState<Scene | null>(null);
  const [board, setBoard] = useState<BoardView | null>(null);
  const [camera, setCamera] = useState<Camera | null>(null);
  const [showParts, setShowParts] = useState(true);
  const [selected, setSelected] = useState<number | null>(null);
  const [tab, setTab] = useState<Tab>("check");
  const [graphics, setGraphics] = useState<"ok" | "unavailable" | "lost">("ok");
  // Counts created renderers: everything sent to the graphics card is sent again.
  const [generation, setGeneration] = useState(0);
  const [size, setSize] = useState({ width: 1, height: 1 });

  const bounds = useMemo(
    () => (scene ? boundsOf([scene.board.positions, ...scene.parts.flatMap((part) => part.meshes.map((item) => item.mesh.positions))]) : null),
    [scene],
  );
  const radius = bounds ? radiusOf(bounds) : 1;
  const aspect = size.width / Math.max(size.height, 1);

  const load = useCallback(async () => {
    const [nextScene, view] = await Promise.all([backend.view3d.scene(), backend.pcb.view()]);
    setScene(nextScene);
    setBoard(view.board);
    onDirty(view.dirty);
  }, [onDirty]);

  useEffect(() => {
    load().catch(onError);
  }, [load, onError]);

  // The graphics context, also after the system took it away.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const create = () => {
      const gl = element.getContext("webgl2", { antialias: true, preserveDrawingBuffer: true });
      if (!gl) {
        setGraphics("unavailable");
        return;
      }
      try {
        renderer.current = new Renderer(gl);
        setGraphics("ok");
        setGeneration((count) => count + 1);
      } catch {
        setGraphics("unavailable");
      }
    };
    const onLost = (event: Event) => {
      event.preventDefault();
      renderer.current = null;
      setGraphics("lost");
    };
    element.addEventListener("webglcontextlost", onLost);
    element.addEventListener("webglcontextrestored", create);
    create();
    return () => {
      element.removeEventListener("webglcontextlost", onLost);
      element.removeEventListener("webglcontextrestored", create);
      renderer.current = null;
    };
  }, []);

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      const ratio = window.devicePixelRatio || 1;
      const width = Math.max(Math.round(element.clientWidth * ratio), 1);
      const height = Math.max(Math.round(element.clientHeight * ratio), 1);
      element.width = width;
      element.height = height;
      setSize({ width, height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // A first camera once the scene and the size are known.
  const hasSize = size.width > 1;
  useEffect(() => {
    if (bounds && hasSize) setCamera((current) => current ?? fitted(bounds, aspect, "iso", true));
  }, [bounds, hasSize, aspect]);

  useEffect(() => {
    if (scene) renderer.current?.setScene(scene);
  }, [scene, generation]);

  useEffect(() => {
    if (!board || !renderer.current) return;
    const rect = faceRect(board);
    if (rect) renderer.current.setFaces(rect, paintFace(board, rect, false), paintFace(board, rect, true));
    else renderer.current.setFaces(null, null, null);
  }, [board, generation]);

  const colliding = useMemo(
    () =>
      new Set(
        (scene?.mechanics.findings ?? [])
          .filter((finding) => finding.code === "collision" || finding.code === "too_close")
          .flatMap((finding) => finding.elements),
      ),
    [scene],
  );

  useEffect(() => {
    if (!camera || !renderer.current) return;
    renderer.current.setHighlights(selected, colliding);
    renderer.current.render(viewProjection(camera, aspect, radius), eyePosition(camera), showParts);
  }, [camera, aspect, radius, showParts, selected, colliding, scene, board, size, generation]);

  const preset = (name: Preset) => {
    if (!bounds) return;
    setCamera((current) => ({ ...fitted(bounds, aspect, name, current?.perspective ?? true), ...PRESETS[name] }));
  };

  const fit = () => {
    if (!bounds || !camera) return;
    const whole = fitted(bounds, aspect, "iso", camera.perspective);
    setCamera({ ...camera, target: whole.target, distance: whole.distance });
  };

  // Pointer and keyboard.
  const drag = useRef<{ x: number; y: number; pan: boolean } | null>(null);
  const onPointerDown = (event: React.PointerEvent<HTMLCanvasElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    event.currentTarget.focus();
    drag.current = { x: event.clientX, y: event.clientY, pan: event.button !== 0 || event.shiftKey };
  };
  const onPointerMove = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const state = drag.current;
    if (!state) return;
    const dx = event.clientX - state.x;
    const dy = event.clientY - state.y;
    drag.current = { ...state, x: event.clientX, y: event.clientY };
    const height = event.currentTarget.clientHeight;
    setCamera((current) => current && (state.pan ? panned(current, dx, dy, height) : rotated(current, dx, dy)));
  };
  // React attaches wheel listeners passively; this one has to stop the page scrolling.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      setCamera((current) => current && zoomed(current, Math.exp(event.deltaY * 0.001), radius));
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [radius]);
  const onKeyDown = (event: React.KeyboardEvent) => {
    const steps: Record<string, [number, number]> = {
      ArrowLeft: [-25, 0],
      ArrowRight: [25, 0],
      ArrowUp: [0, 25],
      ArrowDown: [0, -25],
    };
    const step = steps[event.key];
    if (step) {
      event.preventDefault();
      setCamera((current) => current && rotated(current, step[0], step[1]));
    } else if (event.key === "+" || event.key === "=") {
      setCamera((current) => current && zoomed(current, 0.85, radius));
    } else if (event.key === "-") {
      setCamera((current) => current && zoomed(current, 1 / 0.85, radius));
    }
  };

  const importModel = async (part: ScenePart, scale: number, zOffset: number) => {
    try {
      const path = await backend.pickModelFile(t("fileFilter.model"));
      if (!path) return;
      setScene(await backend.view3d.importModel(part.footprint, path, scale, zOffset));
      onDirty(true);
    } catch (error) {
      onError(error);
    }
  };

  const removeModel = async (part: ScenePart) => {
    try {
      await backend.pcb.apply({ type: "set_footprint_model", footprint: part.footprint, model: null });
      await load();
    } catch (error) {
      onError(error);
    }
  };

  const applyMechanics = async (next: Mechanics) => {
    try {
      await backend.pcb.apply({ type: "set_mechanics", mechanics: next });
      await load();
    } catch (error) {
      onError(error);
    }
  };

  return (
    <div className="editor">
      <aside className="library" aria-label={t("view3d.view")}>
        <h2>{t("view3d.view")}</h2>
        <div className="view3d-buttons">
          {(Object.keys(PRESETS) as Preset[]).map((name) => (
            <button type="button" key={name} onClick={() => preset(name)}>
              {t(`view3d.presets.${name}`)}
            </button>
          ))}
          <button type="button" onClick={fit}>
            {t("view3d.fit")}
          </button>
        </div>
        <label className="sim-traces">
          <input
            type="checkbox"
            checked={camera?.perspective ?? true}
            onChange={(event) => setCamera((current) => current && { ...current, perspective: event.target.checked })}
          />
          {t("view3d.perspective")}
        </label>
        <label className="sim-traces">
          <input type="checkbox" checked={showParts} onChange={(event) => setShowParts(event.target.checked)} />
          {t("view3d.parts")}
        </label>
        {scene && scene.parts.length === 0 && <p className="muted hint">{t("view3d.empty")}</p>}
      </aside>
      <div className="editor-main">
        {graphics !== "ok" && (
          <p className="view3d-message sim-warning" role="alert">
            {t(graphics === "lost" ? "view3d.lost" : "view3d.unavailable")}
          </p>
        )}
        <canvas
          ref={canvas}
          className="view3d-canvas"
          hidden={graphics !== "ok"}
          tabIndex={0}
          role="img"
          aria-label={t("view3d.canvas")}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={() => (drag.current = null)}
          onContextMenu={(event) => event.preventDefault()}
          onKeyDown={onKeyDown}
        />
      </div>
      <aside className="side-panel">
        <div className="side-tabs" role="tablist">
          {TABS.map((item) => (
            <button
              key={item}
              type="button"
              role="tab"
              aria-selected={tab === item}
              className={tab === item ? "side-tab side-tab-active" : "side-tab"}
              onClick={() => setTab(item)}
            >
              {t(`view3d.tabs.${item}`)}
            </button>
          ))}
        </div>
        <div className="side-content">
          {scene && tab === "check" && <CheckTab scene={scene} onPick={setSelected} />}
          {scene && tab === "parts" && (
            <PartsTab
              scene={scene}
              selected={selected}
              onPick={setSelected}
              onImport={(part, scale, zOffset) => void importModel(part, scale, zOffset)}
              onRemove={(part) => void removeModel(part)}
            />
          )}
          {scene && tab === "rules" && <RulesTab mechanics={scene.rules} onApply={(next) => void applyMechanics(next)} />}
        </div>
      </aside>
    </div>
  );
}
