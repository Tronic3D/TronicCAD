// End-to-end test of teamwork: two application instances, each with its own account,
// against a real team server and PostgreSQL. Needs TRONICCAD_TEST_DATABASE_URL.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, repo, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const database = process.env.TRONICCAD_TEST_DATABASE_URL;
mkdirSync(artifacts, { recursive: true });
if (!database) {
  console.log("TRONICCAD_TEST_DATABASE_URL is not set: the team test did not run");
  writeFileSync(join(artifacts, "team-results.json"), `${JSON.stringify([{ name: "team", skipped: true }], null, 2)}\n`);
  console.log("\n0/0 steps passed");
  process.exit(0);
}

const GRID = 1_270_000;
const KEYRING_SERVICE = `TronicCAD-e2e-team-${process.pid}`;
process.env.TRONICCAD_KEYRING_SERVICE = KEYRING_SERVICE;
const root = mkdtempSync(join(tmpdir(), "troniccad-team-"));
const results = [];
const problemsA = [];
const problemsB = [];
const SERVER = "http://127.0.0.1:18931";
const PASSWORD = "e2e team password not real";
const stamp = Date.now();
const emailA = `lead-${stamp}@example.test`;
const emailB = `dev-${stamp}@example.test`;

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
  process.env.WEBVIEW2_USER_DATA_FOLDER = join(root, `webview-${name}`);
  return launch(port, problems);
}

mkdirSync(join(root, "store"));
mkdirSync(join(root, "a"));
mkdirSync(join(root, "b"));
const server = spawn(join(repo, "target/debug/tronic-cloud.exe"), [], {
  stdio: "ignore",
  env: { ...process.env, DATABASE_URL: database, TRONIC_CLOUD_BIND: "127.0.0.1:18931", TRONIC_STORE_DIR: join(root, "store") },
});
for (let i = 0; i < 100; i++) {
  const open = await new Promise((done) => {
    const socket = connect(18931, "127.0.0.1", () => {
      socket.destroy();
      done(true);
    });
    socket.on("error", () => done(false));
  });
  if (open) break;
  await wait(100);
}

const a = await device(9344, problemsA, "a");
const b = await device(9345, problemsB, "b");
const [pa, pb] = [a.page, b.page];
const pathA = join(root, "a", "Gemeinsam.tronic");
const pathB = join(root, "b", "Gemeinsam.tronic");

const module = (page, name) => page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
const netlist = async (page) => (await invoke(page, "schematic_netlist")).text;
const save = async (page) => {
  await page.keyboard.press("Control+s");
  await statusIs(page, "Gespeichert");
};

let column = 1;
async function place(page, reference, value) {
  const at = { x: column * 40 * GRID, y: 0 };
  column += 1;
  const result = await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "place_component", sheet: 1, symbol: "device:resistor", at } });
  await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "set_component", sheet: 1, id: result.created[0], reference, value } });
}
async function setValue(page, reference, value) {
  const view = await invoke(page, "schematic_view", { sheet: 1 });
  const part = view.sheet.components.find((c) => c.reference === reference);
  assert(part, `${reference} exists`);
  await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "set_component", sheet: 1, id: part.id, reference, value } });
}
const setBoard = (page, size) => {
  const mm = 1_000_000;
  return invoke(page, "pcb_apply", {
    command: { type: "set_outline", points: [{ x: 0, y: 0 }, { x: size * mm, y: 0 }, { x: size * mm, y: size * mm }, { x: 0, y: size * mm }] },
  });
};
const account = async (page, email, create) => {
  await module(page, "Projekt");
  await module(page, "Cloud");
  await page.getByLabel("Art").selectOption("cloud");
  await page.getByLabel("Name", { exact: true }).fill("Teamserver");
  await page.getByLabel("Adresse").fill(SERVER);
  await page.getByLabel("E-Mail").fill(email);
  await page.getByLabel("Passwort").fill(PASSWORD);
  if (create) await page.getByRole("checkbox", { name: "Neues Konto anlegen" }).check();
  await page.getByRole("button", { name: "Hinzufügen" }).click();
  await page.locator(".ai-finding", { hasText: "Teamserver" }).waitFor();
};
const syncNow = async (page) => {
  await page.getByRole("button", { name: "Jetzt synchronisieren" }).click();
  await page.getByRole("button", { name: "Jetzt synchronisieren" }).waitFor();
};
const openCloud = async (page) => {
  await module(page, "Projekt");
  await module(page, "Cloud");
  await page.getByRole("heading", { name: "Dieses Projekt" }).waitFor();
};
const openTeam = async (page) => {
  await module(page, "Projekt");
  await module(page, "Team");
  await page.getByLabel("Server").waitFor();
};
const notice = (page, text) => page.locator(".sim-results [role=status]").filter({ hasText: text }).waitFor();
const schematicRow = (page) => page.getByRole("region", { name: "Bereiche und Sperren" }).getByRole("row", { name: /Schaltplan/ });

let code;
let closed;
let exitCode = 0;
try {
  await step("A creates an account, a team and puts a project in it", async () => {
    await selectLanguage(pa, "de-DE");
    await answerDialogsWith(pa, pathA);
    await pa.getByRole("button", { name: /Neues Projekt/ }).click();
    await pa.getByLabel("Projektname").fill("Gemeinsam");
    await pa.getByRole("button", { name: "Auswählen …" }).click();
    await pa.getByText(pathA).waitFor();
    await pa.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(pa, "Projekt angelegt");
    await place(pa, "R1", "1k");
    await save(pa);
    await account(pa, emailA, true);
    await openTeam(pa);
    await pa.getByLabel("Neues Team").fill("Entwicklung");
    await pa.getByRole("button", { name: "Team anlegen" }).click();
    await pa.getByText("Ihre Rolle: Administrator").waitFor();
    await pa.getByRole("button", { name: "Dieses Projekt ins Team stellen" }).click();
    await notice(pa, "gehört jetzt zum Team");
    await openCloud(pa);
    await syncNow(pa);
    await notice(pa, "Version gespeichert");
  });

  await step("A invites B, and B joins by code and fetches the project", async () => {
    await openTeam(pa);
    await pa.getByLabel("Einladen per E-Mail-Adresse").fill(emailB);
    await pa.locator(".team-row").getByLabel("Rolle").selectOption("developer");
    await pa.getByRole("button", { name: "Einladung erstellen" }).click();
    code = await pa.getByTestId("invite-code").textContent();
    assert(code.length > 20, code);

    await selectLanguage(pb, "de-DE");
    await answerDialogsWith(pb, join(root, "b", "Eigen.tronic"));
    await pb.getByRole("button", { name: /Neues Projekt/ }).click();
    await pb.getByLabel("Projektname").fill("Eigen");
    await pb.getByRole("button", { name: "Auswählen …" }).click();
    await pb.getByText(join(root, "b", "Eigen.tronic")).waitFor();
    await pb.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(pb, "Projekt angelegt");
    await account(pb, emailB, true);
    await openTeam(pb);
    await pb.getByLabel("Einladungscode").fill("wrong");
    await pb.getByRole("button", { name: "Beitreten" }).click();
    await pb.getByRole("alert").filter({ hasText: "Diese Einladung gilt nicht" }).waitFor();
    await pb.getByLabel("Einladungscode").fill(code);
    await pb.getByRole("button", { name: "Beitreten" }).click();
    await notice(pb, "beigetreten");
    await pb.getByText("Ihre Rolle: Entwickler").waitFor();
    await answerDialogsWith(pb, pathB);
    await pb.getByRole("region", { name: "Projekte" }).getByRole("button", { name: "Holen" }).click();
    await statusIs(pb, "Projekt geöffnet");
    assert((await netlist(pb)).includes("1k"), "B has the project");
    assert(existsSync(pathB), "as a file");
  });

  await step("A locks the schematic; B is refused there, with the reason", async () => {
    await openTeam(pa);
    await schematicRow(pa).getByRole("button", { name: "Schaltplan sperren" }).click();
    await schematicRow(pa).getByText(/Gesperrt von Ihnen/).waitFor();
    await pa.screenshot({ path: join(artifacts, "team-lock.png") });

    await openTeam(pb);
    await schematicRow(pb).getByText(new RegExp(`Gesperrt von ${emailA}`)).waitFor();
    assert((await schematicRow(pb).getByRole("button").count()) === 0, "B cannot take or end it");

    await setValue(pb, "R1", "from-B");
    await save(pb);
    const saved = readFileSync(pathB);
    await openCloud(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pb, `${emailA} hält die Sperre`);
    assert(Buffer.compare(saved, readFileSync(pathB)) === 0, "B's file stays as it was");
    await pb.screenshot({ path: join(artifacts, "team-refused.png") });
    // B goes back and works elsewhere.
    await setValue(pb, "R1", "1k");
    await save(pb);
  });

  await step("A on the schematic and B on the board at once: both arrive", async () => {
    await setBoard(pb, 60);
    await save(pb);
    await syncNow(pb);
    await notice(pb, "Version gespeichert");

    await setValue(pa, "R1", "from-A");
    await save(pa);
    await openCloud(pa);
    await pa.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pa, "zusammengeführt");
    await pa.screenshot({ path: join(artifacts, "team-merged.png") });
    assert((await netlist(pa)).includes("from-A"), "A keeps the schematic");
    const boardA = (await invoke(pa, "pcb_view")).board;
    assert(boardA.outline.length === 4, "A has B's board");

    await syncNow(pb);
    await notice(pb, "neuere Version geladen");
    assert((await netlist(pb)).includes("from-A"), "B has A's schematic");
    assert((await invoke(pb, "pcb_view")).board.outline.length === 4, "B keeps the board");
  });

  await step("the same area changed on both sides stays for a person", async () => {
    await openTeam(pa);
    await schematicRow(pa).getByRole("button", { name: "Freigeben" }).click();
    await schematicRow(pa).getByText("Frei", { exact: true }).waitFor();
    await setValue(pa, "R1", "second-A");
    await save(pa);
    await openCloud(pa);
    await syncNow(pa);
    await notice(pa, "Version gespeichert");
    await setValue(pb, "R1", "second-B");
    await save(pb);
    await openCloud(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    const dialog = pb.getByRole("dialog", { name: "Es gibt zwei Stände" });
    await dialog.waitFor();
    assert((await netlist(pb)).includes("second-B"), "nothing was overwritten");
    await dialog.getByRole("button", { name: "Mit dem anderen Stand weiter" }).click();
    await notice(pb, "Eine Kopie liegt unter");
    assert((await netlist(pb)).includes("second-A"), "B follows A");
  });

  await step("an administrator can end someone's lock, and the holder learns about it", async () => {
    await openTeam(pb);
    await schematicRow(pb).getByRole("button", { name: "Schaltplan sperren" }).click();
    await schematicRow(pb).getByText(/Gesperrt von Ihnen/).waitFor();
    await openTeam(pa);
    await schematicRow(pa).getByRole("button", { name: "Sperre aufheben" }).click();
    await schematicRow(pa).getByText("Frei", { exact: true }).waitFor();
    await openTeam(pb);
    await pb.getByText(new RegExp(`${emailA} hat Ihre Sperre`)).waitFor();
    await pb.screenshot({ path: join(artifacts, "team-note.png") });
  });

  await step("roles change what the interface and the server allow", async () => {
    await openTeam(pa);
    await pa.getByRole("combobox", { name: `Rolle von ${emailB}` }).selectOption("viewer");
    await wait(300);
    await openTeam(pb);
    await pb.getByText("Ihre Rolle: Betrachter").waitFor();
    assert((await schematicRow(pb).getByRole("button", { name: "Schaltplan sperren" }).count()) === 0, "no lock for a viewer");
    assert((await pb.getByRole("button", { name: "Kommentar hinzufügen" }).count()) === 0, "no comments for a viewer");
    await setValue(pb, "R1", "viewer-try");
    await save(pb);
    await openCloud(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pb, "Keine Berechtigung");
    await setValue(pb, "R1", "second-A");
    await save(pb);
    await openTeam(pa);
    await pa.getByRole("combobox", { name: `Rolle von ${emailB}` }).selectOption("developer");
    await wait(300);
  });

  await step("the last administrator cannot leave the team without one", async () => {
    await pa.getByRole("combobox", { name: `Rolle von ${emailA}` }).selectOption("viewer");
    await pa.getByRole("alert").filter({ hasText: "mindestens einen Administrator" }).waitFor();
  });

  await step("comments reach the others, who see who is working where", async () => {
    await openTeam(pa);
    await pa.getByLabel("Ich arbeite an").selectOption("board");
    await pa.getByLabel("Bauteil oder Stelle").fill("R1");
    await pa.getByLabel("Kommentar", { exact: true }).fill("Bitte den Wert prüfen");
    await pa.getByRole("button", { name: "Kommentar hinzufügen" }).click();
    await pa.getByText("Bitte den Wert prüfen").waitFor();
    await openTeam(pb);
    await pb.getByText("Bitte den Wert prüfen").waitFor();
    await pb.getByText(new RegExp(`${emailA} hat kommentiert`)).waitFor();
    await pb.getByText(new RegExp(`Gerade da: .*${emailA}: Leiterplatte`)).waitFor();
    await pb.getByRole("region", { name: "Mitglieder" }).getByRole("row", { name: new RegExp(emailA) }).getByText("online").waitFor();
    await pb.screenshot({ path: join(artifacts, "team-comments.png") });
    await pb.getByRole("button", { name: "Als gelesen markieren" }).click();
    await pb.getByRole("button", { name: "Als gelesen markieren" }).waitFor({ state: "detached" });
  });

  await step("a contributor can only change what the interface assigned", async () => {
    await openTeam(pa);
    await pa.getByRole("combobox", { name: `Rolle von ${emailB}` }).selectOption("contributor");
    await pa.getByRole("heading", { name: "Bereiche zuweisen" }).waitFor();
    await setBoard(pb, 70);
    await save(pb);
    await openCloud(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pb, "Keine Berechtigung");
    await openTeam(pa);
    const row = pa.getByRole("region", { name: "Bereiche und Sperren" }).getByRole("row", { name: new RegExp(emailB) });
    await row.getByRole("checkbox", { name: "Leiterplatte" }).check();
    await wait(300);
    await openCloud(pb);
    await pb.getByRole("button", { name: "Jetzt synchronisieren" }).click();
    await notice(pb, "Version gespeichert");
    await pa.screenshot({ path: join(artifacts, "team-assign.png") });
    await openTeam(pa);
    await pa.getByRole("combobox", { name: `Rolle von ${emailB}` }).selectOption("developer");
    await wait(300);
  });

  await step("on a server with closed registration an invitation opens the door", async () => {
    closed = spawn(join(repo, "target/debug/tronic-cloud.exe"), [], {
      stdio: "ignore",
      env: { ...process.env, DATABASE_URL: database, TRONIC_CLOUD_BIND: "127.0.0.1:18932", TRONIC_STORE_DIR: join(root, "store"), TRONIC_ALLOW_REGISTRATION: "false" },
    });
    for (let i = 0; i < 100; i++) {
      const open = await new Promise((done) => {
        const socket = connect(18932, "127.0.0.1", () => {
          socket.destroy();
          done(true);
        });
        socket.on("error", () => done(false));
      });
      if (open) break;
      await wait(100);
    }
    const emailC = `new-${stamp}@example.test`;
    await openTeam(pa);
    await pa.getByLabel("Einladen per E-Mail-Adresse").fill(emailC);
    await pa.getByRole("button", { name: "Einladung erstellen" }).click();
    await pa.getByTestId("invite-code").filter({ hasNotText: code }).waitFor();
    const second = await pa.getByTestId("invite-code").textContent();
    await module(pb, "Projekt");
    await module(pb, "Cloud");
    await pb.getByLabel("Art").selectOption("cloud");
    await pb.getByLabel("Name", { exact: true }).fill("Geschlossen");
    await pb.getByLabel("Adresse").fill("http://127.0.0.1:18932");
    await pb.getByLabel("E-Mail").fill(emailC);
    await pb.getByLabel("Passwort").fill(PASSWORD);
    await pb.getByRole("checkbox", { name: "Neues Konto anlegen" }).check();
    await pb.getByRole("button", { name: "Hinzufügen" }).click();
    await pb.getByRole("alert").filter({ hasText: "Keine Berechtigung" }).waitFor();
    await pb.getByLabel("Einladungscode").fill(second);
    await pb.getByRole("button", { name: "Hinzufügen" }).click();
    await pb.locator(".ai-finding", { hasText: "Geschlossen" }).waitFor();
  });

  await step("the audit log shows what happened, in order and intact", async () => {
    await openTeam(pa);
    const section = pa.getByRole("region", { name: "Audit-Protokoll" });
    await section.getByRole("button", { name: "Anzeigen" }).click();
    await section.getByText("Geprüft: Die Protokollkette ist intakt.").waitFor();
    const text = await section.textContent();
    for (const entry of ["Team angelegt", "Projekt hinzugefügt", "Einladung erstellt", "Einladung angenommen", "Sperre gesetzt", "Sperre freigegeben", "Sperre von anderer Person aufgehoben", "Rolle geändert", "Kommentar hinzugefügt", "Version gespeichert"]) {
      assert(text.includes(entry), `missing: ${entry}`);
    }
    await pa.screenshot({ path: join(artifacts, "team-audit.png"), fullPage: true });
    await openTeam(pb);
    assert((await pb.getByRole("region", { name: "Audit-Protokoll" }).count()) === 0, "not for a developer");
  });

  await step("the panel fits in all four languages", async () => {
    await openTeam(pa);
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(pa, language);
      await wait(100);
      await assertNoOverflow(pa, `team ${language}`);
      await pa.screenshot({ path: join(artifacts, `team-lang-${language}.png`) });
    }
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problemsA.length === 0, problemsA.join(" | "));
    assert(problemsB.length === 0, problemsB.join(" | "));
  });
} catch {
  await pa.screenshot({ path: join(artifacts, "team-failure-a.png") }).catch(() => undefined);
  await pb.screenshot({ path: join(artifacts, "team-failure-b.png") }).catch(() => undefined);
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
  server.kill();
  closed?.kill();
  await wait(800);
  if (results.some((result) => result.name.startsWith("A creates"))) {
    try {
      await step("no password or invitation code is in any file of the applications", async () => {
        const scan = (directory) => {
          for (const entry of readdirSync(directory, { withFileTypes: true })) {
            const path = join(directory, entry.name);
            if (entry.isDirectory()) scan(path);
            else if (!path.includes("webview-")) {
              const content = readFileSync(path);
              assert(!content.includes(PASSWORD), `the password is in ${path}`);
              if (code) assert(!content.includes(code), `the code is in ${path}`);
            }
          }
        };
        for (const directory of dataDirs) scan(directory);
      });
    } catch (error) {
      // A failing step reports itself. Anything else would otherwise go unseen.
      if (results.every((result) => result.ok)) console.error(`FAIL  before or between steps\n      ${String(error).slice(0, 1500)}`);
      exitCode = 1;
    }
  }
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

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "team-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
