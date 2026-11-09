// End-to-end test of the autorouter in the real application: a real schematic
// through the real sync, routed, previewed, accepted and checked by the DRC.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9338;
const GRID = 1_270_000;
const MM = 1_000_000;
const dataDir = useIsolatedData();
const root = mkdtempSync(join(tmpdir(), "troniccad-rt-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "Blinker.tronic");
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
const shot = (name) => page.screenshot({ path: join(artifacts, `router-${name}.png`) });
const settled = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 40))));
const board = async () => (await invoke(page, "pcb_view")).board;

/** Places parts and joins pins by net labels, all through the schematic backend. */
async function schematic(parts, nets) {
  const ids = {};
  let column = 0;
  for (const [reference, symbol] of parts) {
    const at = { x: column * 40 * GRID, y: 0 };
    column += 1;
    const result = await invoke(page, "schematic_apply", {
      viewSheet: 1,
      command: { type: "place_component", sheet: 1, symbol, at },
    });
    ids[reference] = { id: result.created[0], at };
    const created = result.view.sheet.components.find((c) => c.id === result.created[0]);
    await invoke(page, "schematic_apply", {
      viewSheet: 1,
      command: { type: "set_component", sheet: 1, id: created.id, reference, value: created.value },
    });
    ids[reference].symbol = result.view.symbols[symbol];
  }
  for (const [name, pins] of Object.entries(nets)) {
    for (const [reference, number] of pins) {
      const part = ids[reference];
      const pin = part.symbol.pins.find((p) => p.number === number);
      assert(pin, `${reference} has no pin ${number}`);
      await invoke(page, "schematic_apply", {
        viewSheet: 1,
        command: {
          type: "add_label",
          sheet: 1,
          at: { x: part.at.x + pin.at.x, y: part.at.y + pin.at.y },
          name,
          kind: "global",
        },
      });
    }
  }
}

async function placeOnBoard(positions, width, height) {
  await invoke(page, "pcb_apply", {
    command: { type: "set_outline", points: [{ x: 0, y: 0 }, { x: width * MM, y: 0 }, { x: width * MM, y: height * MM }, { x: 0, y: height * MM }] },
  });
  const current = await board();
  for (const [reference, [x, y]] of Object.entries(positions)) {
    const part = current.footprints.find((f) => f.reference === reference);
    await invoke(page, "pcb_apply", {
      command: { type: "move", ids: [part.id], delta: { x: x * MM - part.at.x, y: y * MM - part.at.y } },
    });
  }
}

const reopenBoardModule = async () => {
  await page.getByRole("button", { name: "Projekt", exact: true }).click();
  await page.getByRole("button", { name: "Leiterplatte" }).click();
  await page.locator(".editor-canvas").waitFor();
  await settled();
};

let exitCode = 0;
try {
  await step("sets up a 555 flasher from schematic to placed board", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Blinker");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    await schematic(
      [
        ["U1", "timer:ne555"],
        ["R1", "device:resistor"],
        ["R2", "device:resistor"],
        ["C1", "device:capacitor"],
        ["C2", "device:capacitor"],
        ["R3", "device:resistor"],
        ["D1", "device:led"],
        ["J1", "connector:1x02"],
      ],
      {
        VCC: [["J1", "1"], ["U1", "8"], ["U1", "4"], ["R1", "1"]],
        GND: [["J1", "2"], ["U1", "1"], ["C1", "2"], ["C2", "2"], ["D1", "2"]],
        DIS: [["U1", "7"], ["R1", "2"], ["R2", "1"]],
        THR: [["U1", "6"], ["U1", "2"], ["R2", "2"], ["C1", "1"]],
        CTRL: [["U1", "5"], ["C2", "1"]],
        OUT: [["U1", "3"], ["R3", "1"]],
        LED: [["R3", "2"], ["D1", "1"]],
      },
    );
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await page.getByRole("button", { name: "Aus Schaltplan aktualisieren" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    await placeOnBoard(
      { U1: [25, 20], R1: [12, 8], R2: [25, 8], C1: [12, 32], C2: [25, 32], R3: [38, 15], D1: [38, 25], J1: [6, 20] },
      50,
      40,
    );
    await reopenBoardModule();
    await page.keyboard.press("Home");
    await settled();
    const open = (await board()).airwires.length;
    assert(open > 5, `only ${open} open connections`);
    await shot("01-unrouted");
  });

  await step("shows a preview without touching the board", async () => {
    const before = await board();
    await page.getByRole("button", { name: "Autorouter" }).click();
    await page.locator(".route-banner").waitFor({ timeout: 60000 });
    const text = await page.locator(".route-banner").textContent();
    const total = before.airwires.length;
    assert(text.includes(`${total} von ${total} Verbindungen`), text);
    await settled();
    const banner = await page.locator(".route-banner").boundingBox();
    const canvas = await page.locator(".editor-canvas").boundingBox();
    assert(banner && banner.height > 10, "banner has no height");
    assert(banner.y + banner.height <= canvas.y + 1, "banner overlaps the canvas");
    await shot("02-preview");
    const during = await board();
    assert(during.tracks.length === before.tracks.length, "the preview changed the board");
  });

  await step("discarding leaves the board as it was", async () => {
    await page.getByRole("button", { name: "Verwerfen" }).click();
    await page.locator(".route-banner").waitFor({ state: "detached" });
    assert((await board()).tracks.length === 0, "tracks after discarding");
  });

  await step("a proposal for an older board state is refused", async () => {
    await page.getByRole("button", { name: "Autorouter" }).click();
    await page.locator(".route-banner").waitFor({ timeout: 60000 });
    const part = (await board()).footprints[0];
    await invoke(page, "pcb_apply", { command: { type: "move", ids: [part.id], delta: { x: MM, y: 0 } } });
    await page.getByRole("button", { name: "Übernehmen" }).click();
    await page.getByRole("dialog").getByText("wurde geändert").waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    assert((await board()).tracks.length === 0, "stale proposal applied");
    await invoke(page, "pcb_apply", { command: { type: "move", ids: [part.id], delta: { x: -MM, y: 0 } } });
    await reopenBoardModule();
  });

  await step("accepting applies it, the DRC finds nothing and all nets are closed", async () => {
    await page.getByRole("button", { name: "Autorouter" }).click();
    await page.locator(".route-banner").waitFor({ timeout: 60000 });
    await page.getByRole("button", { name: "Übernehmen" }).click();
    await page.locator(".route-banner").waitFor({ state: "detached" });
    await settled();
    const data = await board();
    assert(data.tracks.length > 0, "no tracks");
    assert(data.airwires.length === 0, JSON.stringify(data.airwires));
    await page.getByRole("button", { name: "DRC ausführen" }).first().click();
    await page.locator(".erc-summary").getByText("Keine Befunde.").waitFor();
    await shot("03-routed");
    assert(JSON.stringify((await invoke(page, "pcb_view")).deviations) === "[]", "differs from the schematic");
  });

  await step("a single undo removes everything, redo brings it back", async () => {
    const routed = (await board()).tracks.length;
    await page.keyboard.press("Control+z");
    await settled();
    assert((await board()).tracks.length === 0, "undo left tracks");
    await page.keyboard.press("Control+y");
    await settled();
    assert((await board()).tracks.length === routed, "redo lost tracks");
  });

  await step("the routed board survives saving, reopening and exporting", async () => {
    const before = JSON.stringify(await board());
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await statusIs(page, "Kein Projekt geöffnet");
    await page.getByRole("button", { name: /Blinker/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    assert(JSON.stringify(await board()) === before, "board changed");
    const dir = join(root, "fertigung");
    mkdirSync(dir);
    const names = await invoke(page, "pcb_export", { directory: dir });
    assert(names.length >= 10, `${names.length} files`);
    assert(readFileSync(join(dir, names.find((n) => n.endsWith("copper-l1-top.gbr"))), "utf8").includes("D01*"), "no tracks in the Gerber file");
  });

  await step("a large board stays responsive and can be cancelled", async () => {
    // A new project, so the routed one stays intact.
    await page.getByRole("button", { name: "Neu", exact: true }).click();
    await answerDialogsWith(page, join(root, "gross.tronic"));
    await page.getByRole("dialog").getByLabel("Projektname").fill("Gross");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    const parts = [];
    const nets = {};
    for (let i = 1; i <= 16; i++) parts.push([`U${i}`, "timer:ne555"]);
    for (let n = 0; n < 48; n++) {
      nets[`N${n}`] = [[`U${(n % 16) + 1}`, ["1", "2", "3"][Math.floor(n / 16)]], [`U${((n * 7) % 16) + 1}`, ["4", "5", "6"][Math.floor(n / 16)]]];
    }
    await schematic(parts, nets);
    await page.getByRole("button", { name: "Leiterplatte" }).click();
    await page.locator(".editor-canvas").waitFor();
    await page.getByRole("button", { name: "Aus Schaltplan aktualisieren" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Schließen" }).click();
    const positions = {};
    parts.forEach(([reference], index) => {
      positions[reference] = [10 + (index % 4) * 20, 10 + Math.floor(index / 4) * 20];
    });
    await placeOnBoard(positions, 80, 80);
    await reopenBoardModule();

    await page.getByRole("button", { name: "Autorouter" }).click();
    await page.getByRole("heading", { name: "Autorouter läuft" }).waitFor();
    await wait(200);
    // The interface answers while the router works.
    const start = await page.evaluate(() => performance.now());
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(done)));
    const frame = (await page.evaluate(() => performance.now())) - start;
    assert(frame < 500, `interface blocked for ${frame} ms`);
    await page.getByRole("dialog").getByRole("button", { name: "Abbrechen" }).click();
    await page.locator(".route-banner", { hasText: "Abgebrochen" }).waitFor({ timeout: 30000 });
    await shot("04-cancelled");
    const text = await page.locator(".route-banner").textContent();
    assert(text.includes("Offen:"), text);
    await page.getByRole("button", { name: "Verwerfen" }).click();
  });

  for (const [code, label] of [["en-US", "Autorouter"], ["fr-FR", "Autorouteur"], ["zh-CN", "自动布线"]]) {
    await step(`the router in ${code} has no layout overflow`, async () => {
      await selectLanguage(page, code);
      await page.getByRole("button", { name: label, exact: true }).waitFor();
      await page.getByRole("button", { name: label, exact: true }).click();
      await wait(150);
      await assertNoOverflow(page, `${code} progress`);
      await page.getByRole("dialog").getByRole("button").first().click();
      await page.locator(".route-banner").waitFor({ timeout: 30000 });
      await assertNoOverflow(page, `${code} banner`);
      await shot(`05-${code}`);
      await page.locator(".route-actions button").first().click();
    });
  }

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });
} catch {
  await page.screenshot({ path: join(artifacts, "router-failure.png") }).catch(() => undefined);
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
writeFileSync(join(artifacts, "router-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
