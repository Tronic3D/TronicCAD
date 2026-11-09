// End-to-end test of synchronization: two real application instances, like two
// computers, each with its own settings and project file, against real storage.
// S3, WebDAV and the cloud service run when TRONICCAD_RCLONE and
// TRONICCAD_TEST_DATABASE_URL say where to find their servers.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, repo, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const GRID = 1_270_000;
const KEYRING_SERVICE = `TronicCAD-e2e-cloud-${process.pid}`;
process.env.TRONICCAD_KEYRING_SERVICE = KEYRING_SERVICE;
const root = mkdtempSync(join(tmpdir(), "troniccad-cloud-"));
mkdirSync(artifacts, { recursive: true });
const results = [];
const problemsA = [];
const problemsB = [];
const servers = [];
const WEBDAV_PASSWORD = "e2e-webdav-not-real-1234";
const S3_SECRET = "e2e-s3-secret-not-real-1234";
const CLOUD_PASSWORD = "e2e cloud password not real";

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

const dataDirs = [];
async function device(port, problems, name) {
  dataDirs.push(useIsolatedData());
  // Each instance needs its own browser profile, or the second joins the first.
  process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, `webview-${name}`);
  return launch(port, problems);
}

const a = await device(9342, problemsA, "a");
const b = await device(9343, problemsB, "b");
const [pa, pb] = [a.page, b.page];

const pathA = join(root, "a", "Geraet.tronic");
const pathB = join(root, "b", "Geraet.tronic");
mkdirSync(join(root, "a"));
mkdirSync(join(root, "b"));
const storage = join(root, "storage");
mkdirSync(storage);

const module = (page, name) => page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
const safeInvoke = (page, command, args) =>
  page.evaluate(([c, x]) => window.__TAURI_INTERNALS__.invoke(c, x).catch((error) => ({ rejected: error })), [command, args]);
const netlist = async (page) => (await invoke(page, "schematic_netlist")).text;

const placed = new Map();
async function place(page, reference, value) {
  const count = placed.get(page) ?? 0;
  placed.set(page, count + 1);
  const at = { x: (count + 1) * 40 * GRID + Math.floor(Math.random() * 40) * 40 * GRID, y: 0 };
  const result = await invoke(page, "schematic_apply", {
    viewSheet: 1,
    command: { type: "place_component", sheet: 1, symbol: "device:resistor", at },
  });
  await invoke(page, "schematic_apply", {
    viewSheet: 1,
    command: { type: "set_component", sheet: 1, id: result.created[0], reference, value },
  });
  return result.created[0];
}
async function setValue(page, reference, value) {
  const view = await invoke(page, "schematic_view", { sheet: 1 });
  const part = view.sheet.components.find((c) => c.reference === reference);
  assert(part, `${reference} exists`);
  await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "set_component", sheet: 1, id: part.id, reference, value } });
}
const save = async (page) => {
  await page.keyboard.press("Control+s");
  await statusIs(page, "Gespeichert");
};
const openCloud = async (page) => {
  await module(page, "Projekt");
  await module(page, "Cloud");
  await page.getByRole("heading", { name: "Dieses Projekt" }).waitFor();
};
const addFolder = async (page, name, folder) => {
  await page.getByLabel("Art").selectOption("folder");
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByLabel("Ordner", { exact: true }).fill(folder);
  await page.getByRole("button", { name: "Hinzufügen" }).click();
  await page.locator(".ai-finding", { hasText: name }).waitFor();
};
const connectTarget = (page, name) =>
  page.locator(".ai-finding", { hasText: name }).getByRole("button", { name: "Projekt verbinden" }).click();
const syncNow = async (page) => {
  await page.getByRole("button", { name: "Jetzt synchronisieren" }).click();
  await page.getByRole("button", { name: "Jetzt synchronisieren" }).waitFor();
};
const notice = (page, text) => page.locator(".sim-results [role=status]").filter({ hasText: text }).waitFor();
const objects = (folder, kind) => {
  const projects = join(folder, "projects");
  if (!existsSync(projects)) return [];
  return readdirSync(projects).flatMap((id) => {
    const directory = join(projects, id, kind);
    return existsSync(directory) ? readdirSync(directory).map((name) => join(directory, name)) : [];
  });
};

const waitPort = async (port) => {
  for (let i = 0; i < 100; i++) {
    const open = await new Promise((done) => {
      const socket = connect(port, "127.0.0.1", () => {
        socket.destroy();
        done(true);
      });
      socket.on("error", () => done(false));
    });
    if (open) return;
    await wait(100);
  }
  throw new Error(`port ${port} did not open`);
};

let exitCode = 0;
try {
  await step("device A creates a project and stores its first version", async () => {
    await selectLanguage(pa, "de-DE");
    await answerDialogsWith(pa, pathA);
    await pa.getByRole("button", { name: /Neues Projekt/ }).click();
    await pa.getByLabel("Projektname").fill("Geraet");
    await pa.getByRole("button", { name: "Auswählen …" }).click();
    await pa.getByText(pathA).waitFor();
    await pa.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(pa, "Projekt angelegt");
    await place(pa, "R1", "1k");
    await save(pa);
    await openCloud(pa);
    await pa.getByText("Dieses Projekt ist mit keinem Speicher verbunden.").waitFor();
    await addFolder(pa, "NAS", storage);
    await connectTarget(pa, "NAS");
    await pa.getByRole("row", { name: /NAS/ }).waitFor();
    await syncNow(pa);
    await notice(pa, "NAS: Version gespeichert");
    assert(objects(storage, "commits").length === 1 && objects(storage, "blobs").length === 1, "one version stored");
    assert(!pa.url().includes("error"), "fine");
    await pa.screenshot({ path: join(artifacts, "cloud-first.png") });
  });

  await step("device B starts from a copy of the file and is up to date", async () => {
    copyFileSync(pathA, pathB);
    await selectLanguage(pb, "de-DE");
    await answerDialogsWith(pb, pathB);
    await pb.getByRole("button", { name: "Öffnen", exact: true }).click();
    await statusIs(pb, "Projekt geöffnet");
    await openCloud(pb);
    await addFolder(pb, "NAS", storage);
    await connectTarget(pb, "NAS");
    await pb.getByRole("row", { name: /NAS/ }).waitFor();
    await syncNow(pb);
    await notice(pb, "NAS: aktuell");
    assert(objects(storage, "commits").length === 1, "nothing new stored");
  });

  await step("a change on A reaches B, and the open project shows it", async () => {
    await setValue(pa, "R1", "2k2");
    await save(pa);
    await syncNow(pa);
    await notice(pa, "Version gespeichert");
    assert(!(await netlist(pb)).includes("2k2"), "not yet");
    await syncNow(pb);
    await notice(pb, "neuere Version geladen");
    assert((await netlist(pb)).includes("2k2"), "B has it");
    assert(existsSync(`${pathB}.bak`) || readdirSync(join(root, "b")).some((n) => n.includes("bak")), "the state before stays as a backup");
  });

  await step("unsaved work is neither sent nor overwritten", async () => {
    await setValue(pb, "R1", "unsaved");
    const before = readFileSync(pathB);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await pb.getByRole("alert").filter({ hasText: "Bitte das Projekt zuerst speichern" }).waitFor();
    assert(Buffer.compare(before, readFileSync(pathB)) === 0, "the file is untouched");
    assert((await netlist(pb)).includes("unsaved"), "the work is still there");
    await setValue(pb, "R1", "2k2");
    await save(pb);
  });

  await step("changes on both sides are a conflict; nothing is overwritten", async () => {
    // B's last save is a state of its own, which A has to fetch first.
    await syncNow(pb);
    await syncNow(pa);
    await notice(pa, "neuere Version geladen");
    await setValue(pa, "R1", "from-A");
    await save(pa);
    await syncNow(pa);
    await setValue(pb, "R1", "from-B");
    await save(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    const dialog = pb.getByRole("dialog", { name: "Es gibt zwei Stände" });
    await dialog.waitFor();
    assert((await netlist(pb)).includes("from-B"), "B keeps its state");
    await pb.screenshot({ path: join(artifacts, "cloud-conflict.png") });
    await dialog.getByRole("button", { name: "Später entscheiden" }).click();
    assert((await netlist(pb)).includes("from-B"), "deciding later changes nothing");
  });

  await step("keeping both writes the other state beside the project", async () => {
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await pb.getByRole("dialog").getByRole("button", { name: "Beide als Dateien behalten" }).click();
    await notice(pb, "Eine Kopie liegt unter");
    const copy = readdirSync(join(root, "b")).find((name) => name.includes("conflict"));
    assert(copy, "the copy exists");
    assert(readFileSync(join(root, "b", copy), "utf8").includes("from-A"), "with A's state");
    assert((await netlist(pb)).includes("from-B"), "B continues with its own");
    await syncNow(pb);
    await notice(pb, "aktuell");
    await syncNow(pa);
    await notice(pa, "neuere Version geladen");
    assert((await netlist(pa)).includes("from-B"), "the devices agree");
  });

  await step("taking the other state keeps the local one as a copy", async () => {
    await setValue(pa, "R1", "second-A");
    await save(pa);
    await syncNow(pa);
    await setValue(pb, "R1", "second-B");
    await save(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await pb.getByRole("dialog").getByRole("button", { name: "Mit dem anderen Stand weiter" }).click();
    await notice(pb, "Eine Kopie liegt unter");
    assert((await netlist(pb)).includes("second-A"), "B has A's state");
    const copy = readdirSync(join(root, "b")).find((name) => name.includes("(local"));
    assert(copy && readFileSync(join(root, "b", copy), "utf8").includes("second-B"), "B's state is kept");
    await syncNow(pb);
    await notice(pb, "aktuell");
    await syncNow(pa);
    await notice(pa, "aktuell");
  });

  await step("versions can be listed, compared and restored", async () => {
    await pa.getByRole("row", { name: /NAS/ }).getByRole("button", { name: "Versionen" }).click();
    const section = pa.getByRole("region", { name: "Versionen" });
    await section.waitFor();
    const rows = section.locator("tr");
    await rows.first().waitFor();
    const count = await rows.count();
    assert(count >= 5, `${count} versions`);
    await pa.screenshot({ path: join(artifacts, "cloud-versions.png") });
    // The oldest one, which still had 1k.
    await rows.last().getByRole("button", { name: "Vergleichen" }).click();
    await pa.getByText("R1: second-A im gespeicherten Projekt, 1k in dieser Version").waitFor();
    await rows.last().getByRole("button", { name: "Wiederherstellen" }).click();
    await notice(pa, "wiederhergestellt");
    const restored = await netlist(pa);
    assert(restored.includes("1k") && !restored.includes("second-A"), "the old state is back");
    await syncNow(pa);
    await notice(pa, "Version gespeichert");
    assert(objects(storage, "commits").length > count - 1, "history only grows");
    await syncNow(pb);
    await notice(pb, "neuere Version geladen");
    assert((await netlist(pb)).includes("1k"), "B follows");
  });

  await step("offline: work goes on, nothing is lost, and it catches up", async () => {
    const away = `${storage}-unplugged`;
    renameSync(storage, away);
    await setValue(pa, "R1", "offline-change");
    await save(pa);
    await syncNow(pa);
    await notice(pa, "nicht erreichbar");
    assert((await netlist(pa)).includes("offline-change"), "work is untouched");
    renameSync(away, storage);
    await syncNow(pa);
    await notice(pa, "Version gespeichert");
    await syncNow(pb);
    assert((await netlist(pb)).includes("offline-change"), "B gets it after all");
  });

  await step("a damaged version never reaches the project", async () => {
    await setValue(pa, "R1", "will-be-damaged");
    await save(pa);
    await syncNow(pa);
    const content = readFileSync(pathA);
    const blob = objects(storage, "blobs").find((file) => Buffer.compare(readFileSync(file), content) === 0);
    assert(blob, "the blob was found");
    writeFileSync(blob, content.subarray(0, content.length / 2));
    const before = readFileSync(pathB);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pb, "Prüfsumme");
    assert(Buffer.compare(before, readFileSync(pathB)) === 0, "B's file is untouched");
    writeFileSync(blob, content);
    await syncNow(pb);
    assert((await netlist(pb)).includes("will-be-damaged"), "fine again");
  });

  await step("a backup target takes every state and never changes the project", async () => {
    const backups = join(root, "backups");
    mkdirSync(backups);
    await addFolder(pa, "Sicherung", backups);
    await pa.getByLabel("Verbinden als").selectOption("backup");
    await connectTarget(pa, "Sicherung");
    await pa.getByRole("row", { name: /Sicherung/ }).waitFor();
    await syncNow(pa);
    await notice(pa, "Sicherung: Version gespeichert");
    assert(objects(backups, "commits").length === 1, "stored");
    await syncNow(pa);
    await notice(pa, "Sicherung: aktuell");
    assert(objects(backups, "commits").length === 1, "not twice");
    await pa.getByRole("row", { name: /Sicherung/ }).getByRole("button", { name: "Trennen" }).click();
    assert(objects(backups, "commits").length === 1, "disconnecting deletes nothing");
  });

  await step("automatic sync runs after saving", async () => {
    await pa.getByRole("row", { name: /NAS/ }).getByRole("checkbox", { name: "Automatisch" }).check();
    await wait(300);
    const before = objects(storage, "commits").length;
    await setValue(pa, "R1", "automatic");
    await save(pa);
    for (let i = 0; i < 50 && objects(storage, "commits").length === before; i++) await wait(100);
    assert(objects(storage, "commits").length === before + 1, "stored on its own");
    await pa.getByRole("row", { name: /NAS/ }).getByRole("checkbox", { name: "Automatisch" }).uncheck();
    await syncNow(pb);
    await notice(pb, "neuere Version geladen");
  });

  const rclone = process.env.TRONICCAD_RCLONE;
  if (rclone) {
    await step("WebDAV through the interface, against a real server", async () => {
      const dir = join(root, "webdav");
      mkdirSync(dir);
      servers.push(spawn(rclone, ["serve", "webdav", dir, "--addr", "127.0.0.1:18921", "--user", "tester", "--pass", WEBDAV_PASSWORD], { stdio: "ignore" }));
      await waitPort(18921);
      await pa.getByLabel("Art").selectOption("web_dav");
      await pa.getByLabel("Name", { exact: true }).fill("Dav");
      await pa.getByLabel("Adresse").fill("http://127.0.0.1:18921");
      await pa.getByLabel("Benutzer").fill("tester");
      await pa.getByLabel("Passwort").fill("wrong");
      await pa.getByRole("button", { name: "Hinzufügen" }).click();
      await pa.getByRole("alert").filter({ hasText: "Der Speicher hat die Anmeldung abgelehnt." }).waitFor();
      await pa.getByLabel("Passwort").fill(WEBDAV_PASSWORD);
      await pa.getByRole("button", { name: "Hinzufügen" }).click();
      await pa.locator(".ai-finding", { hasText: "Dav" }).waitFor();
      assert((await pa.getByLabel("Passwort").inputValue()) === "", "the field is cleared");
      await connectTarget(pa, "Dav");
      await pa.getByRole("row", { name: /Dav/ }).waitFor();
      await syncNow(pa);
      await notice(pa, "Dav: Version gespeichert");
      assert(objects(dir, "commits").length === 1, "on the server");
      await pa.getByRole("row", { name: /Dav/ }).getByRole("button", { name: "Trennen" }).click();
    });

    await step("S3 through the interface, against a real server", async () => {
      const dir = join(root, "s3");
      mkdirSync(join(dir, "projekte"), { recursive: true });
      servers.push(spawn(rclone, ["serve", "s3", dir, "--addr", "127.0.0.1:18922", "--auth-key", `e2eaccess,${S3_SECRET}`, "--force-path-style=true"], { stdio: "ignore" }));
      await waitPort(18922);
      await pa.getByLabel("Art").selectOption("s3");
      await pa.getByLabel("Name", { exact: true }).fill("Eimer");
      await pa.getByLabel("Adresse").fill("http://127.0.0.1:18922");
      await pa.getByLabel("Bucket").fill("projekte");
      await pa.getByLabel("Zugriffsschlüssel").fill("e2eaccess");
      await pa.getByLabel("Geheimer Schlüssel").fill(S3_SECRET);
      await pa.getByRole("button", { name: "Hinzufügen" }).click();
      await pa.locator(".ai-finding", { hasText: "Eimer" }).waitFor();
      await connectTarget(pa, "Eimer");
      await pa.getByRole("row", { name: /Eimer/ }).waitFor();
      await syncNow(pa);
      await notice(pa, "Eimer: Version gespeichert");
      assert(objects(join(dir, "projekte"), "commits").length === 1, "on the server");
      await pa.getByRole("row", { name: /Eimer/ }).getByRole("button", { name: "Trennen" }).click();
    });
  } else {
    console.log("      TRONICCAD_RCLONE is not set: WebDAV and S3 through the interface did not run");
    results.push({ name: "WebDAV and S3 through the interface", ok: true, skipped: true });
  }

  const database = process.env.TRONICCAD_TEST_DATABASE_URL;
  if (database) {
    await step("the TronicCAD cloud through the interface, with an account", async () => {
      const dir = join(root, "cloudstore");
      mkdirSync(dir);
      servers.push(
        spawn(join(repo, "target/debug/tronic-cloud.exe"), [], {
          stdio: "ignore",
          env: { ...process.env, DATABASE_URL: database, TRONIC_CLOUD_BIND: "127.0.0.1:18923", TRONIC_STORE_DIR: dir },
        }),
      );
      await waitPort(18923);
      const email = `e2e-${Date.now()}@example.test`;
      await pa.getByLabel("Art").selectOption("cloud");
      await pa.getByLabel("Name", { exact: true }).fill("Wolke");
      await pa.getByLabel("Adresse").fill("http://127.0.0.1:18923");
      await pa.getByLabel("E-Mail").fill(email);
      await pa.getByLabel("Passwort").fill("short");
      await pa.getByRole("checkbox", { name: "Neues Konto anlegen" }).check();
      await pa.getByRole("button", { name: "Hinzufügen" }).click();
      await pa.getByRole("alert").filter({ hasText: "mindestens 12 Zeichen" }).waitFor();
      await pa.getByLabel("Passwort").fill(CLOUD_PASSWORD);
      await pa.getByRole("button", { name: "Hinzufügen" }).click();
      await pa.locator(".ai-finding", { hasText: "Wolke" }).waitFor();
      await connectTarget(pa, "Wolke");
      await pa.getByRole("row", { name: /Wolke/ }).waitFor();
      await syncNow(pa);
      await notice(pa, "Wolke: Version gespeichert");
      // A second device with the same account, logging in.
      await pb.getByLabel("Art").selectOption("cloud");
      await pb.getByLabel("Name", { exact: true }).fill("Wolke");
      await pb.getByLabel("Adresse").fill("http://127.0.0.1:18923");
      await pb.getByLabel("E-Mail").fill(email);
      await pb.getByLabel("Passwort").fill("the wrong password");
      await pb.getByRole("button", { name: "Hinzufügen" }).click();
      await pb.getByRole("alert").filter({ hasText: "Der Speicher hat die Anmeldung abgelehnt." }).waitFor();
      await pb.getByLabel("Passwort").fill(CLOUD_PASSWORD);
      await pb.getByRole("button", { name: "Hinzufügen" }).click();
      await pb.locator(".ai-finding", { hasText: "Wolke" }).waitFor();
      await connectTarget(pb, "Wolke");
      await pb.getByRole("row", { name: /Wolke/ }).waitFor();
      await syncNow(pb);
      await pb.screenshot({ path: join(artifacts, "cloud-account.png") });
      await pa.getByRole("row", { name: /Wolke/ }).getByRole("button", { name: "Trennen" }).click();
      await pb.getByRole("row", { name: /Wolke/ }).getByRole("button", { name: "Trennen" }).click();
    });
  } else {
    console.log("      TRONICCAD_TEST_DATABASE_URL is not set: the cloud service through the interface did not run");
    results.push({ name: "the cloud service through the interface", ok: true, skipped: true });
  }

  await step("the panel fits in all four languages", async () => {
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(pa, language);
      await wait(100);
      await assertNoOverflow(pa, `cloud ${language}`);
      await pa.screenshot({ path: join(artifacts, `cloud-lang-${language}.png`) });
    }
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problemsA.length === 0, problemsA.join(" | "));
    assert(problemsB.length === 0, problemsB.join(" | "));
  });
} catch {
  await pa.screenshot({ path: join(artifacts, "cloud-failure-a.png") }).catch(() => undefined);
  await pb.screenshot({ path: join(artifacts, "cloud-failure-b.png") }).catch(() => undefined);
  exitCode = 1;
} finally {
  for (const app of [a, b]) {
    await app.browser.close().catch(() => undefined);
    try {
      execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // The process has already ended.
    }
  }
  for (const server of servers) server.kill();
  await wait(800);
  // Settings databases are locked while the application runs, so secrets are
  // looked for here, with every file readable.
  if (results.some((result) => result.name.startsWith("WebDAV through") && !result.skipped)) {
    try {
      await step("no password or key is in any file of the applications", async () => {
        const scan = (directory) => {
          for (const entry of readdirSync(directory, { withFileTypes: true })) {
            const path = join(directory, entry.name);
            if (entry.isDirectory()) scan(path);
            else if (!path.includes("webview-")) {
              const content = readFileSync(path);
              for (const secret of [WEBDAV_PASSWORD, S3_SECRET, CLOUD_PASSWORD]) {
                assert(!content.includes(secret), `a secret is in ${path}`);
              }
            }
          }
        };
        for (const directory of dataDirs) scan(directory);
        scan(join(root, "a"));
        scan(join(root, "b"));
      });
    } catch (error) {
      // A failing step reports itself. Anything else would otherwise go unseen.
      if (results.every((result) => result.ok)) console.error(`FAIL  before or between steps\n      ${String(error).slice(0, 1500)}`);
      exitCode = 1;
    }
  }
  // Only entries of this test run carry this service name.
  try {
    const listed = execFileSync("cmdkey", ["/list"], { encoding: "utf8" });
    for (const match of listed.matchAll(new RegExp(`[^\\s=]*${KEYRING_SERVICE}[^\\s]*`, "g"))) {
      execFileSync("cmdkey", [`/delete:${match[0]}`], { stdio: "ignore" });
    }
  } catch {
    // Nothing to clean up.
  }
  cleanUp(root);
  for (const directory of dataDirs) cleanUp(directory);
}

void safeInvoke;
const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "cloud-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
