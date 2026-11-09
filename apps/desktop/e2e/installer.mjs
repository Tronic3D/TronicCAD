// End-to-end test of the Windows installer: installs for this user into a test folder,
// starts the installed program, updates it from there, and uninstalls it. Uses the
// development build of the installer, because only that lets a test set a version.
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "../../..");
const root = mkdtempSync(join(tmpdir(), "troniccad-installer-"));
const target = join(root, "installiert");
const program = join(target, "troniccad.exe");
process.env.TRONICCAD_BIN = program;
const { artifacts, assert, cleanUp, launch, selectLanguage, useIsolatedData, wait } = await import("./lib.mjs");

mkdirSync(artifacts, { recursive: true });
const UNINSTALL_KEY = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\TronicCAD";
const PORT = 9348;
const results = [];
const problems = [];
const dataDir = useIsolatedData();
process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, "webview");
process.env.TRONICCAD_KEYRING_SERVICE = `TronicCAD-e2e-installer-${process.pid}`;
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

function registry(value) {
  const found = spawnSync("reg", ["query", UNINSTALL_KEY, "/v", value], { encoding: "utf8" });
  if (found.status !== 0) return null;
  return found.stdout.match(new RegExp(`${value}\\s+REG_\\w+\\s+(.*)`))?.[1]?.trim() ?? null;
}

const directory = join(repoRoot, "target/debug/bundle/nsis");
const setup = readdirSync(directory).find((name) => name.endsWith("-setup.exe"));
assert(setup, "the installer was built");

if (registry("DisplayName")) {
  // A real installation on this computer must not be touched by a test.
  console.log("TronicCAD is installed on this computer: the installer test did not run");
  writeFileSync(join(artifacts, "installer-results.json"), `${JSON.stringify([{ name: "installer", skipped: true }], null, 2)}\n`);
  console.log("\n0/0 steps passed");
  process.exit(0);
}

const hash = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
let app;
let page;
let browser;

function stopInstalled() {
  try {
    execFileSync(
      "powershell",
      ["-NoProfile", "-Command", `Get-Process troniccad -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '${target}*' } | Stop-Process -Force`],
      { stdio: "ignore" },
    );
  } catch {
    // Nothing was running.
  }
}

const status = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke("update_status"));
const manifests = {};
const files = {};
const http = createServer((request, response) => {
  const name = new URL(request.url, "http://x").pathname.slice(1);
  const body = manifests[name] ?? files[name];
  if (body === undefined) return response.writeHead(404).end();
  return response.writeHead(200, { "content-length": Buffer.byteLength(body) }).end(body);
});
await new Promise((done) => http.listen(0, "127.0.0.1", done));
const SERVER = `http://127.0.0.1:${http.address().port}`;
process.env.TRONICCAD_TEST_UPDATE_URL = SERVER;

let exitCode = 0;
try {
  await step("the installer runs without administrator rights and installs for this user", () => {
    execFileSync(join(directory, setup), ["/S", `/D=${target}`], { stdio: "ignore" });
    assert(existsSync(program), "the program is installed");
    assert(registry("DisplayName")?.includes("TronicCAD"), "it is listed among the installed programs");
    assert(registry("DisplayVersion") === "0.1.0", `listed version: ${registry("DisplayVersion")}`);
    assert(existsSync(join(target, "uninstall.exe")), "the uninstaller is there");
    assert(registry("UninstallString")?.includes("uninstall.exe"), "and known to Windows");
  });

  await step("the installed program starts", async () => {
    const release = join(repoRoot, "target/debug/tronic-release.exe");
    const keyFile = join(root, "release.key");
    const publicKey = execFileSync(release, ["generate-key", keyFile], { encoding: "utf8" }).match(/public key: ([0-9a-f]{64})/)[1];
    process.env.TRONICCAD_TEST_UPDATE_KEYS = publicKey;
    const notes = join(root, "notes.json");
    writeFileSync(notes, JSON.stringify({ "de-DE": "Neu", "en-US": "New", "zh-CN": "新", "fr-FR": "Nouveau" }));
    const next = join(root, "next.exe");
    writeFileSync(next, Buffer.concat([readFileSync(program), Buffer.from("TRONICCAD E2E NEXT")]));
    files["next.exe"] = readFileSync(next);
    const out = join(root, "stable.json");
    execFileSync(
      release,
      ["sign", "--key", keyFile, "--channel", "stable", "--version", "0.1.1", "--kind", "feature", "--previous", "none", "--valid-days", "30", "--notes", notes, "--artifact", `windows-x86_64=${next}=${SERVER}/next.exe`, "--out", out],
      { stdio: "ignore" },
    );
    manifests["stable.json"] = readFileSync(out);
    app = await launch(PORT, problems);
    ({ page, browser } = app);
    await selectLanguage(page, "de-DE");
    assert((await status()).version === "0.1.0", "it reports its version");
    await page.screenshot({ path: join(artifacts, "installer-started.png") });
  });

  const updated = createHash("sha256").update(files["next.exe"] ?? Buffer.alloc(0)).digest("hex");
  await step("an update works where the installer put the program, without administrator rights", async () => {
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("update_check"));
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("update_download"));
    const before = hash(program);
    await page.getByRole("navigation").getByRole("button", { name: /^Updates/ }).click();
    await page.getByRole("button", { name: /Installieren|Herunterladen/ }).first().waitFor();
    await page.reload();
    await page.waitForSelector(".toolbar");
    await page.getByRole("navigation").getByRole("button", { name: /^Updates/ }).click();
    await page.getByRole("button", { name: "Installieren und neu starten" }).click();
    await browser.close().catch(() => {});
    let reached = false;
    for (let attempt = 0; attempt < 120 && !reached; attempt++) {
      try {
        const next = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
        for (const candidate of next.contexts().flatMap((context) => context.pages())) {
          const version = await candidate.evaluate(() => window.__TAURI_INTERNALS__?.invoke("update_status").then((s) => s.version)).catch(() => null);
          if (version === "0.1.1") {
            browser = next;
            page = candidate;
            reached = true;
          }
        }
        if (!reached) await next.close().catch(() => {});
      } catch {
        // Not there yet.
      }
      if (!reached) await wait(500);
    }
    assert(reached, "the new version came up");
    assert(hash(program) === updated, "the new program is in place");
    assert(hash(join(target, "troniccad.previous")) === before, "the previous one is kept");
  });

  await step("Windows lists the version that is really installed", async () => {
    for (let attempt = 0; attempt < 20 && registry("DisplayVersion") !== "0.1.1"; attempt++) await wait(250);
    assert(registry("DisplayVersion") === "0.1.1", `listed version: ${registry("DisplayVersion")}`);
  });

  await step("uninstalling removes the program with all it left behind, and keeps the user's data", async () => {
    await browser.close().catch(() => {});
    stopInstalled();
    await wait(800);
    assert(existsSync(join(dataDir, "troniccad.db")), "there is user data");
    execFileSync(join(target, "uninstall.exe"), ["/S"], { stdio: "ignore" });
    // The uninstaller copies itself elsewhere and returns at once.
    for (let attempt = 0; attempt < 120 && existsSync(target); attempt++) await wait(250);
    assert(!existsSync(target), `left behind: ${existsSync(target) ? readdirSync(target).join(", ") : ""}`);
    assert(registry("DisplayName") === null, "it is no longer listed");
    assert(existsSync(join(dataDir, "troniccad.db")), "the user's data is still there");
  });

  await step("the application reported no errors", () => {
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
  await wait(800);
  if (existsSync(join(target, "uninstall.exe"))) {
    spawnSync(join(target, "uninstall.exe"), ["/S"], { stdio: "ignore" });
    await wait(4000);
  }
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
writeFileSync(join(artifacts, "installer-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
