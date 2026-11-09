// End-to-end test of updates: a copy of the real application, the real signing tool, and
// a local server that misbehaves. The program file really is exchanged and restarted.
// The "newer version" is the same build with bytes added and a version set by a
// development switch, because two builds per test run would take too long.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "../../..");
const root = mkdtempSync(join(tmpdir(), "troniccad-update-"));
const installed = join(root, "programm");
mkdirSync(installed);
const program = join(installed, "troniccad.exe");
const previous = join(installed, "troniccad.previous");
copyFileSync(join(repoRoot, "target/debug/troniccad.exe"), program);
// The library takes the program to start from here, so this comes before it is loaded.
process.env.TRONICCAD_BIN = program;
const { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, launch, selectLanguage, statusIs, useIsolatedData, wait } = await import("./lib.mjs");

mkdirSync(artifacts, { recursive: true });
const PORT = 9347;
const release = join(repoRoot, "target/debug/tronic-release.exe");
const dataDir = useIsolatedData();
const results = [];
const problems = [];
const PLATFORM = "windows-x86_64";
process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, "webview");
process.env.TRONICCAD_KEYRING_SERVICE = `TronicCAD-e2e-update-${process.pid}`;
process.env.TRONICCAD_TEST_APP_VERSION = "0.1.0";

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

const hash = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const original = hash(program);

// What the server answers. Tests change it to misbehave.
const manifests = {};
const files = {};
const http = createServer((request, response) => {
  const name = decodeURIComponent(new URL(request.url, "http://x").pathname.slice(1));
  const body = manifests[name] ?? files[name];
  if (body === undefined) {
    response.writeHead(404).end();
    return;
  }
  response.writeHead(200, { "content-length": Buffer.byteLength(body) }).end(body);
});
await new Promise((done) => http.listen(0, "127.0.0.1", done));
const SERVER = `http://127.0.0.1:${http.address().port}`;
process.env.TRONICCAD_TEST_UPDATE_URL = SERVER;

function run(...args) {
  return execFileSync(release, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

const keyFile = join(root, "release.key");
const publicKey = run("generate-key", keyFile).match(/public key: ([0-9a-f]{64})/)?.[1];
assert(publicKey, "the tool prints its public key");
const foreignKeyFile = join(root, "foreign.key");
run("generate-key", foreignKeyFile);
const notes = join(root, "notes.json");
writeFileSync(
  notes,
  JSON.stringify({
    "de-DE": "Behebt ein Sicherheitsproblem beim Öffnen von Dateien.",
    "en-US": "Fixes a security problem when opening files.",
    "zh-CN": "修复打开文件时的安全问题。",
    "fr-FR": "Corrige un problème de sécurité à l’ouverture des fichiers.",
  }),
);

/** Signs with the real tool. Returns the text the server hands out. */
let sequenceFile = "none";
function sign({ version, channel = "stable", kind = "feature", key = keyFile, platform = PLATFORM, days = "30", artifact, url, keepSequence = false }) {
  const out = join(root, `manifest-${Date.now()}-${Math.random().toString(16).slice(2)}.json`);
  run(
    "sign", "--key", key, "--channel", channel, "--version", version, "--kind", kind, "--previous", sequenceFile,
    "--valid-days", days, "--notes", notes, "--artifact", `${platform}=${artifact}=${url}`, "--out", out,
  );
  if (!keepSequence) sequenceFile = out;
  return readFileSync(out, "utf8");
}

// A valid program that differs from the installed one: bytes after the end of a Windows
// program file do not change how it runs.
const newProgram = join(root, "new-program.exe");
writeFileSync(newProgram, Buffer.concat([readFileSync(program), Buffer.from("TRONICCAD E2E NEW VERSION")]));
const newHash = hash(newProgram);
files["new.exe"] = readFileSync(newProgram);
const newUrl = `${SERVER}/new.exe`;

let app;
let page;
let browser;
const attach = (found) => {
  found.on("console", (message) => {
    if (message.type() === "error") problems.push(message.text());
  });
  found.on("pageerror", (error) => problems.push(String(error)));
};

async function start() {
  app = await launch(PORT, problems);
  ({ page, browser } = app);
  await selectLanguage(page, "de-DE");
}

function stopInstalled() {
  // Whatever runs from the test's own folder, and only that.
  try {
    execFileSync(
      "powershell",
      ["-NoProfile", "-Command", `Get-Process troniccad -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '${installed}*' } | Stop-Process -Force`],
      { stdio: "ignore" },
    );
  } catch {
    // Nothing was running.
  }
}

const outcome = (command, args) =>
  page.evaluate(
    ([name, values]) =>
      window.__TAURI_INTERNALS__.invoke(name, values ?? {}).then(
        () => "no error",
        (error) => error?.code ?? String(error),
      ),
    [command, args],
  );
const status = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke("update_status"));
const openUpdates = () => page.getByRole("navigation").getByRole("button", { name: /^Updates/ }).click();

async function refused(command, code) {
  const found = await outcome(command);
  assert(found === code, `${command}: expected ${code}, got ${found}`);
}

async function nothingOffered() {
  const found = await status();
  assert(found.available === null, `offered: ${JSON.stringify(found.available)}`);
  assert(hash(program) === original, "the program is unchanged");
}

/** After the exchange the new program is reached through the same debugging port. */
async function reachVersion(version) {
  await browser?.close().catch(() => {});
  for (let attempt = 0; attempt < 120; attempt++) {
    try {
      const next = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
      for (const candidate of next.contexts().flatMap((context) => context.pages())) {
        const found = await candidate
          .evaluate(() => window.__TAURI_INTERNALS__?.invoke("update_status").then((s) => s.version))
          .catch(() => null);
        if (found === version && (await candidate.locator(".toolbar").count()) > 0) {
          browser = next;
          page = candidate;
          attach(page);
          return;
        }
      }
      await next.close().catch(() => {});
    } catch {
      // Not there yet.
    }
    await wait(500);
  }
  throw new Error(`version ${version} did not come up`);
}

const projectPath = join(root, "Probe.tronic");
let exitCode = 0;
try {
  await step("the tool refuses a release key inside the repository", () => {
    let refusedKey = false;
    try {
      run("generate-key", join(repoRoot, "e2e-artifacts", "release-key-test.key"));
    } catch {
      refusedKey = true;
    }
    assert(refusedKey, "the key was written");
    assert(!existsSync(join(repoRoot, "e2e-artifacts", "release-key-test.key")), "no file stayed behind");
  });

  await step("without a trusted key the application looks for nothing", async () => {
    await start();
    const found = await status();
    assert(!found.configured, "not configured");
    await refused("update_check", "update.notConfigured");
    await openUpdates();
    await page.getByText("kein Update-Server eingebaut").first().waitFor();
    assert(found.automatic, "looking is on by default");
    assert(found.channel === "stable", "stable by default");
    await app.browser.close().catch(() => {});
    stopInstalled();
    await wait(800);
    process.env.TRONICCAD_TEST_UPDATE_KEYS = publicKey;
    await start();
  });

  await step("a project is open while all this happens", async () => {
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("Probe");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    await openUpdates();
    await page.getByTestId("update-version").filter({ hasText: "0.1.0" }).waitFor();
  });

  await step("a manifest whose version was changed is refused", async () => {
    const signed = JSON.parse(sign({ version: "0.2.0", artifact: newProgram, url: newUrl, keepSequence: true }));
    signed.payload = signed.payload.replace("0.2.0", "9.9.9");
    manifests["stable.json"] = JSON.stringify(signed);
    await refused("update_check", "update.badSignature");
    await nothingOffered();
  });

  await step("a manifest signed with another key is refused", async () => {
    manifests["stable.json"] = sign({ version: "0.2.0", key: foreignKeyFile, artifact: newProgram, url: newUrl, keepSequence: true });
    await refused("update_check", "update.badSignature");
    await nothingOffered();
  });

  await step("a correctly signed older version is not offered", async () => {
    manifests["stable.json"] = sign({ version: "0.0.9", artifact: newProgram, url: newUrl });
    assert((await outcome("update_check")) === "no error", "the check itself is fine");
    await nothingOffered();
    await page.getByRole("button", { name: "Projekt" }).click();
    await openUpdates();
    await page.getByTestId("update-current").waitFor();
    await page.screenshot({ path: join(artifacts, "update-current.png") });
  });

  await step("the same version is not offered either", async () => {
    manifests["stable.json"] = sign({ version: "0.1.0", artifact: newProgram, url: newUrl });
    assert((await outcome("update_check")) === "no error", "the check itself is fine");
    await nothingOffered();
  });

  await step("a manifest of another channel is refused", async () => {
    manifests["stable.json"] = sign({ version: "0.2.0", channel: "developer", artifact: newProgram, url: newUrl });
    await refused("update_check", "update.wrongChannel");
    await nothingOffered();
  });

  await step("a build for another platform is refused", async () => {
    manifests["stable.json"] = sign({ version: "0.2.0", platform: "linux-x86_64", artifact: newProgram, url: newUrl });
    await refused("update_check", "update.wrongPlatform");
    await nothingOffered();
  });

  await step("an expired manifest is refused", async () => {
    manifests["stable.json"] = sign({ version: "0.2.0", days: "-1", artifact: newProgram, url: newUrl });
    await refused("update_check", "update.expired");
    await nothingOffered();
  });

  await step("an address without encryption is refused even when signed", async () => {
    manifests["stable.json"] = sign({ version: "0.2.0", artifact: newProgram, url: "http://updates.example.test/new.exe" });
    await refused("update_check", "update.badAddress");
    await nothingOffered();
  });

  let good;
  await step("an older manifest cannot be replayed after a newer one", async () => {
    const stale = sign({ version: "0.2.0", artifact: newProgram, url: newUrl });
    manifests["stable.json"] = stale;
    assert((await outcome("update_check")) === "no error", "the first is fine");
    good = sign({ version: "0.2.0", kind: "security", artifact: newProgram, url: newUrl });
    manifests["stable.json"] = good;
    assert((await outcome("update_check")) === "no error", "the newer is fine");
    manifests["stable.json"] = stale;
    await refused("update_check", "update.replayed");
    await nothingOffered();
    manifests["stable.json"] = good;
    assert((await outcome("update_check")) === "no error", "the newer still is");
  });

  await step("a changed, short or long download is discarded", async () => {
    const real = files["new.exe"];
    const changed = Buffer.from(real);
    changed[1000] ^= 1;
    for (const [what, body] of [
      ["changed", changed],
      ["short", real.subarray(0, real.length - 1)],
      ["long", Buffer.concat([real, Buffer.from("x")])],
    ]) {
      files["new.exe"] = body;
      const found = await outcome("update_download");
      assert(found === "update.wrongContent", `${what}: ${found}`);
      assert(!(await status()).available.downloaded, `${what}: nothing is ready`);
      assert(!readdirSync(join(dataDir, "updates")).some((name) => name.endsWith(".part") || name.endsWith(".bin")), `${what}: nothing stays behind`);
      await refused("update_install", "update.notDownloaded");
    }
    files["new.exe"] = real;
    assert(hash(program) === original, "the program is unchanged");
  });

  await step("the offer shows its kind and change log in the application's language", async () => {
    await page.getByRole("button", { name: "Projekt" }).click();
    await openUpdates();
    await page.getByRole("heading", { name: "Version 0.2.0 ist verfügbar" }).waitFor();
    await page.getByTestId("update-kind").filter({ hasText: "Sicherheitsupdate" }).waitFor();
    await page.getByTestId("update-notes").filter({ hasText: "Behebt ein Sicherheitsproblem" }).waitFor();
    await page.getByText("gesichert").first().waitFor();
    await page.screenshot({ path: join(artifacts, "update-available.png") });
  });

  await step("the status bar announces a security update", async () => {
    await page.reload();
    await page.waitForSelector(".toolbar");
    assert((await outcome("update_check")) === "no error", "still fine");
    await page.locator(".statusbar").getByRole("button", { name: /Sicherheitsupdate 0\.2\.0/ }).waitFor({ timeout: 8000 }).catch(async () => {
      await page.reload();
      await page.locator(".statusbar").getByRole("button", { name: /Sicherheitsupdate 0\.2\.0/ }).waitFor();
    });
  });

  await step("unsaved work stops the installation", async () => {
    await page.evaluate((path) => window.__TAURI_INTERNALS__.invoke("project_open", { path }), projectPath);
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("schematic_apply", {
        viewSheet: 1,
        command: { type: "place_component", sheet: 1, symbol: "device:resistor", at: { x: 50800000, y: 0 } },
      }),
    );
    await openUpdates();
    await page.getByRole("button", { name: /Herunterladen und prüfen/ }).click();
    await page.getByText("Geladen und geprüft").waitFor();
    await refused("update_install", "update.saveFirst");
    assert(hash(program) === original, "the program is unchanged");
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("project_save", { overwrite: true }));
  });

  await step("the update is installed, restarted and confirmed, with a backup first", async () => {
    const before = readFileSync(projectPath);
    await page.getByRole("button", { name: "Installieren und neu starten" }).click();
    await reachVersion("0.2.0");
    assert(hash(program) === newHash, "the new program is in place");
    assert(hash(previous) === original, "the previous program is kept");
    const found = await status();
    assert(found.previous_version === "0.1.0", `previous: ${found.previous_version}`);
    assert(found.backups.length === 1, `backups: ${found.backups.length}`);
    assert(found.backups[0].from === "0.1.0" && found.backups[0].to === "0.2.0", "the backup says what it is for");
    assert(found.backups[0].projects === 1, `projects: ${found.backups[0].projects}`);
    const backup = found.backups[0].path;
    assert(existsSync(join(backup, "settings.db")), "the settings are backed up");
    const copies = readdirSync(join(backup, "projects"));
    assert(copies.length === 1 && readFileSync(join(backup, "projects", copies[0])).equals(before), "the project is backed up exactly");
    await wait(1500);
    assert(!existsSync(join(dataDir, "updates", "pending.json")), "nothing is pending any more");
    await selectLanguage(page, "de-DE");
    await openUpdates();
    await page.getByRole("button", { name: "Zurück zu Version 0.1.0" }).waitFor();
    await page.getByTestId("update-backup").waitFor();
    await page.screenshot({ path: join(artifacts, "update-installed.png") });
  });

  await step("the same older manifest cannot take the new version back", async () => {
    await refused("update_check", "no error");
    assert((await status()).available === null, "0.2.0 is current now");
  });

  await step("going back on request restores the previous program", async () => {
    await page.getByRole("button", { name: "Zurück zu Version 0.1.0" }).click();
    await reachVersion("0.1.0");
    assert(hash(program) === original, "the previous program is back");
    assert(hash(previous) === newHash, "the other one is kept");
    await selectLanguage(page, "de-DE");
  });

  await step("an update that does not start is taken back by itself", async () => {
    const broken = join(root, "exits-at-once.exe");
    copyFileSync(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "hostname.exe"), broken);
    files["broken.exe"] = readFileSync(broken);
    manifests["stable.json"] = sign({ version: "0.2.1", artifact: broken, url: `${SERVER}/broken.exe` });
    await openUpdates();
    await page.getByRole("button", { name: "Jetzt suchen" }).click();
    await page.getByRole("heading", { name: "Version 0.2.1 ist verfügbar" }).waitFor();
    await page.getByRole("button", { name: /Herunterladen und prüfen/ }).click();
    await page.getByRole("button", { name: "Installieren und neu starten" }).click();
    await page.getByRole("alert").filter({ hasText: "startete nicht" }).waitFor({ timeout: 30000 });
    assert(hash(program) === original, "the working program is back");
    const found = await status();
    assert(found.version === "0.1.0", "and it is still running");
    await page.getByText("schon einmal nicht starten").waitFor();
    await page.screenshot({ path: join(artifacts, "update-failed.png") });
  });

  await step("a file that is no program at all is taken back too", async () => {
    const garbage = join(root, "no-program.bin");
    writeFileSync(garbage, "this is not a program");
    files["garbage.bin"] = readFileSync(garbage);
    manifests["stable.json"] = sign({ version: "0.2.2", artifact: garbage, url: `${SERVER}/garbage.bin` });
    assert((await outcome("update_check")) === "no error", "signed correctly");
    assert((await outcome("update_download")) === "no error", "matches what was signed");
    await refused("update_install", "update.startFailed");
    assert(hash(program) === original, "the working program is back");
  });

  await step("the module holds in all four languages", async () => {
    await page.getByRole("button", { name: "Projekt" }).click();
    await openUpdates();
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      await wait(100);
      await assertNoOverflow(page, `updates ${language}`);
      await page.screenshot({ path: join(artifacts, `update-lang-${language}.png`) });
    }
  });

  await step("the application reported no errors", async () => {
    assert(problems.length === 0, problems.join("; "));
  });
} catch (error) {
  // A failing step reports itself. Anything else would otherwise go unseen.
  if (results.every((result) => result.ok)) console.error(`FAIL  before or between steps\n      ${String(error).slice(0, 1500)}`);
  exitCode = 1;
} finally {
  await browser?.close().catch(() => {});
  stopInstalled();
  http.close();
  await wait(1000);
  try {
    const listed = execFileSync("cmdkey", ["/list"], { encoding: "utf8" });
    for (const match of listed.matchAll(new RegExp(`[^\\s=]*${process.env.TRONICCAD_KEYRING_SERVICE}[^\\s]*`, "g"))) {
      execFileSync("cmdkey", [`/delete:${match[0]}`], { stdio: "ignore" });
    }
  } catch {
    // Nothing to clean up.
  }
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "update-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
