// Checks a built beta package as a tester would get it: installs the release installer
// for this user into a test folder, starts it, and looks at what it unlocks.
// Usage: beta.mjs <package directory, for example dist-beta/beta-1>
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const directory = resolve(process.argv[2] ?? "");
const root = mkdtempSync(join(tmpdir(), "troniccad-beta-"));
const target = join(root, "installiert");
process.env.TRONICCAD_BIN = join(target, "troniccad.exe");
const { artifacts, assert, cleanUp, launch, useIsolatedData, wait } = await import("./lib.mjs");

const UNINSTALL_KEY = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\TronicCAD";
if (spawnSync("reg", ["query", UNINSTALL_KEY]).status === 0) {
  console.log("TronicCAD is installed on this computer: the beta was not checked");
  process.exit(1);
}
mkdirSync(artifacts, { recursive: true });
const dataDir = useIsolatedData();
process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, "webview");
process.env.TRONICCAD_KEYRING_SERVICE = `TronicCAD-e2e-beta-${process.pid}`;
const problems = [];
const setup = readdirSync(directory).find((name) => name.endsWith("-setup.exe"));
assert(setup, "there is an installer");
const build = JSON.parse(readFileSync(join(directory, "build-windows.json"), "utf8"));

let app;
let failed = false;
function check(name, condition, detail = "") {
  console.log(`${condition ? "ok   " : "FAIL "} ${name}${condition ? "" : ` ${detail}`}`);
  if (!condition) failed = true;
}
try {
  const installer = readFileSync(join(directory, setup));
  check("checksum matches", createHash("sha256").update(installer).digest("hex") === build.sha256);
  execFileSync(join(directory, setup), ["/S", `/D=${target}`], { stdio: "ignore" });
  check("installed", existsSync(process.env.TRONICCAD_BIN));
  check("third-party notices are installed", existsSync(join(target, "THIRD_PARTY_NOTICES.md")));
  check("license is installed", existsSync(join(target, "LICENSE.txt")));
  app = await launch(9349, problems);
  const { page } = app;
  const info = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("app_info"));
  check("it names itself", info.beta === build.label, String(info.beta));
  const update = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("update_status"));
  check("it looks for no updates", update.configured === false);
  await page.getByText(build.label).first().waitFor();
  await page.screenshot({ path: join(artifacts, "beta-started.png") });
  check("no errors", problems.length === 0, problems.join("; "));
} catch (error) {
  console.error(`FAIL  ${String(error).slice(0, 1200)}`);
  failed = true;
} finally {
  if (app) {
    await app.browser.close().catch(() => undefined);
    spawnSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
    await wait(1000);
  }
  if (existsSync(join(target, "uninstall.exe"))) {
    spawnSync(join(target, "uninstall.exe"), ["/S"], { stdio: "ignore" });
    for (let attempt = 0; attempt < 80 && existsSync(target); attempt++) await wait(250);
    check("uninstalled", !existsSync(target));
  }
  cleanUp(root);
  cleanUp(dataDir);
}
console.log(failed ? "\nBETA CHECK FAILED" : "\nBETA CHECK PASSED");
process.exit(failed ? 1 : 0);
