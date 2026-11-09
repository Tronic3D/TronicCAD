import { PointerEvent, ReactNode, useCallback, useEffect, useMemo, useRef, useState, WheelEvent } from "react";
import { useTranslation } from "react-i18next";
import { backend } from "../api/backend";
import { icons } from "../components/icons";
import { CustomPartDialog, TextPromptDialog, LabelDialog } from "./EditorDialogs";
import {
  allElementIds,
  Camera,
  clipFromSelection,
  clipOrigin,
  elementsInRect,
  fitCamera,
  GRID,
  hitTest,
  isConnectionPoint,
  Rect,
  route,
  screenToWorld,
  sheetBounds,
  snap,
  zoomAt,
} from "./geometry";
import { LibraryPanel } from "./LibraryPanel";
import { drawScene, Overlay } from "./render";
import { SidePanel } from "./SidePanel";
import {
  ApplyResult,
  Clip,
  CustomPartSpec,
  EditCommand,
  LabelKind,
  NetHighlight,
  Point,
  Rotation,
  SchematicView,
  SymbolDef,
  Violation,
} from "./types";

export type Tool =
  | { kind: "select" }
  | { kind: "wire" }
  | { kind: "bus" }
  | { kind: "label" }
  | { kind: "no_connect" }
  | { kind: "junction" }
  | { kind: "block" }
  | { kind: "place"; symbol: string; rotation: Rotation; mirror: boolean };

type Prompt =
  | { kind: "label"; at: Point }
  | { kind: "block"; at: Point }
  | { kind: "add_sheet" }
  | { kind: "rename_sheet"; sheet: number; name: string }
  | { kind: "custom_part" };

type Drag =
  | { kind: "pan"; sx: number; sy: number; ox: number; oy: number }
  | { kind: "move"; start: Point; moved: boolean }
  | { kind: "box"; start: Point; additive: boolean };

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

const unset = new Set<number>();

export function SchematicEditor(props: {
  /** False while a dialog of the application is open. */
  active: boolean;
  onDirty: (dirty: boolean) => void;
  onError: (error: unknown) => void;
}) {
  const { active, onDirty, onError } = props;
  const { t } = useTranslation();
  const [view, setView] = useState<SchematicView | null>(null);
  const [library, setLibrary] = useState<SymbolDef[]>([]);
  const [tool, setToolState] = useState<Tool>({ kind: "select" });
  const [selection, setSelectionState] = useState<ReadonlySet<number>>(unset);
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const [highlight, setHighlight] = useState<NetHighlight | null>(null);
  const [violations, setViolations] = useState<Violation[] | null>(null);
  const [ercStale, setErcStale] = useState(false);
  const [sideTab, setSideTab] = useState<"properties" | "erc" | "netlist">("properties");
  const [netlistRevision, setNetlistRevision] = useState(0);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const size = useRef({ width: 800, height: 600 });
  const camera = useRef<Camera>({ ox: 400, oy: 300, scale: 14 });
  const fitted = useRef(false);
  const sized = useRef(false);
  const drag = useRef<Drag | null>(null);
  const routing = useRef<Point[]>([]);
  const cursor = useRef<Point | null>(null);
  const moveDelta = useRef<Point | null>(null);
  const box = useRef<Rect | null>(null);
  const clipboard = useRef<Clip | null>(null);
  const frameRequest = useRef<number | null>(null);

  // Current values for the event handlers and the draw function.
  const live = useRef({ view, tool, selection, highlight, violations });
  live.current = { view, tool, selection, highlight, violations };

  const draw = useCallback(() => {
    frameRequest.current = null;
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { alpha: false });
    const { view: currentView, tool: currentTool, selection: currentSelection, highlight: net, violations: found } = live.current;
    if (!context || !currentView) return;
    const routed =
      (currentTool.kind === "wire" || currentTool.kind === "bus") && routing.current.length && cursor.current
        ? {
            points: [
              ...routing.current,
              ...route(routing.current[routing.current.length - 1] as Point, cursor.current),
            ],
            bus: currentTool.kind === "bus",
          }
        : null;
    const ghostSymbol =
      currentTool.kind === "place"
        ? (currentView.symbols[currentTool.symbol] ?? libraryRef.current.find((s) => s.id === currentTool.symbol))
        : undefined;
    const overlay: Overlay = {
      selection: currentSelection,
      highlight: net ? { elements: new Set(net.elements), pins: net.pins } : null,
      violations: found ?? [],
      moveDelta: moveDelta.current,
      routing: routed,
      ghost:
        ghostSymbol && cursor.current && currentTool.kind === "place"
          ? {
              symbol: ghostSymbol,
              at: cursor.current,
              rotation: currentTool.rotation,
              mirror: currentTool.mirror,
            }
          : null,
      box: box.current,
      cursor: currentTool.kind !== "select" && currentTool.kind !== "place" ? cursor.current : null,
    };
    drawScene(
      context,
      size.current.width,
      size.current.height,
      window.devicePixelRatio || 1,
      camera.current,
      currentView,
      overlay,
    );
  }, []);

  const libraryRef = useRef<SymbolDef[]>([]);
  libraryRef.current = library;

  const requestDraw = useCallback(() => {
    if (frameRequest.current === null) {
      frameRequest.current = requestAnimationFrame(draw);
    }
  }, [draw]);

  useEffect(requestDraw, [view, tool, selection, highlight, violations, requestDraw]);

  const adopt = useCallback(
    (next: SchematicView) => {
      setView(next);
      onDirty(next.dirty);
    },
    [onDirty],
  );

  const setSelection = useCallback((ids: Iterable<number>) => setSelectionState(new Set(ids)), []);

  const setTool = useCallback(
    (next: Tool) => {
      routing.current = [];
      setToolState(next);
      if (next.kind !== "select") setSelection([]);
    },
    [setSelection],
  );

  // Loading.
  useEffect(() => {
    backend.schematic.library().then(setLibrary).catch(onError);
    backend.schematic
      .view(1)
      .then((loaded) => setView(loaded))
      .catch(onError);
  }, [onError]);

  const sheetId = view?.sheet.id ?? 1;

  const fit = useCallback(() => {
    const current = live.current.view;
    if (!current) return;
    camera.current = fitCamera(
      sheetBounds(current.sheet, current.symbols),
      size.current.width,
      size.current.height,
    );
    requestDraw();
  }, [requestDraw]);

  // The first view waits for both the document and the real canvas size.
  const fitOnce = useCallback(() => {
    if (!fitted.current && live.current.view && sized.current) {
      fitted.current = true;
      fit();
    }
  }, [fit]);

  useEffect(fitOnce, [view, fitOnce]);

  // Canvas size.
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
  }, [draw, fitOnce, view !== null]);

  const run = useCallback(
    async (command: EditCommand): Promise<ApplyResult | null> => {
      try {
        const result = await backend.schematic.apply(live.current.view?.sheet.id ?? 1, command);
        adopt(result.view);
        setErcStale(true);
        setHighlight(null);
        setNetlistRevision((revision) => revision + 1);
        return result;
      } catch (error) {
        onError(error);
        return null;
      }
    },
    [adopt, onError],
  );

  const showSheet = useCallback(
    async (id: number) => {
      try {
        routing.current = [];
        setSelection([]);
        setHighlight(null);
        const next = await backend.schematic.view(id);
        setView(next);
        camera.current = fitCamera(sheetBounds(next.sheet, next.symbols), size.current.width, size.current.height);
      } catch (error) {
        onError(error);
      }
    },
    [onError, setSelection],
  );

  const restore = useCallback(
    async (which: "undo" | "redo") => {
      try {
        const id = live.current.view?.sheet.id ?? 1;
        const next = await (which === "undo" ? backend.schematic.undo(id) : backend.schematic.redo(id));
        adopt(next);
        setSelection([]);
        setErcStale(true);
        setHighlight(null);
        setNetlistRevision((revision) => revision + 1);
      } catch (error) {
        onError(error);
      }
    },
    [adopt, onError, setSelection],
  );

  const selectedIds = () => [...live.current.selection];

  const transformSelection = useCallback(
    (type: "rotate" | "mirror") => {
      const currentTool = live.current.tool;
      if (currentTool.kind === "place") {
        setToolState(
          type === "rotate"
            ? { ...currentTool, rotation: (((currentTool.rotation + 90) % 360) as Rotation) }
            : { ...currentTool, mirror: !currentTool.mirror },
        );
        return;
      }
      const ids = selectedIds();
      if (ids.length) void run({ type, sheet: live.current.view?.sheet.id ?? 1, ids });
    },
    [run],
  );

  const deleteSelection = useCallback(() => {
    const ids = selectedIds();
    if (ids.length) {
      void run({ type: "delete", sheet: live.current.view?.sheet.id ?? 1, ids });
      setSelection([]);
    }
  }, [run, setSelection]);

  const finishRoute = useCallback(() => {
    const currentTool = live.current.tool;
    const points = routing.current;
    routing.current = [];
    if (points.length >= 2 && (currentTool.kind === "wire" || currentTool.kind === "bus")) {
      void run({
        type: currentTool.kind === "wire" ? "add_wires" : "add_buses",
        sheet: live.current.view?.sheet.id ?? 1,
        points,
      });
    }
    requestDraw();
  }, [requestDraw, run]);

  const cancel = useCallback(() => {
    if (routing.current.length) {
      routing.current = [];
      requestDraw();
    } else if (live.current.tool.kind !== "select") {
      setTool({ kind: "select" });
    } else {
      setSelection([]);
    }
  }, [requestDraw, setSelection, setTool]);

  const paste = useCallback(async () => {
    const clip = clipboard.current;
    const origin = clip && clipOrigin(clip);
    if (!clip || !origin) return;
    const target = cursor.current ?? { x: origin.x + 2 * GRID, y: origin.y + 2 * GRID };
    const result = await run({
      type: "paste",
      sheet: live.current.view?.sheet.id ?? 1,
      clip,
      delta: { x: target.x - origin.x, y: target.y - origin.y },
    });
    if (result) setSelection(result.created);
  }, [run, setSelection]);

  // Keyboard.
  useEffect(() => {
    if (!active || prompt) return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      const key = event.key.toLowerCase();
      const current = live.current.view;
      if (event.ctrlKey || event.metaKey) {
        if (key === "z" && event.shiftKey) void restore("redo");
        else if (key === "z") void restore("undo");
        else if (key === "y") void restore("redo");
        else if (key === "c" && current) clipboard.current = clipFromSelection(current.sheet, live.current.selection);
        else if (key === "x" && current) {
          clipboard.current = clipFromSelection(current.sheet, live.current.selection);
          deleteSelection();
        } else if (key === "v") void paste();
        else if (key === "a" && current) setSelection(allElementIds(current.sheet));
        else return;
        event.preventDefault();
        return;
      }
      if (key === "escape") cancel();
      else if (key === "r") transformSelection("rotate");
      else if (key === "m") transformSelection("mirror");
      else if (key === "delete" || key === "backspace") deleteSelection();
      else if (key === "w") setTool({ kind: "wire" });
      else if (key === "b") setTool({ kind: "bus" });
      else if (key === "l") setTool({ kind: "label" });
      else if (key === "q") setTool({ kind: "no_connect" });
      else if (key === "j") setTool({ kind: "junction" });
      else if (key === "s") setTool({ kind: "select" });
      else if (key === "f") fit();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, cancel, deleteSelection, fit, paste, prompt, restore, setSelection, setTool, transformSelection]);

  const position = (event: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return { sx: event.clientX - (rect?.left ?? 0), sy: event.clientY - (rect?.top ?? 0) };
  };

  const onPointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    const current = live.current.view;
    if (!current) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const { sx, sy } = position(event);
    const world = screenToWorld(camera.current, sx, sy);
    const snapped = snap(world);
    if (event.button === 1) {
      drag.current = { kind: "pan", sx, sy, ox: camera.current.ox, oy: camera.current.oy };
      return;
    }
    if (event.button === 2) {
      if (routing.current.length >= 2) finishRoute();
      else cancel();
      return;
    }
    const currentTool = live.current.tool;
    const sheet = current.sheet.id;
    switch (currentTool.kind) {
      case "select": {
        const tolerance = (5 / camera.current.scale) * GRID;
        const hit = hitTest(current.sheet, current.symbols, world, tolerance);
        if (hit === null) {
          if (!event.shiftKey) setSelection([]);
          drag.current = { kind: "box", start: world, additive: event.shiftKey };
          return;
        }
        const selected = live.current.selection;
        if (event.shiftKey) {
          const next = new Set(selected);
          if (next.has(hit)) next.delete(hit);
          else next.add(hit);
          setSelection(next);
        } else if (!selected.has(hit)) {
          setSelection([hit]);
          live.current.selection = new Set([hit]);
        }
        drag.current = { kind: "move", start: snapped, moved: false };
        return;
      }
      case "wire":
      case "bus": {
        const points = routing.current;
        const last = points[points.length - 1];
        if (!last) {
          routing.current = [snapped];
        } else {
          routing.current = [...points, ...route(last, snapped)];
          const endsOnSomething =
            currentTool.kind === "wire" &&
            routing.current.length >= 2 &&
            isConnectionPoint(current.sheet, current.symbols, snapped);
          if (endsOnSomething) finishRoute();
        }
        requestDraw();
        return;
      }
      case "label":
        setPrompt({ kind: "label", at: snapped });
        return;
      case "block":
        setPrompt({ kind: "block", at: snapped });
        return;
      case "no_connect":
        void run({ type: "add_no_connect", sheet, at: snapped });
        return;
      case "junction":
        void run({ type: "add_junction", sheet, at: snapped });
        return;
      case "place":
        void run({
          type: "place_component",
          sheet,
          symbol: currentTool.symbol,
          at: snapped,
          rotation: currentTool.rotation,
          mirror: currentTool.mirror,
        });
        return;
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    const { sx, sy } = position(event);
    const world = screenToWorld(camera.current, sx, sy);
    const snapped = snap(world);
    cursor.current = snapped;
    const active = drag.current;
    if (active?.kind === "pan") {
      camera.current = {
        ...camera.current,
        ox: active.ox + sx - active.sx,
        oy: active.oy + sy - active.sy,
      };
    } else if (active?.kind === "move") {
      const delta = { x: snapped.x - active.start.x, y: snapped.y - active.start.y };
      if (delta.x !== 0 || delta.y !== 0) active.moved = true;
      moveDelta.current = active.moved ? delta : null;
    } else if (active?.kind === "box") {
      box.current = {
        minX: Math.min(active.start.x, world.x),
        minY: Math.min(active.start.y, world.y),
        maxX: Math.max(active.start.x, world.x),
        maxY: Math.max(active.start.y, world.y),
      };
    }
    requestDraw();
  };

  const onPointerUp = () => {
    const active = drag.current;
    drag.current = null;
    if (active?.kind === "move" && active.moved && moveDelta.current) {
      const delta = moveDelta.current;
      moveDelta.current = null;
      void run({ type: "move", sheet: sheetId, ids: selectedIds(), delta });
    } else if (active?.kind === "box" && box.current && live.current.view) {
      const found = elementsInRect(live.current.view.sheet, live.current.view.symbols, box.current);
      box.current = null;
      setSelection(active.additive ? [...live.current.selection, ...found] : found);
    }
    moveDelta.current = null;
    box.current = null;
    requestDraw();
  };

  const onDoubleClick = (event: { clientX: number; clientY: number }) => {
    const current = live.current.view;
    if (!current) return;
    const currentTool = live.current.tool;
    if (currentTool.kind === "wire" || currentTool.kind === "bus") {
      finishRoute();
      return;
    }
    if (currentTool.kind !== "select") return;
    const { sx, sy } = position(event);
    const tolerance = (5 / camera.current.scale) * GRID;
    const hit = hitTest(current.sheet, current.symbols, screenToWorld(camera.current, sx, sy), tolerance);
    const block = current.sheet.instances.find((instance) => instance.id === hit);
    if (block) void showSheet(block.sheet);
  };

  const onWheel = (event: WheelEvent<HTMLCanvasElement>) => {
    const { sx, sy } = position(event);
    camera.current = zoomAt(camera.current, sx, sy, event.deltaY < 0 ? 1.15 : 1 / 1.15);
    requestDraw();
  };

  // The net of a single selected wire or label is highlighted and named.
  useEffect(() => {
    const ids = [...selection];
    const only = ids[0];
    const current = live.current.view;
    if (ids.length !== 1 || only === undefined || !current) {
      setHighlight(null);
      return;
    }
    const isNetElement =
      current.sheet.wires.some((wire) => wire.id === only) ||
      current.sheet.labels.some((label) => label.id === only) ||
      current.sheet.components.some((component) => component.id === only && current.symbols[component.symbol]?.pins.length === 1);
    if (!isNetElement) {
      setHighlight(null);
      return;
    }
    let stale = false;
    backend.schematic
      .netOf(current.sheet.id, only)
      .then((net) => {
        if (!stale) setHighlight(net);
      })
      .catch(onError);
    return () => {
      stale = true;
    };
  }, [selection, onError]);

  const runErc = useCallback(async () => {
    try {
      setViolations(await backend.schematic.erc());
      setErcStale(false);
      setSideTab("erc");
    } catch (error) {
      onError(error);
    }
  }, [onError]);

  const goToViolation = useCallback(
    async (violation: Violation) => {
      if (violation.sheet !== live.current.view?.sheet.id) await showSheet(violation.sheet);
      const { width, height } = size.current;
      const scale = Math.max(camera.current.scale, 14);
      camera.current = {
        scale,
        ox: width / 2 - (violation.at.x / GRID) * scale,
        oy: height / 2 - (violation.at.y / GRID) * scale,
      };
      setSelection(violation.elements);
      requestDraw();
    },
    [requestDraw, setSelection, showSheet],
  );

  const createPart = useCallback(
    async (spec: CustomPartSpec) => {
      const next = await backend.schematic.createPart(sheetId, spec);
      adopt(next);
      setPrompt(null);
      setTool({ kind: "place", symbol: `project:${spec.name.trim()}`, rotation: 0, mirror: false });
    },
    [adopt, setTool, sheetId],
  );

  const importLibrary = useCallback(async () => {
    try {
      const path = await backend.pickOpenFile(t("fileFilter.library"), "troniclib");
      if (path) adopt(await backend.schematic.importLibrary(sheetId, path));
    } catch (error) {
      onError(error);
    }
  }, [adopt, onError, sheetId, t]);

  const exportLibrary = useCallback(async () => {
    try {
      const path = await backend.pickSaveFile(t("fileFilter.library"), "troniclib");
      if (path) await backend.schematic.exportLibrary(path);
    } catch (error) {
      onError(error);
    }
  }, [onError, t]);

  const allSymbols = useMemo(() => {
    const known = new Set(library.map((symbol) => symbol.id));
    const own = Object.values(view?.symbols ?? {}).filter((symbol) => !known.has(symbol.id));
    return [...library, ...own];
  }, [library, view?.symbols]);

  if (!view) return <div className="editor" />;

  const newSheetName = () => {
    for (let n = view.sheets.length + 1; ; n++) {
      const name = t("schematic.defaultSheetName", { n });
      if (!view.sheets.some((sheet) => sheet.name === name)) return name;
    }
  };

  const toolActive = (kind: Tool["kind"]) => tool.kind === kind;
  const hasSelection = selection.size > 0;

  return (
    <div className="editor">
      <LibraryPanel
        symbols={allSymbols}
        activeSymbol={tool.kind === "place" ? tool.symbol : null}
        onPick={(symbol) => setTool({ kind: "place", symbol, rotation: 0, mirror: false })}
        onNewPart={() => setPrompt({ kind: "custom_part" })}
        onImport={() => void importLibrary()}
        onExport={() => void exportLibrary()}
      />
      <div className="editor-main">
        <div className="editor-toolbar" role="toolbar" aria-label={t("schematic.toolbar")}>
          <ToolButton label={t("schematic.tool.select")} icon={icons.pointer} active={toolActive("select")} onClick={() => setTool({ kind: "select" })} />
          <ToolButton label={t("schematic.tool.wire")} icon={icons.wire} active={toolActive("wire")} onClick={() => setTool({ kind: "wire" })} />
          <ToolButton label={t("schematic.tool.bus")} icon={icons.bus} active={toolActive("bus")} onClick={() => setTool({ kind: "bus" })} />
          <ToolButton label={t("schematic.tool.label")} icon={icons.label} active={toolActive("label")} onClick={() => setTool({ kind: "label" })} />
          <ToolButton label={t("schematic.tool.noConnect")} icon={icons.noConnect} active={toolActive("no_connect")} onClick={() => setTool({ kind: "no_connect" })} />
          <ToolButton label={t("schematic.tool.junction")} icon={icons.junction} active={toolActive("junction")} onClick={() => setTool({ kind: "junction" })} />
          <ToolButton label={t("schematic.tool.block")} icon={icons.block} active={toolActive("block")} onClick={() => setTool({ kind: "block" })} />
          <span className="tool-separator" />
          <ToolButton label={t("schematic.undo")} icon={icons.undo} disabled={!view.can_undo} onClick={() => void restore("undo")} />
          <ToolButton label={t("schematic.redo")} icon={icons.redo} disabled={!view.can_redo} onClick={() => void restore("redo")} />
          <span className="tool-separator" />
          <ToolButton label={t("schematic.rotate")} icon={icons.rotate} disabled={!hasSelection && tool.kind !== "place"} onClick={() => transformSelection("rotate")} />
          <ToolButton label={t("schematic.mirror")} icon={icons.mirror} disabled={!hasSelection && tool.kind !== "place"} onClick={() => transformSelection("mirror")} />
          <ToolButton label={t("schematic.delete")} icon={icons.trash} disabled={!hasSelection} onClick={deleteSelection} />
          <span className="tool-separator" />
          <ToolButton label={t("schematic.annotate")} icon={icons.annotate} onClick={() => void run({ type: "annotate" })} />
          <ToolButton label={t("schematic.fit")} icon={icons.fit} onClick={fit} />
          <button type="button" className="erc-run" onClick={() => void runErc()}>
            {t("schematic.runErc")}
          </button>
        </div>
        <canvas
          ref={canvasRef}
          className="editor-canvas"
          aria-label={t("schematic.canvas")}
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
        <div className="sheet-tabs" role="tablist" aria-label={t("schematic.sheets")}>
          {view.sheets.map((sheet) => (
            <span key={sheet.id} className={`sheet-tab${sheet.id === view.sheet.id ? " sheet-tab-active" : ""}`}>
              <button
                type="button"
                role="tab"
                aria-selected={sheet.id === view.sheet.id}
                onClick={() => void showSheet(sheet.id)}
                onDoubleClick={() => setPrompt({ kind: "rename_sheet", sheet: sheet.id, name: sheet.name })}
                title={t("schematic.sheetHint")}
              >
                {"· ".repeat(sheet.depth)}
                {sheet.name}
              </button>
              {sheet.parent === null && view.sheets.filter((s) => s.parent === null).length > 1 && (
                <button
                  type="button"
                  className="sheet-close"
                  aria-label={t("schematic.deleteSheet", { name: sheet.name })}
                  title={t("schematic.deleteSheet", { name: sheet.name })}
                  onClick={() => {
                    void run({ type: "delete_sheet", sheet: sheet.id });
                  }}
                >
                  {icons.close}
                </button>
              )}
            </span>
          ))}
          <ToolButton label={t("schematic.addSheet")} icon={icons.plus} onClick={() => setPrompt({ kind: "add_sheet" })} />
        </div>
      </div>
      <SidePanel
        tab={sideTab}
        onTab={setSideTab}
        view={view}
        selection={selection}
        highlight={highlight}
        violations={violations}
        ercStale={ercStale}
        netlistRevision={netlistRevision}
        onRunErc={() => void runErc()}
        onViolation={(violation) => void goToViolation(violation)}
        onCommand={(command) => void run(command)}
        onError={onError}
      />

      {prompt?.kind === "label" && (
        <LabelDialog
          onCancel={() => setPrompt(null)}
          onConfirm={(name: string, kind: LabelKind) => {
            setPrompt(null);
            void run({ type: "add_label", sheet: sheetId, at: prompt.at, name, kind });
          }}
        />
      )}
      {prompt?.kind === "block" && (
        <TextPromptDialog
          title={t("schematic.newBlock")}
          label={t("schematic.sheetName")}
          initial={newSheetName()}
          onCancel={() => setPrompt(null)}
          onConfirm={(name) => {
            setPrompt(null);
            setTool({ kind: "select" });
            void run({ type: "add_block", sheet: sheetId, at: prompt.at, name });
          }}
        />
      )}
      {prompt?.kind === "add_sheet" && (
        <TextPromptDialog
          title={t("schematic.addSheet")}
          label={t("schematic.sheetName")}
          initial={newSheetName()}
          onCancel={() => setPrompt(null)}
          onConfirm={(name) => {
            setPrompt(null);
            void run({ type: "add_sheet", name }).then((result) => {
              const created = result?.created[0];
              if (created !== undefined) void showSheet(created);
            });
          }}
        />
      )}
      {prompt?.kind === "rename_sheet" && (
        <TextPromptDialog
          title={t("schematic.renameSheet")}
          label={t("schematic.sheetName")}
          initial={prompt.name}
          onCancel={() => setPrompt(null)}
          onConfirm={(name) => {
            setPrompt(null);
            void run({ type: "rename_sheet", sheet: prompt.sheet, name });
          }}
        />
      )}
      {prompt?.kind === "custom_part" && (
        <CustomPartDialog onCancel={() => setPrompt(null)} onCreate={createPart} />
      )}
    </div>
  );
}
