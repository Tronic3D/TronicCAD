// End-to-end test of the board editor: placed and routed with real mouse and
// keyboard input in the real application, checked against backend and files.
// Also records performance numbers for a large board.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, repo, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9337;
const GRID = 1_270_000;
const MM = 1_000_000;
const SCALE = 10; // Pixels per millimetre on an empty board.
const dataDir = useIsolatedData();
const root = mkdtempSync(join(tmpdir(), "troniccad-pcb-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "Platine.tronic");
const exportDir = join(root, "fertigung");
mkdirSync(exportDir);
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
const shot = (name) => page.screenshot({ path: join(artifacts, `pcb-${name}.png`) });
const settled = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 40))));
const board = async () => (await invoke(page, "pcb_view")).board;
const part = async (reference) => (await board()).footprints.find((f) => f.reference === reference);
const padOf = async (reference, number) =>
  (await board()).pads.find((pad) => pad.reference === reference && pad.number === number);

let canvasBox;
let origin; // Position of the board origin inside the canvas, fixed when the empty board opened.
/** Screen position of a board position in millimetres, camera unchanged since the empty start. */
const screen = (x, y) => ({
  x: canvasBox.x + origin.x + x * SCALE,
  y: canvasBox.y + origin.y + y * SCALE,
});
async function click(x, y) {
  const p = screen(x, y);
  await page.mouse.move(p.x, p.y);
  await page.mouse.click(p.x, p.y);
  await settled();
}
async function dragTo(from, to) {
  const a = screen(from.x, from.y);
  const b = screen(to.x, to.y);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
  await page.mouse.up();
  await settled();
}
const mm = (nm) => nm / MM;

async function schematicCommand(command) {
  await invoke(page, "schematic_apply", { viewSheet: 1, command });
}

let exitCode = 0;
try {
  await step("sets up a project with a schematic", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Platine");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    const place = (symbol, x, y) => schematicCommand({ type: "place_component", sheet: 1, symbol, at: { x: x * GRID, y: y * GRID } });
    await place("device:resistor", 0, 0);
    await place("device:led", 0, 10);
    await place("supply:5v", 0, -8);
    await place("supply:gnd", 0, 18);
    for (const [a, b] of [[-8, -4], [4, 6], [14, 18]]) {
      await schematicCommand({ type: "add_wires", sheet: 1, points: [{ x: 0, y: a * GRID }, { x: 0, y: b * GRID }] });
    }
  });

  await step("the board module shows the difference to the schematic", async () => {
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
    canvasBox = await page.locator(".editor-canvas").boundingBox();
    origin = { x: canvasBox.width / 2, y: canvasBox.height / 2 };
    await page.getByText("Die Platine weicht vom Schaltplan ab (2)").waitFor();
  });

  await step("updating from the schematic brings the parts with their nets", async () => {
    await page.getByRole("button", { name: "Aus Schaltplan aktualisieren" }).click();
    await page.getByRole("dialog").getByText("D1, R1").waitFor();
    await shot("01-sync");
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    await settled();
    canvasBox = await page.locator(".editor-canvas").boundingBox();
    const data = await board();
    assert(data.footprints.length === 2, `${data.footprints.length} parts`);
    assert(data.airwires.length === 1, `${data.airwires.length} airwires`);
    assert(data.airwires[0].net === "Net-D1-1", data.airwires[0].net);
    assert((await padOf("R1", "1")).net === "+5V", "R1.1 net");
    assert((await padOf("D1", "2")).net === "GND", "D1.2 net");
    assert(!(await page.locator(".sync-banner").count()), "banner stays after syncing");
    const again = await invoke(page, "pcb_sync");
    assert(again.report.added.length === 0 && again.report.updated.length === 0, "not idempotent");
  });

  await step("draws the outline with clicks", async () => {
    await page.getByRole("button", { name: "Platinenumriss" }).click();
    for (const [x, y] of [[-25, -20], [5, -20], [5, 20], [-25, 20]]) await click(x, y);
    await page.keyboard.press("Enter");
    await settled();
    const data = await board();
    assert(data.outline.length === 4, `outline ${data.outline.length}`);
    assert(data.outline[0].x === -25 * MM && data.outline[2].y === 20 * MM, JSON.stringify(data.outline[0]));
  });

  await step("drags the parts onto the board", async () => {
    await page.keyboard.press("Escape");
    const r1 = await part("R1");
    const d1 = await part("D1");
    await dragTo({ x: mm(r1.at.x), y: mm(r1.at.y) }, { x: -15, y: -5 });
    await dragTo({ x: mm(d1.at.x), y: mm(d1.at.y) }, { x: -5, y: 5 });
    assert((await part("R1")).at.x === -15 * MM, "R1 not moved");
    assert((await part("D1")).at.y === 5 * MM, "D1 not moved");
    await page.keyboard.press("Escape");
    await settled();
  });

  await step("rotates and flips a part, undo restores", async () => {
    const r1 = await part("R1");
    await click(mm(r1.at.x), mm(r1.at.y));
    await page.keyboard.press("r");
    await settled();
    assert((await part("R1")).rotation === 90, "not rotated");
    await page.keyboard.press("f");
    await settled();
    assert((await part("R1")).bottom === true, "not flipped");
    assert((await padOf("R1", "1")).layer === 1, "pad not on the bottom layer");
    await page.keyboard.press("Control+z");
    await settled();
    await page.keyboard.press("Control+z");
    await settled();
    const back = await part("R1");
    assert(back.rotation === 0 && back.bottom === false, "undo did not restore");
    await page.keyboard.press("Escape");
    await settled();
  });

  await step("routes across layers with a via and closes the connection", async () => {
    const from = await padOf("R1", "2");
    const to = await padOf("D1", "1");
    await page.keyboard.press("x");
    await click(mm(from.at.x), mm(from.at.y));
    await click(-11, -5); // Straight along the top.
    await page.keyboard.press("v");
    await settled();
    await click(-11, 2);
    await page.keyboard.press("v");
    await settled();
    await click(mm(to.at.x), mm(to.at.y));
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await settled();
    const data = await board();
    const layers = new Set(data.tracks.map((t) => t.layer));
    assert(layers.has(0) && layers.has(1), `layers ${[...layers]}`);
    assert(data.vias.length === 2, `${data.vias.length} vias`);
    assert(data.airwires.length === 0, JSON.stringify(data.airwires));
    assert(data.tracks.every((t) => t.net === "Net-D1-1"), "wrong net on a track");
    assert(data.net_lengths["Net-D1-1"] > 0, "no net length");
    await shot("02-routed");
  });

  await step("a track started in free space does nothing", async () => {
    const before = (await board()).tracks.length;
    await page.keyboard.press("x");
    await click(-20, 15);
    await click(-18, 15);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    assert((await board()).tracks.length === before, "created copper without a net");
  });

  await step("the design rule check accepts the routed board", async () => {
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
  });

  await step("draws a copper zone with clicks and it fills around other nets", async () => {
    await page.getByRole("button", { name: "Kupferfläche" }).click();
    for (const [x, y] of [[-24, -19], [4, -19], [4, 19], [-24, 19]]) await click(x, y);
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Netz").selectOption("GND");
    await dialog.getByRole("button", { name: "OK" }).click();
    await settled();
    const data = await board();
    assert(data.zones.length === 1 && data.zones[0].net === "GND" && data.zones[0].layer === 0, JSON.stringify(data.zones));
    assert(data.fills[0].shapes.length >= 1, "no fill");
    assert(data.fills[0].shapes[0].length > 1, "fill has no cut-outs");
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
    await shot("02b-zone");
    await page.keyboard.press("Escape");
  });

  await step("a part pushed off the board is found and marked", async () => {
    const r1 = await part("R1");
    await page.keyboard.press("s");
    await dragTo({ x: mm(r1.at.x), y: mm(r1.at.y) }, { x: -24, y: -5 });
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-item", { hasText: "R1 liegt nicht vollständig auf der Platine." }).waitFor();
    await page.locator(".erc-item", { hasText: "nicht vollständig auf der Platine" }).click();
    await settled();
    await shot("03-drc-findings");
    await page.keyboard.press("Control+z");
    await settled();
    await page.locator(".review-note", { hasText: "seit dieser Prüfung" }).waitFor();
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
    await page.keyboard.press("Home");
    await settled();
  });

  await step("the layer count can be changed to every supported value", async () => {
    await page.getByRole("tab", { name: "Regeln" }).click();
    const select = page.getByLabel("Kupferlagen");
    for (const layers of ["4", "6", "8", "2"]) {
      await select.selectOption(layers);
      await settled();
      const data = await board();
      assert(data.copper_layers === Number(layers), `layers ${data.copper_layers}`);
      assert(data.tracks.some((t) => t.layer === Number(layers) - 1), "bottom copper did not follow");
    }
    await page.getByRole("button", { name: /Innen/ }).waitFor({ state: "detached" });
    await select.selectOption("1");
    await page.getByRole("dialog").getByText("liegt Kupfer").waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    await settled();
    assert((await board()).copper_layers === 2, "layers changed despite copper");
  });

  await step("rules can be edited and take effect", async () => {
    const field = page.getByLabel("Minimale Bahnbreite (mm)");
    await field.fill("0,3");
    await page.getByRole("button", { name: "Übernehmen" }).click();
    await settled();
    assert((await board()).rules.min_track_width === 300_000, "rule not applied");
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-item", { hasText: "0.250 mm breit" }).first().waitFor();
    await page.getByRole("button", { name: "Rückgängig (Strg+Z)" }).click();
    await settled();
    assert((await board()).rules.min_track_width === 200_000, "undo did not restore the rule");
  });

  await step("a footprint chosen in the schematic reaches the board", async () => {
    await page.getByRole("button", { name: "Schaltplan", exact: true }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
    const view = await invoke(page, "schematic_view", { sheet: 1 });
    const r1 = view.sheet.components.find((c) => c.reference === "R1");
    await schematicCommand({ type: "set_footprint", sheet: 1, id: r1.id, footprint: "chip:1206" });
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await page.getByText("weicht vom Schaltplan ab").waitFor({ state: "detached" }).catch(() => undefined);
    await page.getByRole("button", { name: "Aus Schaltplan aktualisieren" }).click();
    await page.getByRole("dialog").getByText("Aktualisiert").waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    assert((await part("R1")).footprint === "Chip 1206", (await part("R1")).footprint);
  });

  await step("export writes all files and asks first when the DRC finds errors", async () => {
    await answerDialogsWith(page, exportDir);
    // A clean board exports without asking.
    await page.getByRole("button", { name: "Fertigungsdaten exportieren" }).click();
    await page.getByRole("heading", { name: "Fertigungsdaten geschrieben" }).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();

    // With an error it asks, and cancelling writes nothing new.
    const rules = (await board()).rules;
    await invoke(page, "pcb_apply", { command: { type: "set_rules", rules: { ...rules, min_track_width: 300_000 } } });
    await page.getByRole("button", { name: "Fertigungsdaten exportieren" }).click();
    await page.getByRole("heading", { name: "Der DRC meldet Fehler" }).waitFor();
    await shot("04-export-warning");
    await page.getByRole("dialog").getByRole("button", { name: "Abbrechen" }).click();
    await page.getByRole("button", { name: "Fertigungsdaten exportieren" }).click();
    await page.getByRole("button", { name: "Trotzdem exportieren" }).click();
    await page.getByRole("heading", { name: "Fertigungsdaten geschrieben" }).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    await invoke(page, "pcb_apply", { command: { type: "set_rules", rules } });
    const names = readdirSync(exportDir);
    for (const suffix of ["copper-l1-top.gbr", "copper-l2-bottom.gbr", "mask-top.gbr", "silk-top.gbr", "paste-top.gbr", "outline.gbr", "plated.drl", "nonplated.drl", "bom.csv", "placement.csv"]) {
      assert(names.some((name) => name.endsWith(suffix)), `missing ${suffix}: ${names}`);
    }
    const top = readFileSync(join(exportDir, names.find((n) => n.endsWith("copper-l1-top.gbr"))), "utf8");
    assert(top.includes("%TF.FileFunction,Copper,L1,Top*%") && top.includes("G36*"), "top layer content");
    assert(readFileSync(join(exportDir, names.find((n) => n.endsWith("bom.csv"))), "utf8").includes("R1"), "BOM");
  });

  await step("the exported files parse with an independent reader", async () => {
    const python = process.env.TRONICCAD_VERIFY_PYTHON;
    if (!python) {
      console.log("      skipped: TRONICCAD_VERIFY_PYTHON is not set");
      return;
    }
    const script = join(repo, "scripts", "parse_gerber.py");
    const output = execFileSync(python, [script, exportDir], { encoding: "utf8" });
    assert(output.includes("All files parsed"), output);
  });

  await step("the board survives saving and reopening", async () => {
    const before = JSON.stringify(await board());
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    assert(JSON.parse(readFileSync(projectPath, "utf8")).format_version === 4, "format version");
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await statusIs(page, "Kein Projekt geöffnet");
    await page.getByRole("button", { name: /Platine/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    assert(JSON.stringify(await board()) === before, "board changed after reopening");
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
    await shot("05-reopened");
  });

  for (const [code, heading] of [
    ["en-US", "Layers"],
    ["fr-FR", "Couches"],
    ["zh-CN", "板层"],
    ["de-DE", "Lagen"],
  ]) {
    await step(`the board editor in ${code} has no layout overflow`, async () => {
      await selectLanguage(page, code);
      await page.getByRole("heading", { name: heading, exact: true }).waitFor();
      await settled();
      await assertNoOverflow(page, code);
      await shot(`06-${code}`);
    });
  }

  await step("measures a large board", async () => {
    const perf = await page.evaluate(async ({ MM }) => {
      const call = window.__TAURI_INTERNALS__.invoke;
      const timed = async (command, args) => {
        const start = performance.now();
        const result = await call(command, args);
        return [performance.now() - start, result];
      };
      // 200 x 100 mm with 8 000 track segments in rows on two layers.
      await call("pcb_apply", { command: { type: "set_outline", points: [{ x: -300 * MM, y: -200 * MM }, { x: 300 * MM, y: -200 * MM }, { x: 300 * MM, y: 200 * MM }, { x: -300 * MM, y: 200 * MM }] } });
      for (let row = 0; row < 40; row++) {
        const points = [];
        for (let i = 0; i <= 100; i++) points.push({ x: (-250 + i * 5) * MM, y: (-190 + row * 9) * MM });
        await call("pcb_apply", { command: { type: "add_tracks", net: `N${row}`, layer: row % 2, points, width: 250_000 } });
      }
      for (let row = 0; row < 40; row++) {
        const points = [];
        for (let i = 0; i <= 100; i++) points.push({ x: (-250 + i * 5) * MM, y: (-186 + row * 9) * MM });
        await call("pcb_apply", { command: { type: "add_tracks", net: `M${row}`, layer: row % 2, points, width: 250_000 } });
      }
      const [editMs, edit] = await timed("pcb_apply", { command: { type: "add_via", net: "N0", at: { x: -280 * MM, y: -190 * MM } } });
      const [drcMs] = await timed("pcb_drc");
      const [saveMs] = await timed("project_save", { overwrite: false });
      return { tracks: edit.view.board.tracks.length, editMs, viewBytes: JSON.stringify(edit).length, drcMs, saveMs };
    }, { MM });
    await page.keyboard.press("Home");
    // The editor only sees the new board after a reload of its view.
    await page.getByRole("button", { name: "Projekt", exact: true }).click();
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
    const box = await page.locator(".editor-canvas").boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.evaluate(() => {
      window.__frames = [];
      let last = performance.now();
      const tick = (now) => {
        window.__frames.push(now - last);
        last = now;
        window.__frameRequest = requestAnimationFrame(tick);
      };
      window.__frameRequest = requestAnimationFrame(tick);
    });
    for (let i = 0; i < 60; i++) {
      await page.mouse.wheel(0, i % 2 ? 120 : -120);
      await wait(16);
    }
    const frames = await page.evaluate(() => {
      cancelAnimationFrame(window.__frameRequest);
      return window.__frames.slice(5).sort((a, b) => a - b);
    });
    perf.zoomFrameMedianMs = frames[Math.floor(frames.length / 2)];
    perf.zoomFrameP95Ms = frames[Math.floor(frames.length * 0.95)];
    await shot("07-large");
    writeFileSync(join(artifacts, "pcb-perf.json"), `${JSON.stringify(perf, null, 2)}\n`);
    console.log(`      ${JSON.stringify(perf)}`);
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });
} catch {
  await page.screenshot({ path: join(artifacts, "pcb-failure.png") }).catch(() => undefined);
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
  if (existsSync(dataDir)) cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "pcb-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
