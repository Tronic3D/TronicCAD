// End-to-end test against the real application binary with its real backend.
//
// Windows only for now: it drives WebView2 through the Chrome DevTools
// Protocol. Only the native file dialogs are replaced, because they cannot be
// operated from inside the page; everything else is the shipped code path.
//
// Usage: npm run e2e   (after `npx tauri build --debug --no-bundle`)
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { cleanUp, outputFile, recordHang, useIsolatedData, waitForInterface } from "./lib.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
const binary = process.env.TRONICCAD_BIN ?? join(repo, "target/debug/troniccad.exe");
const artifacts = join(repo, "e2e-artifacts");
const PORT = 9333;
const dataDir = useIsolatedData();

mkdirSync(artifacts, { recursive: true });
const root = mkdtempSync(join(tmpdir(), "troniccad-e2e-"));
const folder = join(root, "Größe 电路 é");
mkdirSync(folder);
const projectPath = join(folder, "Netzteil 电源.tronic");
const secondPath = join(folder, "Kopie.tronic");

const results = [];
const consoleProblems = [];

async function step(name, action) {
  try {
    await action();
    results.push({ name, ok: true });
    console.log(`ok    ${name}`);
  } catch (error) {
    results.push({ name, ok: false, error: String(error) });
    console.error(`FAIL  ${name}\n      ${String(error).split("\n").slice(0, 6).join("\n      ")}`);
    throw error;
  }
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function launch() {
  const child = spawn(binary, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
    stdio: ["ignore", outputFile(PORT), outputFile(PORT)],
  });
  let browser;
  // A first start of the web view on a fresh computer can take much longer than here.
  for (let attempt = 0; attempt < 240 && !browser && child.exitCode === null; attempt++) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
    } catch {
      await new Promise((done) => setTimeout(done, 500));
    }
  }
  if (!browser) {
    const code = child.exitCode;
    recordHang(PORT);
    // Left running, it would keep its folders locked and hold up the rest of the run.
    child.kill();
    throw new Error(`application did not expose its web view, exit code ${code}`);
  }
  let page;
  for (let attempt = 0; attempt < 40 && !page; attempt++) {
    page = browser.contexts().flatMap((context) => context.pages())[0];
    if (!page) await new Promise((done) => setTimeout(done, 250));
  }
  assert(page, "no page found");
  page.on("console", (message) => {
    if (message.type() === "error") consoleProblems.push(message.text());
  });
  page.on("pageerror", (error) => consoleProblems.push(String(error)));
  await waitForInterface(page, consoleProblems);
  return { child, browser, page };
}

/** Makes the next native file dialog answer with `path`. */
async function answerDialogsWith(page, path) {
  await page.evaluate((target) => {
    window.__e2eDialogPath = target;
    if (!window.__e2ePatched) {
      window.__e2ePatched = true;
      // Tauri locks its JavaScript internals, so the answer is given one level
      // lower, where the dialog command would reach the backend over IPC.
      const realFetch = window.fetch;
      window.fetch = (url, init) =>
        String(url).includes("plugin%3Adialog")
          ? Promise.resolve(
              new Response(JSON.stringify(window.__e2eDialogPath), {
                headers: { "Tauri-Response": "ok", "Content-Type": "application/json" },
              }),
            )
          : realFetch(url, init);
    }
  }, path);
}

const invoke = (page, command, args) =>
  page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [command, args]);

async function statusIs(page, text) {
  await page.waitForFunction(
    (expected) => document.querySelector("[role=status]")?.textContent === expected,
    text,
    { timeout: 5000 },
  );
}

async function selectLanguage(page, code) {
  await page.selectOption(".language-picker select", code);
  await page.waitForFunction((c) => document.documentElement.lang === c, code);
}

async function assertNoOverflow(page, label) {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    const clipped = [...document.querySelectorAll(".tool-button, .rail-text, .start-card, .modal, h1, h2, label")]
      .filter((el) => el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0)
      .map((el) => `${el.className || el.tagName}: ${el.textContent?.slice(0, 40)}`);
    return { page: doc.scrollWidth > doc.clientWidth, clipped };
  });
  assert(!overflow.page, `${label}: page overflows horizontally`);
  assert(overflow.clipped.length === 0, `${label}: clipped elements: ${overflow.clipped.join(" | ")}`);
}

const shot = (page, name) => page.screenshot({ path: join(artifacts, `${name}.png`) });
const file = (path) => JSON.parse(readFileSync(path, "utf8"));

let app;
let exitCode = 0;
try {
  app = await launch();
  let { page } = app;

  await step("application starts and shows the interface", async () => {
    await selectLanguage(page, "de-DE");
    await page.getByRole("heading", { name: "Willkommen bei TronicCAD" }).waitFor();
    await shot(page, "01-welcome-de");
  });

  await step("creates a project on a path with non-ASCII characters", async () => {
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Netzteil 电源");
    await page.getByLabel("Beschreibung").fill("5 V, 2 A – Größe é");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await shot(page, "02-new-project-dialog");
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    const stored = file(projectPath);
    assert(stored.format === "tronicproject", "wrong format id");
    assert(stored.project.name === "Netzteil 电源", "name not stored");
    assert(stored.project.description === "5 V, 2 A – Größe é", "description not stored");
    await shot(page, "03-project-de");
  });

  for (const [code, label, heading] of [
    ["en-US", "Project name", "Project details"],
    ["zh-CN", "项目名称", "项目信息"],
    ["fr-FR", "Nom du projet", "Informations du projet"],
    ["de-DE", "Projektname", "Projektdaten"],
  ]) {
    await step(`switches to ${code} with an open project and no layout overflow`, async () => {
      await selectLanguage(page, code);
      await page.getByRole("heading", { name: heading }).waitFor();
      assert((await page.getByLabel(label).inputValue()) === "Netzteil 电源", "input lost");
      await assertNoOverflow(page, code);
      await shot(page, `04-project-${code}`);
    });
  }

  await step("edits and saves; the old version remains as backup", async () => {
    await page.getByLabel("Projektname").fill("Netzteil V2");
    await statusIs(page, "Ungespeicherte Änderungen");
    assert(file(projectPath).project.name === "Netzteil 电源", "saved before the user asked");
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    assert(file(projectPath).project.name === "Netzteil V2", "not written");
    assert(file(`${projectPath}.bak`).project.name === "Netzteil 电源", "backup missing");
  });

  await step("detects an external change instead of overwriting it", async () => {
    const before = readFileSync(projectPath, "utf8");
    const external = `${before}\n`;
    writeFileSync(projectPath, external);
    await page.getByLabel("Projektname").fill("Netzteil V3");
    await statusIs(page, "Ungespeicherte Änderungen");
    await page.getByRole("button", { name: "Speichern", exact: true }).click();
    await page.getByRole("heading", { name: "Datei wurde extern geändert" }).waitFor();
    assert(readFileSync(projectPath, "utf8") === external, "file was overwritten");
    await shot(page, "05-conflict");
    await page.getByRole("button", { name: "Abbrechen" }).click();
    await statusIs(page, "Ungespeicherte Änderungen");
    await page.getByRole("button", { name: "Speichern", exact: true }).click();
    await page.getByRole("button", { name: "Überschreiben" }).click();
    await statusIs(page, "Gespeichert");
    assert(file(projectPath).project.name === "Netzteil V3", "overwrite failed");
  });

  await step("saves under a new name and continues on the new file", async () => {
    await answerDialogsWith(page, secondPath);
    await page.getByRole("button", { name: "Speichern unter" }).click();
    await page.getByText(secondPath, { exact: false }).first().waitFor();
    assert(file(secondPath).project.name === "Netzteil V3", "copy not written");
  });

  await step("refuses a tampered file and reports it in the active language", async () => {
    const tampered = join(folder, "manipuliert.tronic");
    writeFileSync(tampered, readFileSync(secondPath, "utf8").replace("Netzteil V3", "Manipuliert"));
    await answerDialogsWith(page, tampered);
    await page.getByRole("button", { name: "Öffnen", exact: true }).click();
    await page.getByRole("heading", { name: "Vorgang fehlgeschlagen" }).waitFor();
    await page.getByText("Integritätsprüfung").waitFor();
    await shot(page, "06-integrity-error");
    await page.getByRole("button", { name: "Schließen", exact: true }).last().click();
    assert((await page.getByLabel("Projektname").inputValue()) === "Netzteil V3", "project replaced");
  });

  await step("lists recent projects and reopens one", async () => {
    await page.getByRole("button", { name: "Schließen", exact: true }).click();
    await statusIs(page, "Kein Projekt geöffnet");
    await shot(page, "07-recent");
    await page.getByRole("button", { name: new RegExp("Kopie") }).first().click();
    await statusIs(page, "Projekt geöffnet");
  });

  await step("language and recent list survive a restart", async () => {
    await selectLanguage(page, "fr-FR");
    await app.browser.close();
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
    await new Promise((done) => setTimeout(done, 1500));
    app = await launch();
    page = app.page;
    await page.getByRole("heading", { name: "Bienvenue dans TronicCAD" }).waitFor();
    await page.getByText(secondPath).waitFor();
    await assertNoOverflow(page, "fr-FR start screen");
    await shot(page, "08-welcome-fr-after-restart");
    await selectLanguage(page, "zh-CN");
    await page.getByRole("heading", { name: "欢迎使用 TronicCAD" }).waitFor();
    await assertNoOverflow(page, "zh-CN start screen");
    await shot(page, "09-welcome-zh");
    await selectLanguage(page, "de-DE");
  });

  await step("reopens the project with its content after the restart", async () => {
    await page.getByRole("button", { name: new RegExp("Netzteil V3") }).first().click();
    await statusIs(page, "Projekt geöffnet");
    assert((await page.getByLabel("Projektname").inputValue()) === "Netzteil V3", "name lost");
  });

  await step("closing the window with unsaved changes asks first", async () => {
    await page.getByLabel("Projektname").fill("Nicht speichern");
    await statusIs(page, "Ungespeicherte Änderungen");
    // Without /F this is a regular window close request.
    execFileSync("taskkill", ["/PID", String(app.child.pid)], { stdio: "ignore" });
    await page.getByRole("heading", { name: "Ungespeicherte Änderungen" }).waitFor();
    await shot(page, "10-unsaved-prompt");
    assert(app.child.exitCode === null, "application exited despite unsaved changes");
    await page.getByRole("button", { name: "Abbrechen" }).click();
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(consoleProblems.length === 0, consoleProblems.join(" | "));
  });

  await step("window closes after discarding, and nothing was written", async () => {
    const recent = await invoke(page, "recent_list");
    for (const entry of recent) {
      if (entry.path.startsWith(root)) await invoke(page, "recent_remove", { path: entry.path });
    }
    const closed = new Promise((done) => app.child.once("exit", done));
    execFileSync("taskkill", ["/PID", String(app.child.pid)], { stdio: "ignore" });
    await page.getByRole("button", { name: "Nicht speichern" }).click();
    await Promise.race([
      closed,
      new Promise((_, fail) => setTimeout(() => fail(new Error("window stayed open")), 8000)),
    ]);
    assert(file(projectPath).project.name === "Netzteil V3", "discarded edit reached the file");
    assert(existsSync(`${projectPath}.bak`), "backup vanished");
    app = null;
  });

  await step("capabilities allow the native dialogs and the window title", async () => {
    app = await launch();
    page = app.page;
    await invoke(page, "plugin:window|set_title", { label: "main", value: "TronicCAD" });
    // A missing permission rejects at once. A granted one opens the real
    // dialog, whose promise stays pending until the application is ended below.
    const outcome = await page.evaluate(() =>
      Promise.race([
        window.__TAURI_INTERNALS__
          .invoke("plugin:dialog|open", { options: {} })
          .then(() => "answered", (error) => `rejected: ${error}`),
        new Promise((done) => setTimeout(() => done("dialog open"), 2500)),
      ]),
    );
    assert(outcome === "dialog open", outcome);
  });
} catch (error) {
  // A failing step reports itself. Anything else would otherwise go unseen.
  if (results.every((result) => result.ok)) console.error(`FAIL  before or between steps\n      ${String(error).slice(0, 1500)}`);
  exitCode = 1;
} finally {
  if (app) {
    await app.browser.close().catch(() => undefined);
    try {
      execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // The process has already ended.
    }
  }
  await new Promise((done) => setTimeout(done, 500));
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
