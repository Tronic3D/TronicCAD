// End-to-end test of simulation and 3D view in the real application: results
// against hand-calculated values, real pixels from WebGL, real file round trips.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9339;
const GRID = 1_270_000;
const MM = 1_000_000;
const dataDir = useIsolatedData();
const root = mkdtempSync(join(tmpdir(), "troniccad-s3-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "Teiler.tronic");
const problems = [];
const results = [];

async function step(name, action) {
  try {
    await action();
    results.push({ name, ok: true });
    console.log(`ok    ${name}`);
  } catch (error) {
    results.push({ name, ok: false, error: String(error) });
    console.error(`FAIL  ${name}\n      ${String(error).split("\n").slice(0, 8).join("\n      ")}`);
    throw error;
  }
}

const app = await launch(PORT, problems);
const { page } = app;
const shot = (name) => page.screenshot({ path: join(artifacts, `sim3d-${name}.png`) });
const settled = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 60))));
const near = (actual, expected, tolerance, label) =>
  assert(Math.abs(actual - expected) <= tolerance, `${label}: ${actual}, expected ${expected} ± ${tolerance}`);
const module = (name) => page.getByRole("navigation").getByRole("button", { name, exact: true }).click();

let column = 0;
/** Places a part, sets its value and joins pins to nets by global labels. */
async function place(reference, symbol, value, pins) {
  const at = { x: column * 40 * GRID, y: 0 };
  column += 1;
  const result = await invoke(page, "schematic_apply", {
    viewSheet: 1,
    command: { type: "place_component", sheet: 1, symbol, at },
  });
  const id = result.created[0];
  const created = result.view.sheet.components.find((c) => c.id === id);
  await invoke(page, "schematic_apply", {
    viewSheet: 1,
    command: { type: "set_component", sheet: 1, id, reference, value: value ?? created.value },
  });
  const definition = result.view.symbols[symbol];
  for (const [number, name] of Object.entries(pins)) {
    const pin = definition.pins.find((p) => p.number === number);
    assert(pin, `${reference} has no pin ${number}`);
    await invoke(page, "schematic_apply", {
      viewSheet: 1,
      command: {
        type: "add_label",
        sheet: 1,
        at: { x: at.x + pin.at.x, y: at.y + pin.at.y },
        name,
        kind: "global",
      },
    });
  }
  return id;
}

const SOURCE = "DC 10 AC 1 PULSE(0 10 0 1u 1u 1 2)";
const run = (analysis, supplies = {}) => invoke(page, "sim_run", { supplies, analysis });
const trace = (outcome, name) => outcome.result.traces.find((item) => item.name === name).values;
const at = (outcome, x) => {
  let best = 0;
  outcome.result.x.forEach((value, index) => {
    if (Math.abs(value - x) < Math.abs(outcome.result.x[best] - x)) best = index;
  });
  return best;
};

/** Samples the canvas: how many of a grid's points differ from the background, and a fingerprint. */
const sample = (selector = ".view3d-canvas") =>
  page.evaluate((query) => {
    const canvas = document.querySelector(query);
    const gl = canvas.getContext("webgl2");
    const pixels = new Uint8Array(4);
    let foreground = 0;
    let print = 0;
    const colors = new Set();
    for (let i = 1; i < 40; i++) {
      for (let j = 1; j < 40; j++) {
        gl.readPixels(Math.floor((canvas.width * i) / 40), Math.floor((canvas.height * j) / 40), 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        const [r, g, b] = pixels;
        if (Math.abs(r - 28) + Math.abs(g - 33) + Math.abs(b - 38) > 12) foreground += 1;
        colors.add(`${r >> 3},${g >> 3},${b >> 3}`);
        print = (print * 31 + r + g * 3 + b * 7) % 1000003;
      }
    }
    return { foreground, print, colors: colors.size, width: canvas.width, height: canvas.height };
  }, selector);

const plotInk = () =>
  page.evaluate(() =>
    [...document.querySelectorAll(".sim-plot")].map((canvas) => {
      const { data } = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height);
      let colored = 0;
      for (let i = 0; i < data.length; i += 4) {
        const spread = Math.max(data[i], data[i + 1], data[i + 2]) - Math.min(data[i], data[i + 1], data[i + 2]);
        if (data[i + 3] > 200 && spread > 60) colored += 1;
      }
      return colored;
    }),
  );

let exitCode = 0;
try {
  await step("builds an RC divider from real library parts", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Teiler");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    await place("V1", "sim:vsource", SOURCE, { 1: "IN", 2: "GND" });
    await place("R1", "device:resistor", "10k", { 1: "IN", 2: "OUT" });
    await place("R2", "device:resistor", "10k", { 1: "OUT", 2: "GND" });
    await place("C1", "device:capacitor", "1u", { 1: "OUT", 2: "GND" });
  });

  await step("simulation and 3D are real modules", async () => {
    assert((await page.locator(".rail-item-planned").count()) === 0, "no module says planned");
    await module("Simulation");
    await page.getByRole("button", { name: "Simulation starten" }).waitFor();
  });

  await step("operating point in the interface: the divider gives 5 V at DC", async () => {
    await page.getByRole("button", { name: "Simulation starten" }).click();
    const cell = page.locator('[data-trace="V(OUT)"]');
    await cell.waitFor();
    // The pulse source starts at 0 V; its DC value applies to the operating point.
    const text = await cell.textContent();
    assert(text === "5 V", `V(OUT) shows "${text}"`);
    assert((await page.locator('[data-trace="V(IN)"]').textContent()) === "10 V", "V(IN)");
    // 10 V across 20 kΩ.
    const current = await page.locator('[data-trace="I(R1)"]').textContent();
    assert(current === "500 µA", `I(R1) shows "${current}"`);
    await shot("op");
  });

  await step("the deck and the models used can be read", async () => {
    const deck = await page.locator(".sim-deck").textContent();
    for (const expected of ["R", "10000", "0.000001", "PULSE"]) assert(deck.includes(expected), `deck lacks ${expected}:\n${deck}`);
    const models = await page.locator(".sim-table").last().textContent();
    assert(models.includes("R1") && models.includes("C1"), models);
  });

  await step("transient: τ = 5 ms, hand-calculated 10/2·(1−e^−t/τ)", async () => {
    const outcome = await run({ type: "transient", stop: "25m", step: "10u" });
    const out = trace(outcome, "V(OUT)");
    for (const [t, expected] of [
      [5e-3, 5 * (1 - Math.exp(-1))],
      [10e-3, 5 * (1 - Math.exp(-2))],
      [25e-3, 5 * (1 - Math.exp(-5))],
    ]) {
      near(out[at(outcome, t)], expected, 0.01, `V(OUT) at ${t}`);
    }
  });

  await step("transient in the interface draws curves, toggles them and reads them out", async () => {
    await page.getByLabel("Analyse").selectOption("transient");
    await page.getByLabel("Dauer").fill("25m");
    await page.getByLabel("Zeitschritt").fill("10u");
    await page.getByRole("button", { name: "Simulation starten" }).click();
    await page.locator(".sim-plot").first().waitFor();
    await settled();
    const ink = await plotInk();
    assert(ink.length === 2, `${ink.length} plots, one for volts and one for amperes`);
    assert(ink.every((count) => count > 200), `curves too thin: ${ink}`);
    const plot = page.locator(".sim-plot").first();
    const box = await plot.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.locator(".sim-readout").first().filter({ hasText: "V(OUT)" }).waitFor();
    await shot("transient");
    await page.getByRole("checkbox", { name: "V(OUT)" }).uncheck();
    await settled();
    assert((await plotInk())[0] < ink[0], "hiding a curve must remove ink");
    await page.getByRole("checkbox", { name: "V(OUT)" }).check();
  });

  await step("results save as CSV that any program can read", async () => {
    const csvPath = join(root, "ergebnis.csv");
    await answerDialogsWith(page, csvPath);
    await page.getByRole("button", { name: "Ergebnisse als CSV speichern" }).click();
    await page.getByText("Gespeichert:").waitFor();
    const lines = readFileSync(csvPath, "utf8").trim().split("\n");
    assert(lines[0].startsWith("x [s],"), lines[0]);
    const columns = lines[0].split(",");
    const out = columns.indexOf("V(OUT) [V]");
    assert(out > 0, lines[0]);
    const last = lines[lines.length - 1].split(",");
    near(Number(last[0]), 0.025, 1e-9, "last time");
    near(Number(last[out]), 5 * (1 - Math.exp(-5)), 0.01, "last value");
  });

  await step("the SPICE deck saves to a file", async () => {
    const deckPath = join(root, "kontrolle.cir");
    await answerDialogsWith(page, deckPath);
    await page.getByRole("button", { name: "SPICE-Netzliste speichern" }).click();
    await page.getByText("kontrolle.cir").waitFor();
    assert(readFileSync(deckPath, "utf8").includes("PULSE"), "deck content");
  });

  await step("AC: corner at 31.83 Hz, half of AC 1 at low frequency, -45° at the corner", async () => {
    const outcome = await run({ type: "ac", start: "1", stop: "10k", points_per_decade: 50 });
    const magnitude = trace(outcome, "V(OUT)");
    const phase = trace(outcome, "P(OUT)");
    near(magnitude[0], 0.5, 0.001, "low frequency magnitude");
    const corner = at(outcome, 31.83);
    near(magnitude[corner], 0.5 / Math.SQRT2, 0.01, "magnitude at the corner");
    near(phase[corner], -45, 2, "phase at the corner");
    near(magnitude[magnitude.length - 1], 0.5 / Math.hypot(1, 10000 / 31.83), 0.0002, "high frequency magnitude");
  });

  await step("AC in the interface shows dB and phase on a logarithmic axis", async () => {
    await page.getByLabel("Analyse").selectOption("ac");
    await page.getByRole("button", { name: "Simulation starten" }).click();
    await page.getByRole("checkbox", { name: "Betrag in dB" }).waitFor();
    await settled();
    const ink = await plotInk();
    assert(ink.length >= 2, `${ink.length} plots`);
    assert(ink.every((count) => count > 100), `${ink}`);
    await shot("ac");
  });

  await step("DC sweep: the output follows at half the input", async () => {
    const outcome = await run({ type: "dc_sweep", source: "V1", start: "0", stop: "10", step: "1" });
    const out = trace(outcome, "V(OUT)");
    assert(out.length === 11, `${out.length} points`);
    out.forEach((value, index) => near(value, index / 2, 1e-4, `V(OUT) at ${index} V`));
    await page.getByLabel("Analyse").selectOption("dc_sweep");
    assert((await page.getByLabel("Variierte Quelle").inputValue()) === "V1", "swept source");
    await page.getByLabel("Ende", { exact: true }).fill("10");
    await page.getByRole("button", { name: "Simulation starten" }).click();
    await page.locator(".sim-plot").first().waitFor();
    await shot("sweep");
  });

  await step("a wrong entry names the value instead of running", async () => {
    await page.getByLabel("Schritt", { exact: true }).fill("viel");
    await page.getByRole("button", { name: "Simulation starten" }).click();
    await page.getByRole("alert").filter({ hasText: "„viel“ ist kein gültiger Wert." }).waitFor();
    await page.getByLabel("Schritt", { exact: true }).fill("0.5");
  });

  await step("a part without a model is named and a supply must be given", async () => {
    await place("U1", "timer:ne555", undefined, { 1: "GND" });
    const supply = await invoke(page, "library_list");
    const symbol = supply.find((item) => item.category === "supply" && item.id !== "power:gnd" && !/gnd/i.test(item.id));
    const id = await place("#PWR1", symbol.id, "VSOLAR", { 1: "SOL" });
    await module("Projekt");
    await module("Simulation");
    await page.getByText(/Nicht simuliert, weil kein Modell vorhanden ist: U1/).waitFor();
    await page.getByText("Spannung fehlt für: VSOLAR").waitFor();
    const start = page.getByRole("button", { name: "Simulation starten" });
    assert(await start.isDisabled(), "must not run without a supply voltage");
    await page.getByLabel("VSOLAR").fill("12");
    await page.waitForFunction(() => ![...document.querySelectorAll(".sim-warning")].some((el) => el.textContent.includes("VSOLAR")));
    assert(await start.isEnabled(), "may run now");
    const refused = await page.evaluate(
      (analysis) => window.__TAURI_INTERNALS__.invoke("sim_run", { supplies: {}, analysis }).catch((error) => error),
      { type: "operating_point" },
    );
    assert(refused.code === "sim.incomplete", `backend refuses too: ${JSON.stringify(refused).slice(0, 200)}`);
    const accepted = await run({ type: "operating_point" }, { VSOLAR: "12" });
    near(trace(accepted, "V(OUT)")[0], 5, 1e-6, "still 5 V beside the rest");
    await shot("report");
    // The supply goes again; the 555 stays for the board that follows.
    await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "delete", sheet: 1, ids: [id] } });
  });

  await step("the simulation form fits in all four languages", async () => {
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      await settled();
      await assertNoOverflow(page, `simulation ${language}`);
    }
  });

  await step("sets up a board with two parts that collide", async () => {
    await invoke(page, "pcb_sync");
    await invoke(page, "pcb_apply", {
      command: { type: "set_outline", points: [{ x: 0, y: 0 }, { x: 50 * MM, y: 0 }, { x: 50 * MM, y: 40 * MM }, { x: 0, y: 40 * MM }] },
    });
    const { board } = await invoke(page, "pcb_view");
    const move = async (reference, x, y) => {
      const part = board.footprints.find((item) => item.reference === reference);
      assert(part, `${reference} on the board`);
      await invoke(page, "pcb_apply", {
        command: { type: "move", ids: [part.id], delta: { x: x * MM - part.at.x, y: y * MM - part.at.y } },
      });
    };
    await move("R1", 15, 15);
    await move("R2", 15, 15);
    await move("C1", 35, 25);
    await move("U1", 30, 15);
    await module("3D-Ansicht");
    await page.locator(".view3d-canvas").waitFor();
    await page.waitForFunction(() => document.querySelector(".erc-summary"));
    await settled();
  });

  await step("the collision is reported, in words and in the picture", async () => {
    await page.getByRole("button", { name: "R1 und R2 kollidieren." }).waitFor();
    const before = await sample();
    assert(before.foreground > 200, `only ${before.foreground} pixels show anything`);
    assert(before.colors > 8, `only ${before.colors} colours`);
    await shot("collision");
  });

  await step("views differ: top, bottom, front, isometric", async () => {
    const prints = new Map();
    for (const name of ["Oben", "Unten", "Vorne", "Seite", "Isometrisch"]) {
      await page.getByRole("button", { name, exact: true }).click();
      await settled();
      const seen = await sample();
      // Seen from the edge the thin board covers only a few points of the grid.
      const needed = name === "Vorne" || name === "Seite" ? 3 : 200;
      assert(seen.foreground >= needed, `${name}: only ${seen.foreground} pixels`);
      prints.set(name, seen.print);
      await shot(`view-${name.toLowerCase()}`);
    }
    assert(new Set(prints.values()).size === 5, `views look alike: ${[...prints]}`);
  });

  await step("copper is painted on the side it belongs to", async () => {
    const look = async (name) => {
      await page.getByRole("button", { name, exact: true }).click();
      await settled();
      return (await sample()).print;
    };
    const top = await look("Oben");
    const bottom = await look("Unten");
    const track = { type: "add_tracks", net: "OUT", layer: 1, width: 3 * MM };
    await invoke(page, "pcb_apply", { command: { ...track, points: [{ x: 5 * MM, y: 35 * MM }, { x: 45 * MM, y: 35 * MM }] } });
    await module("Projekt");
    await module("3D-Ansicht");
    await page.locator(".erc-summary").waitFor();
    await settled();
    assert((await look("Unten")) !== bottom, "a bottom track must show from below");
    await shot("bottom-track");
    assert((await look("Oben")) === top, "a bottom track must not show from above");
    await page.getByRole("button", { name: "Isometrisch" }).click();
    await settled();
  });

  await step("the mouse rotates, moves and zooms; keys rotate too", async () => {
    const canvas = page.locator(".view3d-canvas");
    const box = await canvas.boundingBox();
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const start = await sample();
    await page.mouse.move(center.x, center.y);
    await page.mouse.down();
    await page.mouse.move(center.x + 120, center.y + 40, { steps: 4 });
    await page.mouse.up();
    await settled();
    const turned = await sample();
    assert(turned.print !== start.print, "dragging must rotate");
    await page.mouse.move(center.x, center.y);
    await page.mouse.wheel(0, -800);
    await settled();
    const zoomed = await sample();
    assert(zoomed.print !== turned.print, "the wheel must zoom");
    assert(zoomed.foreground > turned.foreground, `zooming in fills more: ${turned.foreground} -> ${zoomed.foreground}`);
    await page.mouse.move(center.x, center.y);
    await page.mouse.down({ button: "right" });
    await page.mouse.move(center.x + 60, center.y, { steps: 3 });
    await page.mouse.up({ button: "right" });
    await settled();
    const moved = await sample();
    assert(moved.print !== zoomed.print, "right-dragging must move");
    await page.keyboard.press("ArrowLeft");
    await settled();
    assert((await sample()).print !== moved.print, "keys must rotate");
    await page.getByRole("button", { name: "Isometrisch" }).click();
    await settled();
  });

  await step("perspective and parts can be switched off", async () => {
    const start = await sample();
    await page.getByRole("checkbox", { name: "Perspektive" }).uncheck();
    await settled();
    const flat = await sample();
    assert(flat.print !== start.print, "projection must change the picture");
    await page.getByRole("checkbox", { name: "Perspektive" }).check();
    await page.getByRole("checkbox", { name: "Bauteile zeigen" }).uncheck();
    await settled();
    assert((await sample()).print !== start.print, "hiding parts must change the picture");
    await shot("no-parts");
    await page.getByRole("checkbox", { name: "Bauteile zeigen" }).check();
  });

  await step("moving a part away clears the collision", async () => {
    const { board } = await invoke(page, "pcb_view");
    const part = board.footprints.find((item) => item.reference === "R2");
    await invoke(page, "pcb_apply", { command: { type: "move", ids: [part.id], delta: { x: 0, y: 12 * MM } } });
    await module("Projekt");
    await module("3D-Ansicht");
    await page.locator(".erc-summary").waitFor();
    assert((await page.getByRole("button", { name: "R1 und R2 kollidieren." }).count()) === 0, "still collides");
  });

  await step("a height limit from the mechanics tab reports too-tall parts", async () => {
    await page.getByRole("tab", { name: "Mechanik" }).click();
    await page.getByLabel("Höhengrenze oben, mm").fill("0,1");
    await page.getByRole("button", { name: "Übernehmen" }).click();
    await page.getByRole("tab", { name: "Prüfung" }).click();
    await page.getByText(/die Grenze ist 0.10 mm/).first().waitFor();
    await page.getByRole("tab", { name: "Mechanik" }).click();
    await page.getByLabel("Höhengrenze oben, mm").fill("");
    await page.getByRole("button", { name: "Übernehmen" }).click();
  });

  await step("imports an STL file; height and picture change, the check counts it", async () => {
    const stl = join(root, "tall.stl");
    writeFileSync(stl, "solid t\nvertex -1 -1 0\nvertex 1 -1 0\nvertex 0 1 20\nendsolid t\n");
    await page.getByRole("tab", { name: "Bauteile", exact: true }).click();
    const before = await sample();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).click();
    await answerDialogsWith(page, stl);
    await page.getByRole("button", { name: "Modell importieren" }).click();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).filter({ hasText: "tall.stl" }).waitFor();
    // A model belongs to the footprint: R1 and R2 share the 0805 of C1.
    assert((await page.locator(".view3d-parts tr", { hasText: "tall.stl" }).count()) === 3, "same footprint, same model");
    const row = await page.locator(".view3d-parts tr", { hasText: "C1" }).textContent();
    assert(row.includes("20") && row.includes("tall.stl"), row);
    await settled();
    assert((await sample()).print !== before.print, "the model must appear");
    await shot("model");
  });

  await step("STEP is refused with an honest reason, a damaged file with another", async () => {
    const step21 = join(root, "teil.step");
    writeFileSync(step21, "ISO-10303-21;\n");
    await answerDialogsWith(page, step21);
    await page.getByRole("button", { name: "Modell importieren" }).click();
    await page.getByText(/STEP-Dateien können noch nicht gelesen werden/).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    const broken = join(root, "kaputt.obj");
    writeFileSync(broken, "v 1 2\nf 1 2 3\n");
    await answerDialogsWith(page, broken);
    await page.getByRole("button", { name: "Modell importieren" }).click();
    await page.getByText(/beschädigt oder kein gültiges Modell/).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
  });

  await step("the model and the mechanics survive saving and reopening", async () => {
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    const raw = JSON.parse(readFileSync(projectPath, "utf8"));
    assert(raw.header?.format_version === 4 || raw.format_version === 4, "format version 4");
    await module("Projekt");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Öffnen/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    const scene = await invoke(page, "view3d_scene");
    assert(scene.parts.find((part) => part.reference === "C1").model_name === "tall.stl", "model after reopening");
    assert(scene.rules.thickness === 1_600_000, "thickness");
  });

  await step("removing the model goes back to the built-in body", async () => {
    await module("3D-Ansicht");
    await page.getByRole("tab", { name: "Bauteile", exact: true }).click();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).click();
    await page.getByRole("button", { name: "Modell entfernen" }).click();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).filter({ hasText: "Körper" }).waitFor();
  });

  await step("measures a model at the limit and a long simulation", async () => {
    const triangles = 200_000;
    const bytes = Buffer.alloc(84 + triangles * 50);
    bytes.writeUInt32LE(triangles, 80);
    for (let i = 0; i < triangles; i++) {
      const base = 84 + i * 50 + 12;
      const angle = (i / triangles) * Math.PI * 40;
      const radius = 1 + (i / triangles) * 2;
      const corners = [
        [radius * Math.cos(angle), radius * Math.sin(angle), 0],
        [radius * Math.cos(angle + 0.01), radius * Math.sin(angle + 0.01), 0],
        [radius * Math.cos(angle), radius * Math.sin(angle), 3],
      ];
      corners.flat().forEach((value, index) => bytes.writeFloatLE(value, base + index * 4));
    }
    const big = join(root, "gross.stl");
    writeFileSync(big, bytes);
    await answerDialogsWith(page, big);
    const started = Date.now();
    await page.getByRole("button", { name: "Modell importieren" }).click();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).filter({ hasText: "gross.stl" }).waitFor();
    const imported = Date.now() - started;
    await settled();
    const frame = await page.evaluate(async () => {
      const canvas = document.querySelector(".view3d-canvas");
      canvas.focus();
      const times = [];
      for (let i = 0; i < 20; i++) {
        const begin = performance.now();
        canvas.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
        await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
        times.push(performance.now() - begin);
      }
      times.sort((a, b) => a - b);
      return times[10];
    });
    const saveStart = Date.now();
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    const saved = Date.now() - saveStart;
    await page.getByRole("button", { name: "Modell entfernen" }).click();
    await page.locator(".view3d-parts tr", { hasText: "C1" }).filter({ hasText: "Körper" }).waitFor();
    const simStart = Date.now();
    await run({ type: "transient", stop: "1", step: "10u" });
    const simulated = Date.now() - simStart;
    console.log(`      ${triangles} triangles (3 parts): import ${imported} ms, median two frames ${frame.toFixed(1)} ms, save ${saved} ms; 100000 steps RC: ${simulated} ms`);
  });

  await step("a lost graphics context says so and comes back", async () => {
    await page.getByRole("tab", { name: "Prüfung" }).click();
    await page.evaluate(() => {
      const gl = document.querySelector(".view3d-canvas").getContext("webgl2");
      window.__lose = gl.getExtension("WEBGL_lose_context");
      window.__lose.loseContext();
    });
    await page.getByRole("alert").filter({ hasText: "Grafikkontext" }).waitFor();
    await page.evaluate(() => window.__lose.restoreContext());
    await page.getByRole("alert").filter({ hasText: "Grafikkontext" }).waitFor({ state: "detached" });
    await settled();
    await settled();
    const seen = await sample();
    assert(seen.foreground > 200, `only ${seen.foreground} pixels after restoring`);
  });

  await step("the 3D view fits in all four languages", async () => {
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      for (const index of [0, 1, 2]) {
        await page.getByRole("tab").nth(index).click();
        await settled();
        await assertNoOverflow(page, `3D ${language} tab ${index}`);
      }
      await shot(`lang-${language}`);
    }
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });
} catch {
  await page.screenshot({ path: join(artifacts, "sim3d-failure.png") }).catch(() => undefined);
  exitCode = 1;
} finally {
  await app.browser.close().catch(() => undefined);
  try {
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // The process has already ended.
  }
  await wait(600);
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "sim3d-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
