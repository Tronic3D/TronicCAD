import { PointerEvent, ReactNode, useCallback, useEffect, useRef, useState, WheelEvent } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { icons } from "../components/icons";
import {
  copperAt,
  elementsInBox,
  fitBoard,
  hitElement,
  MM,
  PcbCamera,
  route45,
  screenToBoard,
  snapTo,
  zoomCamera,
} from "./geometry";
import { ExportDialog, RouterProgressDialog, SyncDialog, ZoneDialog } from "./PcbDialogs";
import { PcbSidePanel } from "./PcbSidePanel";
import { drawBoard, layerColor, PcbOverlay } from "./render";
import { DrcViolation, PcbCommand, PcbView, Point, RouteResult, SyncReport } from "./types";

type Tool = "select" | "route" | "via" | "outline" | "zone";

type Drag =
  | { kind: "pan"; sx: number; sy: number; ox: number; oy: number }
  | { kind: "move"; start: Point; moved: boolean }
  | { kind: "box"; start: Point; additive: boolean };

const GRIDS = [100_000, 250_000, 500_000, 1_000_000, 1_270_000];

function ToolButton(props: {
  label: string;
  icon: ReactNode;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`icon-tool${props.active ? " icon-tool-active" : ""}`}
      aria-label={props.label}
      aria-pressed={props.active}
      title={props.label}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.icon}
    </button>
  );
}

export function PcbEditor(props: {
  active: boolean;
  onDirty: (dirty: boolean) => void;
  onError: (error: unknown) => void;
}) {
  const { active, onDirty, onError } = props;
  const { t, i18n } = useTranslation();
  const [view, setView] = useState<PcbView | null>(null);
  const [tool, setToolState] = useState<Tool>("select");
  const [activeLayer, setActiveLayer] = useState(0);
  const [hidden, setHidden] = useState<ReadonlySet<number>>(new Set());
  const [grid, setGrid] = useState(500_000);
  const [selection, setSelectionState] = useState<ReadonlySet<number>>(new Set());
  const [violations, setViolations] = useState<DrcViolation[] | null>(null);
  const [drcStale, setDrcStale] = useState(false);
  const [sideTab, setSideTab] = useState<"properties" | "drc" | "rules">("properties");
  const [syncReport, setSyncReport] = useState<SyncReport | null>(null);
  const [zonePoints, setZonePoints] = useState<Point[] | null>(null);
  const [exporting, setExporting] = useState<"ask" | string[] | null>(null);
  const [routerProgress, setRouterProgress] = useState<{ done: number; total: number } | null>(null);
  const [proposal, setProposal] = useState<RouteResult | null>(null);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const size = useRef({ width: 800, height: 600 });
  const camera = useRef<PcbCamera>({ ox: 400, oy: 300, scale: 10 });
  const fitted = useRef(false);
  const sized = useRef(false);
  const drag = useRef<Drag | null>(null);
  const moveDelta = useRef<Point | null>(null);
  const box = useRef<{ min: Point; max: Point } | null>(null);
  const cursor = useRef<Point | null>(null);
  const rawCursor = useRef<Point | null>(null);
  // Track being routed: placed segments per layer change are sent at once.
  const routing = useRef<{ net: string; layer: number; points: Point[] } | null>(null);
  const polygon = useRef<Point[]>([]);
  const frame = useRef<number | null>(null);
  const routerRunning = useRef(false);

  const visible = (count: number) =>
    new Set([...Array(count).keys()].filter((layer) => !hidden.has(layer)));

  const live = useRef({ view, tool, selection, violations, activeLayer, hidden, grid, proposal });
  live.current = { view, tool, selection, violations, activeLayer, hidden, grid, proposal };

  const draw = useCallback(() => {
    frame.current = null;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d", { alpha: false });
    const state = live.current;
    if (!ctx || !state.view) return;
    const board = state.view.board;
    const route = routing.current;
    const last = route?.points[route.points.length - 1];
    const overlay: PcbOverlay = {
      selection: state.selection,
      visible: new Set(
        [...Array(board.copper_layers).keys()].filter((layer) => !state.hidden.has(layer)),
      ),
      activeLayer: state.activeLayer,
      violations: state.violations ?? [],
      moveDelta: moveDelta.current,
      routing:
        route && last
          ? {
              layer: route.layer,
              width: board.rules.track_width,
              points: [...route.points, ...(cursor.current ? route45(last, cursor.current) : [])],
            }
          : null,
      polygon: polygon.current.length
        ? [...polygon.current, ...(cursor.current ? [cursor.current] : [])]
        : null,
      box: box.current,
      cursor: state.tool !== "select" ? cursor.current : null,
      proposal: state.proposal,
    };
    drawBoard(ctx, size.current.width, size.current.height, window.devicePixelRatio || 1, camera.current, board, overlay);
  }, []);

  const requestDraw = useCallback(() => {
    if (frame.current === null) frame.current = requestAnimationFrame(draw);
  }, [draw]);

  useEffect(requestDraw, [view, tool, selection, violations, activeLayer, hidden, proposal, requestDraw]);

  useEffect(() => {
    // Events can arrive after the router has returned; those are ignored.
    const pending = backend.pcb.onRouteProgress((done, total) => {
      if (routerRunning.current) setRouterProgress({ done, total });
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const adopt = useCallback(
    (next: PcbView) => {
      setView(next);
      onDirty(next.dirty);
    },
    [onDirty],
  );

  const fit = useCallback(() => {
    const current = live.current.view;
    if (!current) return;
    camera.current = fitBoard(current.board, size.current.width, size.current.height);
    requestDraw();
  }, [requestDraw]);

  const fitOnce = useCallback(() => {
    if (!fitted.current && live.current.view && sized.current) {
      fitted.current = true;
      fit();
    }
  }, [fit]);

  useEffect(() => {
    backend.pcb.view().then(setView).catch(onError);
  }, [onError]);

  useEffect(fitOnce, [view, fitOnce]);

  const hasView = view !== null;
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const ratio = window.devicePixelRatio || 1;
      const { clientWidth: width, clientHeight: height } = canvas;
      size.current = { width, height };
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round(height * ratio);
      sized.current = true;
      fitOnce();
      draw();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [draw, fitOnce, hasView]);

  const setSelection = useCallback((ids: Iterable<number>) => {
    const next = new Set(ids);
    live.current.selection = next;
    setSelectionState(next);
  }, []);

  const run = useCallback(
    async (command: PcbCommand) => {
      try {
        const result = await backend.pcb.apply(command);
        adopt(result.view);
        setDrcStale(true);
        return result;
      } catch (error) {
        onError(error);
        return null;
      }
    },
    [adopt, onError],
  );

  const setTool = useCallback(
    (next: Tool) => {
      routing.current = null;
      polygon.current = [];
      setToolState(next);
      setSelection([]);
    },
    [setSelection],
  );

  const restore = useCallback(
    async (which: "undo" | "redo") => {
      try {
        adopt(await (which === "undo" ? backend.pcb.undo() : backend.pcb.redo()));
        setSelection([]);
        setDrcStale(true);
      } catch (error) {
        onError(error);
      }
    },
    [adopt, onError, setSelection],
  );

  const sync = useCallback(async () => {
    try {
      const result = await backend.pcb.sync();
      adopt(result.view);
      setSyncReport(result.report);
      setDrcStale(true);
    } catch (error) {
      onError(error);
    }
  }, [adopt, onError]);

  const runDrc = useCallback(async () => {
    try {
      setViolations(await backend.pcb.drc());
      setDrcStale(false);
      setSideTab("drc");
    } catch (error) {
      onError(error);
    }
  }, [onError]);

  const exportFiles = useCallback(async () => {
    try {
      const directory = await backend.pickDirectory();
      if (!directory) {
        setExporting(null);
        return;
      }
      setExporting(await backend.pcb.export(directory));
    } catch (error) {
      setExporting(null);
      onError(error);
    }
  }, [onError]);

  const startExport = useCallback(async () => {
    try {
      const found = await backend.pcb.drc();
      setViolations(found);
      setDrcStale(false);
      if (found.some((violation) => violation.severity === "error")) setExporting("ask");
      else await exportFiles();
    } catch (error) {
      onError(error);
    }
  }, [exportFiles, onError]);

  const startRouter = useCallback(async () => {
    setProposal(null);
    routerRunning.current = true;
    setRouterProgress({ done: 0, total: live.current.view?.board.airwires.length ?? 0 });
    try {
      setProposal(await backend.pcb.routeStart());
    } catch (error) {
      onError(error);
    } finally {
      routerRunning.current = false;
      setRouterProgress(null);
    }
  }, [onError]);

  const acceptProposal = useCallback(async () => {
    setProposal(null);
    try {
      adopt((await backend.pcb.routeAccept()).view);
      setDrcStale(true);
    } catch (error) {
      onError(error);
    }
  }, [adopt, onError]);

  const discardProposal = useCallback(() => {
    setProposal(null);
    backend.pcb.routeDiscard().catch(onError);
  }, [onError]);

  const finishRoute = useCallback(() => {
    routing.current = null;
    requestDraw();
  }, [requestDraw]);

  /** Sends the placed part of a route and continues from its end. */
  const commitRoute = useCallback(async () => {
    const route = routing.current;
    const board = live.current.view?.board;
    if (!route || !board || route.points.length < 2) return;
    const end = route.points[route.points.length - 1] as Point;
    await run({
      type: "add_tracks",
      net: route.net,
      layer: route.layer,
      points: route.points,
      width: board.rules.track_width,
    });
    if (routing.current === route) route.points = [end];
  }, [run]);

  /** Places a via at the end of the route and goes on to the next layer. */
  const switchLayerWithVia = useCallback(async () => {
    const route = routing.current;
    const board = live.current.view?.board;
    if (!route || !board || board.copper_layers < 2) return;
    await commitRoute();
    const end = route.points[route.points.length - 1] as Point;
    await run({ type: "add_via", net: route.net, at: end });
    route.layer = (route.layer + 1) % board.copper_layers;
    setActiveLayer(route.layer);
    requestDraw();
  }, [commitRoute, requestDraw, run]);

  const closePolygon = useCallback(() => {
    const points = polygon.current;
    polygon.current = [];
    const currentTool = live.current.tool;
    if (points.length >= 3) {
      if (currentTool === "outline") {
        void run({ type: "set_outline", points });
        setTool("select");
      } else if (currentTool === "zone") {
        setZonePoints(points);
      }
    }
    requestDraw();
  }, [requestDraw, run, setTool]);

  const cancel = useCallback(() => {
    if (routing.current) {
      finishRoute();
    } else if (polygon.current.length) {
      polygon.current = [];
      requestDraw();
    } else if (live.current.tool !== "select") {
      setTool("select");
    } else {
      setSelection([]);
    }
  }, [finishRoute, requestDraw, setSelection, setTool]);

  const selectedIds = () => [...live.current.selection];

  // Keyboard.
  const dialogOpen = syncReport !== null || zonePoints !== null || exporting !== null || routerProgress !== null;
  useEffect(() => {
    if (!active || dialogOpen) return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      const key = event.key.toLowerCase();
      if (event.ctrlKey || event.metaKey) {
        if (key === "z" && event.shiftKey) void restore("redo");
        else if (key === "z") void restore("undo");
        else if (key === "y") void restore("redo");
        else return;
        event.preventDefault();
        return;
      }
      if (key === "escape") cancel();
      else if (key === "enter") closePolygon();
      else if (key === "r" && selectedIds().length) void run({ type: "rotate", ids: selectedIds() });
      else if (key === "f" && selectedIds().length) void run({ type: "flip", ids: selectedIds() });
      else if (key === "delete" || key === "backspace") {
        if (selectedIds().length) {
          void run({ type: "delete", ids: selectedIds() });
          setSelection([]);
        }
      } else if (key === "x") setTool("route");
      else if (key === "v") {
        if (routing.current) void switchLayerWithVia();
        else setTool("via");
      } else if (key === "s") setTool("select");
      else if (key === "home") fit();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, cancel, closePolygon, dialogOpen, fit, restore, run, setSelection, setTool, switchLayerWithVia]);

  const position = (event: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return { sx: event.clientX - (rect?.left ?? 0), sy: event.clientY - (rect?.top ?? 0) };
  };

  const onPointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    const current = live.current.view;
    if (!current) return;
    const board = current.board;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const { sx, sy } = position(event);
    const world = screenToBoard(camera.current, sx, sy);
    const snapped = snapTo(world, live.current.grid);
    const tolerance = (5 / camera.current.scale) * MM;
    if (event.button === 1) {
      drag.current = { kind: "pan", sx, sy, ox: camera.current.ox, oy: camera.current.oy };
      return;
    }
    if (event.button === 2) {
      if (live.current.tool === "outline" || live.current.tool === "zone") closePolygon();
      else cancel();
      return;
    }
    const layer = live.current.activeLayer;
    switch (live.current.tool) {
      case "select": {
        const shown = visible(board.copper_layers);
        const hit = hitElement(board, world, tolerance, shown);
        if (hit === null) {
          if (!event.shiftKey) setSelection([]);
          drag.current = { kind: "box", start: world, additive: event.shiftKey };
          return;
        }
        if (event.shiftKey) {
          const next = new Set(live.current.selection);
          if (next.has(hit)) next.delete(hit);
          else next.add(hit);
          setSelection(next);
        } else if (!live.current.selection.has(hit)) {
          setSelection([hit]);
        }
        drag.current = { kind: "move", start: snapped, moved: false };
        return;
      }
      case "route": {
        const copper = copperAt(board, world, layer, tolerance);
        const route = routing.current;
        if (!route) {
          if (copper) routing.current = { net: copper.net, layer, points: [copper.at] };
          requestDraw();
          return;
        }
        const last = route.points[route.points.length - 1] as Point;
        const reachesOwnNet = copper?.net === route.net;
        const target = reachesOwnNet && copper ? copper.at : snapped;
        route.points = [...route.points, ...route45(last, target)];
        if (reachesOwnNet && route.points.length >= 2) {
          void commitRoute().then(finishRoute);
        } else {
          void commitRoute();
        }
        requestDraw();
        return;
      }
      case "via": {
        const copper = copperAt(board, world, layer, tolerance);
        if (copper) void run({ type: "add_via", net: copper.net, at: copper.at });
        return;
      }
      case "outline":
      case "zone":
        polygon.current = [...polygon.current, snapped];
        requestDraw();
        return;
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    const { sx, sy } = position(event);
    const world = screenToBoard(camera.current, sx, sy);
    const snapped = snapTo(world, live.current.grid);
    cursor.current = snapped;
    rawCursor.current = world;
    const current = drag.current;
    if (current?.kind === "pan") {
      camera.current = {
        ...camera.current,
        ox: current.ox + sx - current.sx,
        oy: current.oy + sy - current.sy,
      };
    } else if (current?.kind === "move") {
      const delta = { x: snapped.x - current.start.x, y: snapped.y - current.start.y };
      if (delta.x !== 0 || delta.y !== 0) current.moved = true;
      moveDelta.current = current.moved ? delta : null;
    } else if (current?.kind === "box") {
      box.current = {
        min: { x: Math.min(current.start.x, world.x), y: Math.min(current.start.y, world.y) },
        max: { x: Math.max(current.start.x, world.x), y: Math.max(current.start.y, world.y) },
      };
    }
    requestDraw();
  };

  const onPointerUp = () => {
    const current = drag.current;
    drag.current = null;
    if (current?.kind === "move" && current.moved && moveDelta.current) {
      void run({ type: "move", ids: selectedIds(), delta: moveDelta.current });
    } else if (current?.kind === "box" && box.current && live.current.view) {
      const found = elementsInBox(live.current.view.board, box.current.min, box.current.max);
      setSelection(current.additive ? [...live.current.selection, ...found] : found);
    }
    moveDelta.current = null;
    box.current = null;
    requestDraw();
  };

  const onDoubleClick = () => {
    const currentTool = live.current.tool;
    if (currentTool === "outline" || currentTool === "zone") closePolygon();
    else if (currentTool === "route") finishRoute();
  };

  const onWheel = (event: WheelEvent<HTMLCanvasElement>) => {
    const { sx, sy } = position(event);
    camera.current = zoomCamera(camera.current, sx, sy, event.deltaY < 0 ? 1.15 : 1 / 1.15);
    requestDraw();
  };

  const goToViolation = useCallback(
    (violation: DrcViolation) => {
      const { width, height } = size.current;
      const scale = Math.max(camera.current.scale, 12);
      camera.current = {
        scale,
        ox: width / 2 - (violation.at.x / MM) * scale,
        oy: height / 2 - (violation.at.y / MM) * scale,
      };
      setSelection(violation.elements);
      requestDraw();
    },
    [requestDraw, setSelection],
  );

  if (!view) return <div className="editor" />;
  const board = view.board;
  const hasSelection = selection.size > 0;
  const layerName = (layer: number) =>
    layer === 0
      ? t("pcb.layers.top")
      : layer === board.copper_layers - 1
        ? t("pcb.layers.bottom")
        : t("pcb.layers.inner", { n: layer });
  const gridFormat = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 });

  return (
    <div className="editor">
      <aside className="library" aria-label={t("pcb.layers.title")}>
        <h2>{t("pcb.layers.title")}</h2>
        <ul className="layer-list">
          {[...Array(board.copper_layers).keys()].map((layer) => (
            <li key={layer} className={layer === activeLayer ? "layer-row layer-active" : "layer-row"}>
              <input
                type="checkbox"
                aria-label={t("pcb.layers.visible", { name: layerName(layer) })}
                checked={!hidden.has(layer)}
                onChange={(event) => {
                  const next = new Set(hidden);
                  if (event.target.checked) next.delete(layer);
                  else next.add(layer);
                  setHidden(next);
                }}
              />
              <button type="button" className="layer-pick" onClick={() => setActiveLayer(layer)} aria-pressed={layer === activeLayer}>
                <i className={`layer-swatch layer-swatch-${layerColor(layer, board.copper_layers).slice(1)}`} />
                {layerName(layer)}
              </button>
            </li>
          ))}
        </ul>
        <label className="field">
          <span>{t("pcb.grid")}</span>
          <select value={grid} onChange={(event) => setGrid(Number(event.target.value))}>
            {GRIDS.map((value) => (
              <option key={value} value={value}>
                {t("pcb.mmValue", { value: gridFormat.format(value / MM) })}
              </option>
            ))}
          </select>
        </label>
        <p className="muted hint">{t("pcb.routeHint")}</p>
        <p className={board.airwires.length ? "airwire-count airwire-open" : "airwire-count"}>
          {t("pcb.airwires", { count: board.airwires.length })}
        </p>
      </aside>
      <div className="editor-main">
        <div className="editor-toolbar" role="toolbar" aria-label={t("schematic.toolbar")}>
          <ToolButton label={t("pcb.tool.select")} icon={icons.pointer} active={tool === "select"} onClick={() => setTool("select")} />
          <ToolButton label={t("pcb.tool.route")} icon={icons.wire} active={tool === "route"} onClick={() => setTool("route")} />
          <ToolButton label={t("pcb.tool.via")} icon={icons.junction} active={tool === "via"} onClick={() => setTool("via")} />
          <ToolButton label={t("pcb.tool.outline")} icon={icons.fit} active={tool === "outline"} onClick={() => setTool("outline")} />
          <ToolButton label={t("pcb.tool.zone")} icon={icons.block} active={tool === "zone"} onClick={() => setTool("zone")} />
          <span className="tool-separator" />
          <ToolButton label={t("schematic.undo")} icon={icons.undo} disabled={!view.can_undo} onClick={() => void restore("undo")} />
          <ToolButton label={t("schematic.redo")} icon={icons.redo} disabled={!view.can_redo} onClick={() => void restore("redo")} />
          <span className="tool-separator" />
          <ToolButton label={t("pcb.rotate")} icon={icons.rotate} disabled={!hasSelection} onClick={() => void run({ type: "rotate", ids: selectedIds() })} />
          <ToolButton label={t("pcb.flip")} icon={icons.mirror} disabled={!hasSelection} onClick={() => void run({ type: "flip", ids: selectedIds() })} />
          <ToolButton
            label={t("pcb.delete")}
            icon={icons.trash}
            disabled={!hasSelection}
            onClick={() => {
              void run({ type: "delete", ids: selectedIds() });
              setSelection([]);
            }}
          />
          <ToolButton label={t("pcb.fit")} icon={icons.project} onClick={fit} />
          <span className="pcb-actions">
            <button type="button" onClick={() => void sync()}>
              {t("pcb.sync")}
            </button>
            <button type="button" onClick={() => void startRouter()}>
              {t("pcb.router.start")}
            </button>
            <button type="button" onClick={() => void runDrc()}>
              {t("pcb.runDrc")}
            </button>
            <button type="button" onClick={() => void startExport()}>
              {t("pcb.export")}
            </button>
          </span>
        </div>
        {view.deviations.length > 0 && (
          <p className="sync-banner" role="status">
            {t("pcb.deviations", { count: view.deviations.length })}
          </p>
        )}
        {proposal && (
          <div className="route-banner" role="status">
            <span>
              {t("pcb.router.result", {
                done: proposal.total - Math.min(proposal.remaining.length, proposal.total),
                total: proposal.total,
                vias: proposal.vias.length,
              })}
              {proposal.cancelled && ` ${t("pcb.router.cancelled")}`}
              {proposal.timed_out && ` ${t("pcb.router.timedOut")}`}
            </span>
            {proposal.remaining.length > 0 && (
              <span className="route-open">
                {t("pcb.router.open", { nets: [...new Set(proposal.remaining.map((wire) => wire.net))].join(", ") })}
              </span>
            )}
            <span className="route-actions">
              <button type="button" onClick={discardProposal}>
                {t("pcb.router.discard")}
              </button>
              <button type="button" className="primary" disabled={proposal.tracks.length === 0} onClick={() => void acceptProposal()}>
                {t("pcb.router.accept")}
              </button>
            </span>
          </div>
        )}
        <canvas
          ref={canvasRef}
          className="editor-canvas"
          aria-label={t("pcb.canvas")}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={() => {
            cursor.current = null;
            requestDraw();
          }}
          onDoubleClick={onDoubleClick}
          onWheel={onWheel}
          onContextMenu={(event) => event.preventDefault()}
        />
      </div>
      <PcbSidePanel
        tab={sideTab}
        onTab={setSideTab}
        board={board}
        selection={selection}
        violations={violations}
        stale={drcStale}
        layerName={layerName}
        onRunDrc={() => void runDrc()}
        onViolation={goToViolation}
        onCommand={(command) => void run(command)}
      />

      {routerProgress && (
        <RouterProgressDialog
          done={routerProgress.done}
          total={routerProgress.total}
          onCancel={() => {
            backend.pcb.routeCancel().catch(onError);
          }}
        />
      )}
      {syncReport && <SyncDialog report={syncReport} onClose={() => setSyncReport(null)} />}
      {zonePoints && (
        <ZoneDialog
          nets={board.nets}
          layerName={layerName(activeLayer)}
          onCancel={() => setZonePoints(null)}
          onConfirm={(net) => {
            void run({ type: "add_zone", net, layer: activeLayer, outline: zonePoints });
            setZonePoints(null);
            setTool("select");
          }}
        />
      )}
      {exporting !== null && (
        <ExportDialog
          state={exporting}
          errors={(violations ?? []).filter((violation) => violation.severity === "error").length}
          onContinue={() => void exportFiles()}
          onClose={() => setExporting(null)}
        />
      )}
    </div>
  );
}
