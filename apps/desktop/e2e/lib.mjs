// Shared helpers of the end-to-end tests.
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, openSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const here = dirname(fileURLToPath(import.meta.url));
export const repo = resolve(here, "../../..");
export const binary = process.env.TRONICCAD_BIN ?? join(repo, "target/debug/troniccad.exe");
export const artifacts = join(repo, "e2e-artifacts");

/** Keeps the test away from the user's settings, recent list and snapshots. */
export function useIsolatedData() {
  const directory = mkdtempSync(join(tmpdir(), "troniccad-data-"));
  process.env.TRONICCAD_DATA_DIR = directory;
  return directory;
}

/**
 * Removes a test's folder. The web view can hold files for a while after the program
 * ended, on slower computers for longer. What remains is only a temporary folder, and it
 * must not take the place of the test's own result.
 */
export function cleanUp(path) {
  try {
    rmSync(path, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
  } catch (error) {
    console.warn(`note: ${path} could not be removed: ${error.code ?? error}`);
  }
}

export function assert(condition, message) {
  if (!condition) throw new Error(message);
}

export const wait = (ms) => new Promise((done) => setTimeout(done, ms));

/** What the program printed, kept so that a failed start can be explained. */
export function outputFile(port) {
  mkdirSync(artifacts, { recursive: true });
  return openSync(join(artifacts, `app-output-${port}.log`), "w");
}

/** Records what is going on while the program hangs, before it is ended. */
export function recordHang(port) {
  const script = [
    "Add-Type -AssemblyName System.Windows.Forms, System.Drawing",
    "$s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds",
    "$p = New-Object System.Drawing.Bitmap $s.Width, $s.Height",
    "[System.Drawing.Graphics]::FromImage($p).CopyFromScreen($s.Location, [System.Drawing.Point]::Empty, $s.Size)",
    `$p.Save('${join(artifacts, "hang-desktop.png")}')`,
    "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'troniccad|msedgewebview2|WerFault' } | ForEach-Object { $_.Name + ' ' + $_.CommandLine }",
    `Get-NetTCPConnection -LocalPort ${port} -ErrorAction SilentlyContinue | Format-List State, OwningProcess`,
  ].join("; ");
  const found = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8", timeout: 30000 });
  console.error(`at the hang:\n${found.stdout}\n${found.stderr}`.slice(0, 6000));
  try {
    console.error(`program output:\n${readFileSync(join(artifacts, `app-output-${port}.log`), "utf8").slice(0, 3000)}`);
  } catch {
    // Nothing written.
  }
}

/** Waits for the interface. If it does not come, says what is there instead. */
export async function waitForInterface(page, problems) {
  try {
    await page.waitForSelector(".toolbar");
  } catch (error) {
    mkdirSync(artifacts, { recursive: true });
    await page.screenshot({ path: join(artifacts, "start-failure.png") }).catch(() => undefined);
    const shown = await page.evaluate(() => document.body?.innerText?.slice(0, 500)).catch(() => "unreadable");
    throw new Error(`no interface at ${page.url()}: ${JSON.stringify(shown)}; console: ${problems.join("; ")}; ${error}`);
  }
}

export async function launch(port, problems) {
  const child = spawn(binary, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: ["ignore", outputFile(port), outputFile(port)],
  });
  let browser;
  // A first start of the web view on a fresh computer can take much longer than here.
  for (let attempt = 0; attempt < 240 && !browser && child.exitCode === null; attempt++) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    } catch {
      await wait(500);
    }
  }
  if (!browser) {
    const code = child.exitCode;
    recordHang(port);
    // Left running, it would keep its folders locked and hold up the rest of the run.
    child.kill();
    throw new Error(`application did not expose its web view, exit code ${code}`);
  }
  let page;
  for (let attempt = 0; attempt < 40 && !page; attempt++) {
    page = browser.contexts().flatMap((context) => context.pages())[0];
    if (!page) await wait(250);
  }
  assert(page, "no page found");
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(message.text());
  });
  page.on("pageerror", (error) => problems.push(String(error)));
  await waitForInterface(page, problems);
  return { child, browser, page };
}

/** Makes native file dialogs answer with `path`, one level below Tauri's locked internals. */
export async function answerDialogsWith(page, path) {
  await page.evaluate((target) => {
    window.__e2eDialogPath = target;
    if (!window.__e2ePatched) {
      window.__e2ePatched = true;
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

export const invoke = (page, command, args) =>
  page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [command, args]);

export async function statusIs(page, text) {
  await page.waitForFunction(
    (expected) => document.querySelector(".statusbar [role=status]")?.textContent === expected,
    text,
    { timeout: 30000 },
  );
}

export async function selectLanguage(page, code) {
  await page.selectOption(".language-picker select", code);
  await page.waitForFunction((c) => document.documentElement.lang === c, code);
}

export async function assertNoOverflow(page, label) {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    const clipped = [...document.querySelectorAll(".tool-button, .rail-text, .start-card, .modal, h1, h2, label, .part, .side-tab, .erc-run, .erc-item")]
      .filter((el) => el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0)
      .map((el) => `${el.className || el.tagName}: ${el.textContent?.slice(0, 40)}`);
    return { page: doc.scrollWidth > doc.clientWidth, clipped };
  });
  assert(!overflow.page, `${label}: page overflows horizontally`);
  assert(overflow.clipped.length === 0, `${label}: clipped elements: ${overflow.clipped.join(" | ")}`);
}
