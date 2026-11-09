// End-to-end test of automatic saving: the real application is killed hard,
// the way a crash or power loss ends it, and started again.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, cleanUp, invoke, launch, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9336;
const GRID = 1_270_000;
const dataDir = useIsolatedData();
const root = mkdtempSync(join(tmpdir(), "troniccad-rec-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "Größe 电路.tronic");
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

let app = await launch(PORT, problems);
let { page } = app;

/** Ends the process without any chance to clean up. */
async function crash() {
  await app.browser.close().catch(() => undefined);
  execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  await wait(1200);
}

async function restart() {
  app = await launch(PORT, problems);
  page = app.page;
  await page.locator(".toolbar").waitFor();
  await wait(300);
}

const snapshots = () => readdirSync(join(dataDir, "autosave")).filter((name) => name.endsWith(".autosave"));
const onDisk = () => JSON.parse(readFileSync(projectPath, "utf8")).project;
const dialogOpen = () => page.getByRole("dialog").count();

async function editSchematic() {
  await invoke(page, "schematic_apply", {
    viewSheet: 1,
    command: { type: "add_no_connect", sheet: 1, at: { x: 3 * GRID, y: 4 * GRID } },
  });
}

let exitCode = 0;
try {
  await step("the timer saves unsaved work without touching the project file", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Original");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    const before = readFileSync(projectPath, "utf8");

    await page.getByLabel("Projektname").fill("Ungespeichert");
    await statusIs(page, "Ungespeicherte Änderungen");
    await editSchematic();
    // The real 30 second timer, no shortcut.
    await page.getByText(/Automatisch gesichert/).waitFor({ timeout: 40000 });
    assert(snapshots().length === 1, `snapshots: ${snapshots()}`);
    assert(readFileSync(projectPath, "utf8") === before, "project file was changed");
    await page.screenshot({ path: join(artifacts, "rec-01-autosaved.png") });
  });

  await step("after a crash the work is offered and restored completely", async () => {
    await crash();
    await restart();
    await page.getByRole("heading", { name: "Ungespeicherte Arbeit gefunden" }).waitFor();
    await page.getByRole("dialog").getByText("Ungespeichert", { exact: true }).waitFor();
    await page.screenshot({ path: join(artifacts, "rec-02-recovery-dialog.png") });
    await page.getByRole("button", { name: "Wiederherstellen" }).click();
    await statusIs(page, "Ungespeicherte Änderungen");
    assert((await page.getByLabel("Projektname").inputValue()) === "Ungespeichert", "name not restored");
    const view = await invoke(page, "schematic_view", { sheet: 1 });
    assert(view.sheet.no_connects.length === 1, "schematic not restored");
    assert(onDisk().name === "Original", "restoring wrote to the file by itself");
  });

  await step("a second crash right after restoring loses nothing", async () => {
    await crash();
    await restart();
    await page.getByRole("button", { name: "Wiederherstellen" }).click();
    await statusIs(page, "Ungespeicherte Änderungen");
    assert((await page.getByLabel("Projektname").inputValue()) === "Ungespeichert", "work lost");
  });

  await step("saving writes the file and removes the snapshot", async () => {
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    assert(onDisk().name === "Ungespeichert", "not saved");
    assert(onDisk().schematic.sheets[0].no_connects.length === 1, "schematic not saved");
    assert(snapshots().length === 0, `snapshots: ${snapshots()}`);
  });

  await step("a crash without unsaved work asks nothing", async () => {
    await crash();
    await restart();
    assert((await dialogOpen()) === 0, "unexpected dialog");
    await page.getByRole("button", { name: /Ungespeichert/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
  });

  await step("an external change is reported and not overwritten", async () => {
    await page.getByLabel("Projektname").fill("Meine Fassung");
    await statusIs(page, "Ungespeicherte Änderungen");
    assert((await invoke(page, "autosave_now")) !== null, "no snapshot written");
    await crash();
    const external = `${readFileSync(projectPath, "utf8")}\n`;
    writeFileSync(projectPath, external);
    await restart();
    await page.getByText("wurde seitdem geändert").waitFor();
    await page.screenshot({ path: join(artifacts, "rec-03-file-changed.png") });
    await page.getByRole("button", { name: "Wiederherstellen" }).click();
    await statusIs(page, "Ungespeicherte Änderungen");
    await page.keyboard.press("Control+s");
    await page.getByRole("heading", { name: "Datei wurde extern geändert" }).waitFor();
    assert(readFileSync(projectPath, "utf8") === external, "external version overwritten");
    await page.getByRole("button", { name: "Überschreiben" }).click();
    await statusIs(page, "Gespeichert");
    assert(onDisk().name === "Meine Fassung", "not overwritten after confirming");
  });

  await step("discarding removes the work and keeps the file", async () => {
    await page.getByLabel("Projektname").fill("Wegwerfen");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "autosave_now");
    await crash();
    await restart();
    await page.getByRole("button", { name: "Verwerfen" }).click();
    await page.getByRole("heading", { name: "Ungespeicherte Arbeit gefunden" }).waitFor({ state: "hidden" });
    assert(snapshots().length === 0, "snapshot remains");
    assert(onDisk().name === "Meine Fassung", "file changed");
    await crash();
    await restart();
    assert((await dialogOpen()) === 0, "offered again after discarding");
  });

  await step("deciding later keeps it, and opening that file asks again", async () => {
    await page.getByRole("button", { name: /Meine Fassung/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    await page.getByLabel("Projektname").fill("Für später");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "autosave_now");
    await crash();
    await restart();
    await page.getByRole("button", { name: "Später entscheiden" }).click();
    assert((await dialogOpen()) === 0, "dialog stayed");
    assert(snapshots().length === 1, "snapshot removed");
    await page.getByRole("button", { name: /Für später|Meine Fassung/ }).first().click();
    await page.getByRole("heading", { name: "Ungespeicherte Arbeit gefunden" }).waitFor();
    await page.getByRole("button", { name: "Wiederherstellen" }).click();
    await statusIs(page, "Ungespeicherte Änderungen");
    assert((await page.getByLabel("Projektname").inputValue()) === "Für später", "wrong content");
  });

  await step("closing without saving removes the snapshot, nothing is offered later", async () => {
    await invoke(page, "autosave_now");
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await page.getByRole("button", { name: "Nicht speichern" }).click();
    await statusIs(page, "Kein Projekt geöffnet");
    assert(snapshots().length === 0, "snapshot remains after closing");
    await crash();
    await restart();
    assert((await dialogOpen()) === 0, "offered after an explicit decision");
  });

  await step("save as removes the snapshot of the old file", async () => {
    await page.getByRole("button", { name: /Für später|Meine Fassung/ }).first().click();
    await statusIs(page, "Projekt geöffnet");
    await page.getByLabel("Projektname").fill("Noch eine Fassung");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "autosave_now");
    assert(snapshots().length === 1, "no snapshot");
    await answerDialogsWith(page, join(root, "anderes.tronic"));
    await page.getByRole("button", { name: "Speichern unter" }).click();
    await statusIs(page, "Gespeichert");
    assert(snapshots().length === 0, "snapshot remains after save as");
  });

  await step("opening another project without saving removes the old snapshot", async () => {
    await page.getByLabel("Projektname").fill("Nicht behalten");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "autosave_now");
    assert(snapshots().length === 1, "no snapshot");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: "Öffnen", exact: true }).click();
    await page.getByRole("button", { name: "Nicht speichern" }).click();
    await statusIs(page, "Projekt geöffnet");
    assert(snapshots().length === 0, "snapshot remains after switching");
    assert(!readFileSync(join(root, "anderes.tronic"), "utf8").includes("Nicht behalten"), "discarded work was written");
  });

  await step("the window close dialog removes the snapshot too", async () => {
    await page.getByLabel("Projektname").fill("Fensterschluss");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "autosave_now");
    assert(snapshots().length === 1, "no snapshot");
    const closed = new Promise((done) => app.child.once("exit", done));
    execFileSync("taskkill", ["/PID", String(app.child.pid)], { stdio: "ignore" });
    await page.getByRole("button", { name: "Nicht speichern" }).click();
    await Promise.race([
      closed,
      new Promise((_, fail) => setTimeout(() => fail(new Error("window stayed open")), 8000)),
    ]);
    assert(snapshots().length === 0, "snapshot remains after closing the window");
    await app.browser.close().catch(() => undefined);
    await restart();
    assert((await dialogOpen()) === 0, "offered after closing the window");
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });
} catch {
  await page.screenshot({ path: join(artifacts, "rec-failure.png") }).catch(() => undefined);
  exitCode = 1;
} finally {
  await app.browser.close().catch(() => undefined);
  try {
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // The process has already ended.
  }
  await wait(800);
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "recovery-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
