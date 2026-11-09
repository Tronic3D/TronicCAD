// End-to-end test of extensions and of the third-party information in the application.
// Uses the shipped example, which needs Python.
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, repo, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const results = [];
const problems = [];
const root = mkdtempSync(join(tmpdir(), "troniccad-ext-"));
const dataDir = useIsolatedData();
const installed = join(dataDir, "extensions");
mkdirSync(artifacts, { recursive: true });
process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, "webview");
const python = process.env.TRONICCAD_VERIFY_PYTHON ? null : spawnSync("python", ["--version"]).status === 0;

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

let app;
let exitCode = 0;
const projectPath = join(root, "Probe.tronic");
try {
  app = await launch(9350, problems);
  const { page } = app;
  await selectLanguage(page, "de-DE");
  const openModule = (name) => page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
  const card = () => page.getByTestId("extension-card").filter({ hasText: "Part list" });
  const outcome = (command, args) =>
    page.evaluate(
      ([name, values]) =>
        window.__TAURI_INTERNALS__.invoke(name, values ?? {}).then(
          () => "no error",
          (error) => error?.code ?? String(error),
        ),
      [command, args],
    );

  await step("all features are open without an activation", async () => {
    const rail = await page.getByRole("navigation").innerText();
    assert(!/Lizenz/.test(rail), `the rail still names licenses: ${rail}`);
    assert(!/Personal|Professional|Business/.test(rail), "the rail still names editions");
  });

  await step("the application names its own license and all third-party components", async () => {
    await openModule("Hilfe");
    await page.getByRole("button", { name: "Fremdkomponenten", exact: true }).click();
    await page.getByText("Apache-Lizenz 2.0").first().waitFor();
    const programs = await page.getByTestId("third-party-programs").innerText();
    for (const name of ["NSIS", "WebView2", "SQLite", "WebKitGTK"]) assert(programs.includes(name), `${name} is not named`);
    const bom = JSON.parse(readFileSync(join(repo, "sbom.cdx.json"), "utf8"));
    await page.getByTestId("third-party-components").locator("tbody tr").first().waitFor();
    assert((await page.getByTestId("third-party-components").locator("tbody tr").count()) === bom.components.length, "not all components are shown");
    await page.getByLabel("Suchen").fill("tauri-plugin-dialog");
    await page.getByTestId("third-party-components").getByText("tauri-plugin-dialog").waitFor();
    assert((await page.getByTestId("third-party-components").locator("tbody tr").count()) === 1, "the search does not narrow");
    await page.getByRole("button", { name: "Vollständige Lizenztexte" }).click();
    await page.getByTestId("third-party-text").filter({ hasText: "Third-Party Notices" }).waitFor({ timeout: 15000 });
    await page.getByRole("button", { name: "Lizenz von TronicCAD" }).click();
    await page.getByTestId("third-party-text").filter({ hasText: "Apache License" }).waitFor();
    await page.screenshot({ path: join(artifacts, "open-source-third-party.png") });
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      await wait(100);
      await assertNoOverflow(page, `third party ${language}`);
    }
  });

  await step("a project is open", async () => {
    await openModule("Projekt");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Probe");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    const placed = await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "place_component", sheet: 1, symbol: "device:resistor", at: { x: 50_800_000, y: 0 } } });
    await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "set_component", sheet: 1, id: placed.created[0], reference: "R7", value: "4k7" } });
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
  });

  await step("a broken extension is listed with its reason", async () => {
    mkdirSync(join(installed, "broken"), { recursive: true });
    writeFileSync(join(installed, "broken", "extension.json"), "{ not valid");
    await openModule("Erweiterungen");
    await page.getByTestId("extension-card").filter({ hasText: "lässt sich nicht lesen" }).waitFor();
  });

  await step("an extension does nothing until it is trusted", async () => {
    cpSync(join(repo, "extensions/examples/part-list"), join(installed, "part-list"), { recursive: true });
    await page.getByRole("button", { name: "Neu laden" }).click();
    await card().waitFor();
    assert(await card().getByRole("button", { name: "Vertrauen", exact: true }).isVisible(), "offered to trust");
    assert((await card().getByRole("button", { name: "Ausführen" }).count()) === 0, "it cannot be run yet");
    const refused = await outcome("extensions_run", { id: "part-list" });
    assert(refused === "extensions.notTrusted", refused);
    await page.screenshot({ path: join(artifacts, "open-source-extensions.png") });
  });

  await step("trusting asks first, and the backend refuses a changed fingerprint", async () => {
    const refused = await outcome("extensions_trust", { id: "part-list", shownFingerprint: "0".repeat(64) });
    assert(refused === "extensions.changed", refused);
    await card().getByRole("button", { name: "Vertrauen", exact: true }).click();
    await card().getByText("mit Ihren Rechten laufen").waitFor();
    await card().getByRole("button", { name: "Ja, vertrauen" }).click();
    await card().getByRole("button", { name: "Ausführen" }).waitFor();
  });

  if (python !== false) {
    await step("the example runs, reads the project and writes its result beside it", async () => {
      await card().getByRole("button", { name: "Ausführen" }).click();
      await card().getByTestId("extension-result").filter({ hasText: "1 components listed" }).waitFor({ timeout: 30000 });
      const output = join(root, "Probe-extensions", "part-list", "part-list.csv");
      assert(existsSync(output), "the file was written");
      const text = readFileSync(output, "utf8");
      assert(text.includes("R7") && text.includes("4k7"), text);
    });
  }

  await step("a changed extension needs trust again", async () => {
    writeFileSync(join(installed, "part-list", "main.py"), "import os\n", { flag: "a" });
    await page.getByRole("button", { name: "Neu laden" }).click();
    await card().getByText("Dateien wurden geändert").waitFor();
    const refused = await outcome("extensions_run", { id: "part-list" });
    assert(refused === "extensions.notTrusted", refused);
  });

  await step("the module holds in all four languages", async () => {
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      await wait(100);
      await assertNoOverflow(page, `extensions ${language}`);
    }
  });

  await step("the application reported no errors", () => {
    assert(problems.length === 0, problems.join("; "));
  });
} catch (error) {
  if (results.every((result) => result.ok)) console.error(`FAIL  before or between steps\n      ${String(error).slice(0, 1500)}`);
  exitCode = 1;
} finally {
  if (app) {
    await app.browser.close().catch(() => undefined);
    try {
      execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // Already ended.
    }
    await wait(500);
  }
  cleanUp(root);
  cleanUp(dataDir);
}
const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "extensions-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
