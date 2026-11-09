// End-to-end test of the schematic editor: drawn with real mouse and keyboard
// input in the real application, checked against the real backend and file.
// Also records performance numbers for 10 000 objects (ROADMAP phase 2).
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9335;
const dataDir = useIsolatedData();
const GRID = 1_270_000;
const problems = [];
const results = [];
const root = mkdtempSync(join(tmpdir(), "troniccad-sch-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "Schaltung.tronic");

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

let app = await launch(PORT, problems);
let { page } = app;
const shot = (name) => page.screenshot({ path: join(artifacts, `sch-${name}.png`) });

const SCALE = 14;
/** Screen position of a grid point on a freshly fitted, empty sheet. */
async function screen(x, y) {
  const box = await page.locator(".editor-canvas").boundingBox();
  return {
    x: box.x + box.width / 2 + x * SCALE,
    y: box.y + box.height / 2 + y * SCALE,
  };
}
async function click(x, y) {
  const p = await screen(x, y);
  await page.mouse.move(p.x, p.y);
  await page.mouse.click(p.x, p.y);
}
const settled = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 30))));
async function pick(name) {
  await page.getByRole("button", { name, exact: true }).click();
}
async function drawWire(points) {
  await page.keyboard.press("w");
  for (const [x, y] of points) {
    await click(x, y);
    await settled();
  }
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await settled();
}
const netlist = async () => (await invoke(page, "schematic_netlist")).text;
const view = async (sheet = 1) => invoke(page, "schematic_view", { sheet });
const nets = async () =>
  (await netlist())
    .split("[nets]\n")[1]
    .trim()
    .split("\n")
    .map((line) => line.split("\t"));
const netOf = async (node) => (await nets()).find(([, nodes]) => nodes.split(" ").includes(node));
const undo = () => page.keyboard.press("Control+z").then(settled);

let exitCode = 0;
try {
  await step("opens the editor for a new project", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("LED-Schaltung");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    await page.getByRole("button", { name: "Schaltplan öffnen" }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
  });

  await step("places parts with the mouse and shows them with references", async () => {
    await pick("Widerstand");
    await click(0, 0);
    await settled();
    await pick("LED");
    await click(0, 10);
    await settled();
    await pick("+5V");
    await click(0, -8);
    await settled();
    await pick("GND");
    await click(0, 18);
    await settled();
    await page.keyboard.press("Escape");
    await settled();
    const { sheet } = await view();
    assert(sheet.components.length === 4, `expected 4 components, got ${sheet.components.length}`);
    const refs = sheet.components.map((c) => c.reference).sort();
    assert(refs.includes("R1") && refs.includes("D1"), refs.join());
    const resistor = sheet.components.find((c) => c.reference === "R1");
    assert(resistor.at.x === 0 && resistor.at.y === 0, "placed off target");
  });

  await step("draws wires and the netlist reflects them", async () => {
    await drawWire([[0, -8], [0, -4]]);
    await drawWire([[0, 4], [0, 6]]);
    await drawWire([[0, 14], [0, 18]]);
    const text = await netlist();
    const expected = ["+5V\tR1.1", "GND\tD1.2", "Net-D1-1\tD1.1 R1.2"];
    for (const line of expected) assert(text.includes(line), `missing "${line}" in:\n${text}`);
    await shot("01-led-circuit");
  });

  await step("the rule check accepts the clean circuit", async () => {
    await page.getByRole("button", { name: "ERC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
  });

  await step("the rule check finds a removed wire and marks it in the graphic", async () => {
    await undo();
    await page.getByRole("button", { name: "ERC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText(/Fehler/).waitFor();
    const list = await page.locator(".erc-item").allTextContents();
    assert(list.some((text) => text.includes("Pin D1.2 ist nicht verbunden.")), list.join(" | "));
    assert(list.some((text) => text.includes("Versorgungssymbol GND")), list.join(" | "));
    await page.locator(".erc-item", { hasText: "Pin D1.2" }).click();
    await settled();
    await shot("02-erc-findings");
    await page.keyboard.press("Control+y");
    await settled();
    await page.locator(".review-note", { hasText: "seit dieser Prüfung" }).waitFor();
    await page.keyboard.press("f");
    await settled();
    await page.getByRole("button", { name: "ERC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
  });

  await step("selects everything inside a box drawn with the mouse", async () => {
    await page.keyboard.press("s");
    await page.getByRole("tab", { name: "Eigenschaften" }).click();
    // A fresh camera on the known layout keeps the coordinates simple.
    await page.getByRole("tab", { name: "1", exact: true }).click();
    await settled();
    const box = await page.locator(".editor-canvas").boundingBox();
    // Sheet fitted: find R1 through a box selection around the whole drawing.
    await page.mouse.move(box.x + 5, box.y + 5);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width - 5, box.y + box.height - 5, { steps: 4 });
    await page.mouse.up();
    await settled();
    await page.locator(".side-content", { hasText: /Elemente ausgewählt/ }).waitFor();
    const count = await page.locator(".side-content").textContent();
    assert(count.includes("7 Elemente"), `box selection: ${count}`);
    await page.keyboard.press("Escape");
    await settled();
  });

  await step("undo and redo work from the toolbar", async () => {
    const before = await netlist();
    await page.getByRole("button", { name: "Rückgängig (Strg+Z)" }).click();
    await settled();
    assert((await netlist()) !== before, "undo did nothing");
    await page.getByRole("button", { name: "Wiederholen (Strg+Y)" }).click();
    await settled();
    assert((await netlist()) === before, "redo did not restore");
  });

  // New sheet with a known camera for the remaining interactions.
  let page2;
  await step("adds a sheet, global labels cross-reference both sheets", async () => {
    await page.getByRole("button", { name: "Seite hinzufügen" }).click();
    await page.getByRole("dialog").getByRole("textbox").fill("Ausgang");
    await page.getByRole("dialog").getByRole("button", { name: "OK" }).click();
    await page.getByRole("tab", { name: "Ausgang" }).waitFor();
    await page.waitForFunction(() => document.querySelector(".sheet-tab-active")?.textContent.includes("Ausgang"));
    await settled();
    page2 = (await view(1)).sheets.find((sheet) => sheet.name === "Ausgang").id;

    await pick("Kondensator");
    await click(0, 0);
    await settled();
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");

    await page.keyboard.press("l");
    await click(0, -4);
    await page.getByRole("dialog").getByLabel("Name").fill("TAKT");
    await page.getByRole("dialog").getByLabel("Art").selectOption("global");
    await page.getByRole("dialog").getByRole("button", { name: "OK" }).click();
    await settled();
    await page.keyboard.press("Escape");

    // Add the same global label to the first sheet through the backend, on R1.2.
    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "add_label", sheet: 1, at: { x: 0, y: 4 * GRID }, name: "TAKT", kind: "global" },
    });
    const data = await view(page2);
    assert(data.cross_references.length === 1, "no cross reference");
    assert(data.cross_references[0].sheets[0] === "1", JSON.stringify(data.cross_references));
    const net = await netOf("C1.1");
    assert(net[0] === "TAKT" && net[1].includes("R1.2"), JSON.stringify(net));
  });

  await step("selects a part by click and edits it in the properties", async () => {
    await page.keyboard.press("s");
    await page.getByRole("tab", { name: "Eigenschaften" }).click();
    await click(0, 0);
    await page.getByRole("textbox", { name: "Referenz" }).waitFor();
    assert((await page.getByRole("textbox", { name: "Referenz" }).inputValue()) === "C1", "wrong part selected");
    await page.locator(".review-note", { hasText: "nicht gegen ein Datenblatt" }).waitFor();
    await page.getByRole("textbox", { name: "Wert" }).fill("220n");
    await page.getByRole("textbox", { name: "Wert" }).press("Enter");
    await settled();
    const c1 = (await view(page2)).sheet.components.find((c) => c.reference === "C1");
    assert(c1.value === "220n", `value ${c1.value}`);
  });

  await step("drags a part with the mouse and keeps its connection", async () => {
    await pick("Widerstand");
    await click(8, 8); // R2.1 at (8, 4)
    await settled();
    await page.keyboard.press("Escape");
    await drawWire([[0, 4], [8, 4]]);
    await page.keyboard.press("s");
    assert((await netOf("C1.2"))[1].includes("R2.1"), "wire does not connect");

    const from = await screen(0, 0);
    const to = await screen(-6, 2);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 6 });
    await page.mouse.up();
    await settled();
    const c1 = (await view(page2)).sheet.components.find((c) => c.reference === "C1");
    assert(c1.at.x === -6 * GRID && c1.at.y === 2 * GRID, JSON.stringify(c1.at));
    assert((await netOf("C1.2"))[1].includes("R2.1"), "drag broke the connection");
    await shot("07-after-drag");
    await undo();
    const back = (await view(page2)).sheet.components.find((c) => c.reference === "C1");
    assert(back.at.x === 0 && back.at.y === 0, "undo did not restore the position");
  });

  await step("rotates and mirrors with the keyboard, undo restores", async () => {
    await click(0, 0);
    await settled();
    const before = (await view(page2)).sheet.components.find((c) => c.reference === "C1");
    await page.keyboard.press("r");
    await settled();
    const rotated = (await view(page2)).sheet.components.find((c) => c.id === before.id);
    assert(rotated.rotation === 90, `rotation ${rotated.rotation}`);
    assert((await netOf("C1.2"))[1].includes("R2.1"), "rotating broke the connection");
    await page.keyboard.press("m");
    await settled();
    const mirrored = (await view(page2)).sheet.components.find((c) => c.id === before.id);
    assert(mirrored.mirror === true, "not mirrored");
    await undo();
    await undo();
    const restored = (await view(page2)).sheet.components.find((c) => c.id === before.id);
    assert(restored.rotation === 0 && !restored.mirror, "undo did not restore");
    await page.keyboard.press("Escape");
  });

  await step("pans with the middle mouse button", async () => {
    const start = await screen(10, 10);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(start.x + 3 * SCALE, start.y, { steps: 3 });
    await page.mouse.up({ button: "middle" });
    await page.keyboard.press("j");
    await click(0, -10); // now three grid units further left
    await settled();
    await page.keyboard.press("Escape");
    const { sheet } = await view(page2);
    const junction = sheet.junctions.at(-1);
    assert(junction.at.x === -3 * GRID && junction.at.y === -10 * GRID, JSON.stringify(junction));
    await undo();
    await page.mouse.move(start.x, start.y);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(start.x - 3 * SCALE, start.y, { steps: 3 });
    await page.mouse.up({ button: "middle" });
    await settled();
  });

  await step("copies and pastes with new references", async () => {
    await page.keyboard.press("Control+a");
    await settled();
    await page.keyboard.press("Control+c");
    const box = await page.locator(".editor-canvas").boundingBox();
    await page.mouse.move(box.x + 40, box.y + 40);
    await settled();
    await page.keyboard.press("Control+v");
    await settled();
    const { sheet } = await view(page2);
    const refs = sheet.components.map((c) => c.reference).filter((r) => r.startsWith("C")).sort();
    assert(refs.join() === "C1,C2", refs.join());
    await page.keyboard.press("Control+z");
    await settled();
  });

  let child;
  await step("creates a hierarchical block and connects through its port", async () => {
    await page.getByRole("button", { name: "Hierarchischer Block" }).click();
    const box = await page.locator(".editor-canvas").boundingBox();
    await page.mouse.click(box.x + box.width * 0.7, box.y + box.height * 0.3);
    await page.getByRole("dialog").getByRole("textbox").fill("Filter");
    await page.getByRole("dialog").getByRole("button", { name: "OK" }).click();
    await page.getByRole("tab", { name: /Filter/ }).waitFor();
    child = (await view(1)).sheets.find((sheet) => sheet.name === "Filter");
    assert(child.parent === page2 && child.depth === 1, JSON.stringify(child));
    const block = (await view(page2)).sheet.instances[0];

    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "place_component", sheet: child.id, symbol: "device:resistor", at: { x: 0, y: 0 } },
    });
    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "add_label", sheet: child.id, at: { x: 0, y: -4 * GRID }, name: "EIN", kind: "hierarchical" },
    });
    const updated = (await view(page2)).sheet.instances[0];
    assert(updated.ports.length === 1 && updated.ports[0].name === "EIN", JSON.stringify(updated.ports));
    const port = { x: updated.at.x + updated.ports[0].at.x, y: updated.at.y + updated.ports[0].at.y };
    const c1 = (await view(page2)).sheet.components.find((c) => c.reference === "C1");
    const pin = { x: c1.at.x, y: c1.at.y + 4 * GRID };
    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "add_wires", sheet: page2, points: [pin, { x: port.x, y: pin.y }, port] },
    });
    const net = await netOf("C1.2");
    const inner = (await view(child.id)).sheet.components[0].reference;
    assert(net && net[1].includes(`${inner}.1`), `not connected through the block: ${JSON.stringify(net)} block ${JSON.stringify(block)}`);
  });

  await step("buses and renumbering work", async () => {
    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "add_buses", sheet: page2, points: [{ x: -20 * GRID, y: 10 * GRID }, { x: -10 * GRID, y: 10 * GRID }] },
    });
    await invoke(page, "schematic_apply", {
      viewSheet: page2,
      command: { type: "add_label", sheet: page2, at: { x: -20 * GRID, y: 10 * GRID }, name: "D[0..3]", kind: "local" },
    });
    await page.getByRole("button", { name: "Referenzen neu nummerieren" }).click();
    await settled();
    const refs = (await view(child.id)).sheet.components.map((c) => c.reference);
    assert(refs.length === 1 && refs[0].startsWith("R"), refs.join());
    await page.getByRole("tab", { name: "Ausgang" }).click();
    await settled();
    await page.locator(".editor-canvas").waitFor();
    await shot("03-sheet-with-block-and-bus");
  });

  await step("a custom part is created, exported and imported again", async () => {
    await page.getByRole("button", { name: "Eigenes Bauteil" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Name").fill("Mein-IC");
    await dialog.locator("textarea").fill("1 VCC power_in left\n2 OUT output right");
    await shot("04-custom-part");
    await dialog.getByRole("button", { name: "Anlegen" }).click();
    await page.getByRole("button", { name: "Mein-IC" }).waitFor();
    const library = join(root, "eigene.troniclib");
    await answerDialogsWith(page, library);
    await page.getByRole("button", { name: "Eigene Bauteile exportieren" }).click();
    await wait(300);
    const exported = JSON.parse(readFileSync(library, "utf8"));
    assert(exported.format === "troniclibrary", "wrong library format");
    assert(exported.symbols.some((symbol) => symbol.name === "Mein-IC"), "part not exported");
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Bibliothek importieren" }).click();
    await wait(300);
    assert(problems.length === 0, problems.join(" | "));
  });

  await step("the schematic survives saving and reopening", async () => {
    const before = await netlist();
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    const stored = JSON.parse(readFileSync(projectPath, "utf8"));
    assert(stored.format_version === 4, `format ${stored.format_version}`);
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await statusIs(page, "Kein Projekt geöffnet");
    await page.getByRole("button", { name: /LED-Schaltung/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    assert((await netlist()) === before, "netlist changed after reopening");
    await page.getByRole("button", { name: "Schaltplan", exact: false }).first().click();
    await page.locator(".editor-canvas").waitFor();
  });

  for (const [code, heading] of [
    ["en-US", "Library"],
    ["fr-FR", "Bibliothèque"],
    ["zh-CN", "元件库"],
    ["de-DE", "Bibliothek"],
  ]) {
    await step(`editor in ${code} has no layout overflow`, async () => {
      await selectLanguage(page, code);
      await page.getByRole("heading", { name: heading, exact: true }).waitFor();
      await settled();
      await assertNoOverflow(page, code);
      await shot(`05-editor-${code}`);
    });
  }

  await step("a tampered schematic is refused", async () => {
    const text = readFileSync(projectPath, "utf8");
    const tampered = join(root, "manipuliert.tronic");
    writeFileSync(tampered, text.replace('"TAKT"', '"XAKT"'));
    await answerDialogsWith(page, tampered);
    await page.getByRole("button", { name: "Öffnen", exact: true }).click();
    await page.getByText("Integritätsprüfung").waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
  });

  await step("measures 10 000 objects", async () => {
    const big = join(root, "gross.tronic");
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await page.waitForFunction(() => !document.querySelector(".editor-canvas"));
    const perf = await page.evaluate(
      async ({ path, GRID }) => {
        const call = window.__TAURI_INTERNALS__.invoke;
        const timed = async (command, args) => {
          const start = performance.now();
          const result = await call(command, args);
          return [performance.now() - start, result];
        };
        await call("project_create", { path, name: "Gross", description: "" });
        await call("schematic_apply", {
          viewSheet: 1,
          command: { type: "place_component", sheet: 1, symbol: "device:resistor", at: { x: 0, y: 0 } },
        });
        const symbol = "device:resistor";
        const components = [];
        const wires = [];
        for (let i = 0; i < 5000; i++) {
          const x = (i % 100) * 8 * GRID;
          const y = Math.floor(i / 100) * 16 * GRID;
          components.push({ id: 0, symbol, reference: "R?", value: "10k", at: { x, y }, rotation: 0, mirror: false });
          wires.push({ id: 0, a: { x, y: y + 4 * GRID }, b: { x, y: y + 12 * GRID } });
        }
        const [pasteMs] = await timed("schematic_apply", {
          viewSheet: 1,
          command: { type: "paste", sheet: 1, clip: { components, wires }, delta: { x: 0, y: 0 } },
        });
        const [editMs, editResult] = await timed("schematic_apply", {
          viewSheet: 1,
          command: { type: "add_no_connect", sheet: 1, at: { x: -GRID, y: -GRID } },
        });
        const bytes = JSON.stringify(editResult).length;
        const [ercMs] = await timed("schematic_erc");
        const [netlistMs] = await timed("schematic_netlist");
        const [saveMs] = await timed("project_save", { overwrite: false });
        return { objects: 10002, pasteMs, editMs, viewBytes: bytes, ercMs, netlistMs, saveMs };
      },
      { path: big, GRID },
    );
    // Show the large sheet in the real editor and measure its frame time.
    await page.getByRole("button", { name: /Schaltplan/ }).first().click().catch(() => undefined);
    await page.reload();
    await page.waitForSelector(".toolbar");
    await page.waitForFunction(() => true);
    await answerDialogsWith(page, big);
    await page.getByRole("button", { name: /Projekt öffnen/ }).click();
    await statusIs(page, "Projekt geöffnet");
    await page.getByRole("button", { name: "Schaltplan öffnen" }).click();
    await page.locator(".editor-canvas").waitFor();
    await settled();
    await page.keyboard.press("f");
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
    await shot("06-10000-objects");
    writeFileSync(join(artifacts, "perf.json"), `${JSON.stringify(perf, null, 2)}\n`);
    console.log(`      ${JSON.stringify(perf)}`);
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });

} catch {
  await page.screenshot({ path: join(artifacts, "sch-failure.png") }).catch(() => undefined);
  exitCode = 1;
} finally {
  await app.browser.close().catch(() => undefined);
  try {
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // The process has already ended.
  }
  await wait(500);
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "schematic-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
