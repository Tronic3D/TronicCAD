// End-to-end test of the AI assistant in the real application, against a real
// HTTP server on this computer that records every request it gets.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createServer as createHttp } from "node:http";
import { createServer as createTcp } from "node:net";
import { networkInterfaces, tmpdir } from "node:os";
import { join } from "node:path";
import { answerDialogsWith, artifacts, assert, assertNoOverflow, cleanUp, invoke, launch, selectLanguage, statusIs, useIsolatedData, wait } from "./lib.mjs";

const PORT = 9340;
const GRID = 1_270_000;
const KEYRING_SERVICE = `TronicCAD-e2e-${process.pid}`;
process.env.TRONICCAD_KEYRING_SERVICE = KEYRING_SERVICE;
const dataDir = useIsolatedData();
const root = mkdtempSync(join(tmpdir(), "troniccad-ai-"));
mkdirSync(artifacts, { recursive: true });
const projectPath = join(root, "GeheimerName.tronic");
const problems = [];
const results = [];
// Clearly not a real credential. It exists only for this test, on this computer.
const TEST_KEY = "e2e-test-key-not-real-0123456789";

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

// ---- The fake local model server. ----

const requests = [];
let script = [];
let release = null;

// The default count suits the size of these requests, as a real server would report it.
const sse = (pieces, finish = "stop", promptTokens = 1000) => {
  const lines = pieces.map((content) => `data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: null }] })}\n\n`);
  lines.push(`data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: finish }] })}\n\n`);
  lines.push(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: promptTokens, completion_tokens: 9 } })}\n\n`);
  lines.push("data: [DONE]\n\n");
  return lines;
};

const server = createHttp((request, response) => {
  let body = "";
  request.on("data", (part) => (body += part));
  request.on("end", async () => {
    requests.push({ method: request.method, url: request.url, headers: request.headers, body });
    if (request.url === "/v1/models") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ data: [{ id: "klein" }, { id: "gross" }] }));
    } else if (request.url === "/api/ps") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ models: [{ name: "klein", size: 2147483648, size_vram: 1073741824, context_length: 4096 }] }));
    } else {
      const next = script.shift() ?? { lines: sse(["ok"]) };
      if (next.status) {
        response.writeHead(next.status, { "content-type": "application/json" });
        response.end(JSON.stringify({ error: { message: next.message ?? "" } }));
        return;
      }
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const line of next.lines) response.write(line);
      if (next.hold) await new Promise((done) => (release = done));
      response.end();
    }
  });
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const serverUrl = `http://127.0.0.1:${server.address().port}`;
const chats = () => requests.filter((r) => r.url === "/v1/chat/completions");

// An address of this computer that is not loopback counts as external. Nothing
// here speaks TLS: what counts is whether a connection is attempted at all.
const lanAddress = Object.values(networkInterfaces())
  .flat()
  .find((item) => item.family === "IPv4" && !item.internal && !item.address.startsWith("169.254"))?.address;
let externalConnections = 0;
const externalServer = createTcp((socket) => {
  externalConnections += 1;
  socket.destroy();
});
await new Promise((done) => externalServer.listen(0, "0.0.0.0", done));

const app = await launch(PORT, problems);
const { page } = app;
const shot = (name) => page.screenshot({ path: join(artifacts, `ai-${name}.png`) });
const settled = () => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 60))));
const module = (name) => page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
const safeInvoke = (command, args) =>
  page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a).catch((error) => ({ rejected: error })), [command, args]);

let column = 0;
async function place(reference, symbol, value, pins) {
  const at = { x: column * 40 * GRID, y: 0 };
  column += 1;
  const result = await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "place_component", sheet: 1, symbol, at } });
  const id = result.created[0];
  const created = result.view.sheet.components.find((c) => c.id === id);
  await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "set_component", sheet: 1, id, reference, value: value ?? created.value } });
  const definition = result.view.symbols[symbol];
  for (const [number, name] of Object.entries(pins)) {
    const pin = definition.pins.find((p) => p.number === number);
    await invoke(page, "schematic_apply", { viewSheet: 1, command: { type: "add_label", sheet: 1, at: { x: at.x + pin.at.x, y: at.y + pin.at.y }, name, kind: "global" } });
  }
  return id;
}

const netlistText = async () => (await invoke(page, "schematic_netlist")).text;
const ask = async (text) => {
  await page.getByLabel("Ihre Anfrage").fill(text);
  await page.getByRole("button", { name: "Senden", exact: true }).click();
};
const setProvider = async (url, model = "klein") => {
  await page.getByLabel("Betriebsart").selectOption("local");
  await page.getByLabel("Adresse").fill(url);
  await page.getByLabel("Modell", { exact: true }).fill(model);
  await page.getByRole("button", { name: "Speichern", exact: true }).nth(1).click();
};

let exitCode = 0;
try {
  await step("sets up a project with known problems", async () => {
    await selectLanguage(page, "de-DE");
    await answerDialogsWith(page, projectPath);
    await page.getByRole("button", { name: /Neues Projekt/ }).click();
    await page.getByLabel("Projektname").fill("GeheimerName");
    await page.getByRole("button", { name: "Auswählen …" }).click();
    await page.getByText(projectPath).waitFor();
    await page.getByRole("button", { name: "Anlegen" }).click();
    await statusIs(page, "Projekt angelegt");
    await place("#PWR1", "supply:vcc", "+5V", { 1: "+5V" });
    await place("#PWR2", "supply:gnd", "GND", { 1: "GND" });
    await place("R1", "device:resistor", "47", { 1: "+5V", 2: "A" });
    await place("D1", "device:led", "rot", { 1: "A", 2: "GND" });
    await place("U1", "timer:ne555", "NE555", { 8: "+5V", 1: "GND" });
  });

  await step("without any provider the rules work and the module is real", async () => {
    await module("KI-Assistent");
    await page.getByText(/R1: etwa 63,8|D1 mit R1: etwa 63\.8 mA/).waitFor();
    await page.getByText("U1 hat keinen Abblockkondensator von +5V nach Masse.").waitFor();
    assert((await page.getByRole("button", { name: "Senden", exact: true }).isDisabled()) === true, "nothing to send to");
    assert((await page.locator(".rail-item-planned").count()) === 0, "no module says planned");
    assert(requests.length === 0, "no provider, no request");
    await shot("rules");
  });

  await step("the application process holds no connection outside this computer", async () => {
    const output = execFileSync(
      "powershell",
      ["-NoProfile", "-Command", `Get-NetTCPConnection -OwningProcess ${app.child.pid} -ErrorAction SilentlyContinue | ForEach-Object { $_.RemoteAddress } | Sort-Object -Unique; exit 0`],
      { encoding: "utf8" },
    );
    const outside = output.split(/\s+/).filter((a) => a && !["0.0.0.0", "::", "127.0.0.1", "::1"].includes(a));
    assert(outside.length === 0, `connections to ${outside}`);
  });

  await step("a local server is recognized as local, offers models and reports its load", async () => {
    await setProvider(serverUrl);
    await page.getByText("Lokal: Die Daten bleiben auf diesem Rechner.").waitFor();
    await page.getByRole("button", { name: "Verbindung testen" }).click();
    await page.getByText("Erreichbar. Verfügbare Modelle: 2.").waitFor();
    await page.getByText(/klein: 2 GB geladen, davon 1 GB auf der GPU/).waitFor();
    await page.getByText(/Arbeitsspeicher:/).waitFor();
    assert(chats().length === 0, "testing sends no project data");
  });

  await step("an answer streams in, marked as unchecked, and local needs no question", async () => {
    script.push({ lines: sse(["Der Strom ", "ist zu hoch."]) });
    await ask("Was fällt auf?");
    await page.locator(".ai-message-assistant").filter({ hasText: "Der Strom ist zu hoch." }).waitFor();
    await page.getByText("Antwort des Modells, ungeprüft").first().waitFor();
    await page.getByText(/1000 Tokens gesendet, 9 Tokens erhalten/).waitFor();
    assert((await page.getByRole("dialog").count()) === 0, "no consent dialog for local");
    await shot("answer");
  });

  await step("what was shown is exactly what went out, and paths and names stay home", async () => {
    await page.getByText("Gesendete Daten zeigen").click();
    const shown = await page.getByTestId("ai-payload").textContent();
    const body = JSON.parse(chats().at(-1).body);
    const sent = body.messages.map((m) => `[${m.role}]\n${m.content}`).join("\n\n");
    assert(shown === sent, `payload differs:\n${shown.slice(0, 300)}\n---\n${sent.slice(0, 300)}`);
    assert(body.model === "klein", body.model);
    assert(sent.includes("R1 | device:resistor | 47"), "selected data goes along");
    for (const secret of ["GeheimerName", root, projectPath]) assert(!sent.includes(secret), `leaked ${secret}`);
    assert(sent.includes("Answer in German"), "language");
    await page.getByText("Gesendete Daten zeigen").click();
  });

  await step("what is unticked does not go along", async () => {
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
    await page.getByRole("checkbox", { name: "Bauteile und Netze" }).uncheck();
    await ask("Nur eine Frage");
    await page.locator(".ai-message-assistant").filter({ hasText: "ok" }).waitFor();
    const body = JSON.parse(chats().at(-1).body);
    assert(body.messages.length === 2 && body.messages[1].content === "Nur eine Frage", body.messages[1].content);
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
    await page.getByRole("checkbox", { name: "Bauteile und Netze" }).check();
  });

  await step("the conversation continues with the earlier turns", async () => {
    await ask("Erste Frage");
    await page.locator(".ai-message-assistant").first().waitFor();
    await ask("Zweite Frage");
    await page.locator(".ai-message-assistant").nth(1).waitFor();
    const body = JSON.parse(chats().at(-1).body);
    assert(body.messages.length === 4, `${body.messages.length} messages`);
    assert(body.messages[2].role === "assistant", "turn order");
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  const proposalAnswer = [
    "Ich schlage Änderungen vor. Der ERC ist bestanden.\n\n```tronic-changes\n",
    JSON.stringify({
      changes: [
        { type: "set_value", reference: "R1", value: "330" },
        { type: "place", symbol: "device:capacitor", reference: "C7", value: "100n" },
        { type: "connect", reference: "C7", pin: "1", net: "+5V" },
        { type: "connect", reference: "C7", pin: "2", net: "GND" },
        { type: "set_value", reference: "D1", value: "gruen" },
        { type: "set_erc_result", passed: true },
        { type: "set_value", reference: "R99", value: "1" },
      ],
    }),
    "\n```",
  ];

  await step("a proposal changes nothing, and invented results and parts are refused", async () => {
    const before = await netlistText();
    const realErc = await page.locator(".ai-checks").textContent();
    script.push({ lines: sse(proposalAnswer) });
    await ask("Bitte verbessern");
    await page.getByRole("region", { name: "Vorgeschlagene Änderungen" }).waitFor();
    await page.getByText("R1: Wert auf 330 setzen").waitFor();
    await page.getByText("device:capacitor als C7 platzieren, Wert 100n").waitFor();
    await page.getByText(/Keine erlaubte Änderung: .*set_erc_result/).waitFor();
    await page.getByText("Dieses Bauteil gibt es nicht: R99").waitFor();
    await page.getByLabel("Echter ERC vorher und nachher").waitFor();
    assert((await netlistText()) === before, "the schematic must not change yet");
    assert((await page.locator(".ai-checks").textContent()) === realErc, "the model's claim must not touch the real result");
    const asked = await page.locator(".ai-message-user").last().textContent();
    assert(!asked.includes("project_data"), "the conversation shows the question, not the data");
    const text = await page.locator(".ai-message-assistant").last().textContent();
    assert(text.includes("ungeprüft") && text.includes("bestanden"), "its claim stays its own, labelled");
    assert(!text.includes("tronic-changes"), "the block is not shown as text");
    await shot("proposal");
  });

  await step("the user picks, it applies as one undo step, and the real checks run", async () => {
    await page.getByRole("checkbox", { name: "D1: Wert auf gruen setzen" }).uncheck();
    await page.getByRole("button", { name: "Übernehmen (4)" }).click();
    await page.getByText(/Die Änderungen wurden übernommen/).waitFor();
    const after = await netlistText();
    assert(after.includes("C7"), "placed");
    assert(!after.includes("gruen"), "not chosen, not applied");
    await page.getByText("D1 mit R1: etwa 9,1 mA bei 5,00 V").or(page.getByText(/D1 mit R1: etwa 9\.1 mA/)).waitFor();
    assert((await page.getByText("U1 hat keinen Abblockkondensator").count()) === 0, "the rules see the capacitor");
    await statusIs(page, "Ungespeicherte Änderungen");
    await invoke(page, "schematic_undo", { sheet: 1 });
    const undone = await netlistText();
    assert(!undone.includes("C7"), "one undo removes the whole proposal");
    await invoke(page, "schematic_redo", { sheet: 1 });
  });

  await step("a proposal for an outdated schematic is refused", async () => {
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
    script.push({ lines: sse([`\`\`\`tronic-changes\n${JSON.stringify({ changes: [{ type: "delete", reference: "C7" }] })}\n\`\`\``]) });
    await ask("Noch etwas");
    await page.getByText("C7 löschen").waitFor();
    await place("R5", "device:resistor", "1k", {});
    await page.getByRole("button", { name: "Übernehmen (1)" }).click();
    await page.getByRole("alert").filter({ hasText: "Der Schaltplan hat sich seit dem Vorschlag geändert" }).waitFor();
    assert((await netlistText()).includes("C7"), "not applied");
  });

  await step("a proposal can be discarded", async () => {
    script.push({ lines: sse([`\`\`\`tronic-changes\n${JSON.stringify({ changes: [{ type: "delete", reference: "R5" }] })}\n\`\`\``]) });
    await ask("Noch einmal");
    await page.getByText("R5 löschen").waitFor();
    await page.getByRole("button", { name: "Verwerfen" }).click();
    await page.getByText("R5 löschen").waitFor({ state: "detached" });
    assert((await netlistText()).includes("R5"), "still there");
    const gone = await safeInvoke("ai_apply", { indices: [0] });
    assert(gone.rejected?.code === "ai.noProposal", JSON.stringify(gone));
  });

  await step("stopping ends the wait at once and drops the answer", async () => {
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
    const before = chats().length;
    script.push({ lines: sse(["Anfang "]).slice(0, 1), hold: true });
    await ask("Lange Antwort");
    await page.getByText("Anfang").waitFor();
    await page.getByRole("button", { name: "Anhalten" }).click();
    await page.getByRole("button", { name: "Senden", exact: true }).waitFor();
    release();
    await wait(200);
    assert((await page.locator(".ai-message-assistant").count()) === 0, "a stopped answer does not stay");
    assert(chats().length === before + 1, "stopping must not send anything again");
  });

  await step("a model server that fails says so", async () => {
    script.push({ status: 500, message: "Modell abgestuerzt" });
    await ask("Frage");
    await page.getByRole("alert").filter({ hasText: "Der Anbieter meldet Status 500: Modell abgestuerzt" }).waitFor();
    script.push({ status: 401 });
    await ask("Frage");
    await page.getByRole("alert").filter({ hasText: "Der Anbieter hat den Schlüssel abgelehnt." }).waitFor();
    script.push({ lines: sse(["unvollst"], "length") });
    await ask("Frage");
    await page.getByText("Die Antwort hat die Längengrenze erreicht und ist unvollständig.").waitFor();
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  await step("a request the server may have cut is flagged, a normal one is not", async () => {
    await page.getByRole("button", { name: "Neues Gespräch" }).click().catch(() => undefined);
    assert((await page.getByRole("alert").filter({ hasText: "abgeschnitten" }).count()) === 0, "no alarm so far");
    script.push({ lines: sse(["gekappt"], "stop", 2050) });
    await ask("Viele Daten");
    await page.getByRole("alert").filter({ hasText: /abgeschnitten.*4096/ }).waitFor();
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
    script.push({ lines: sse(["normal"]) });
    await ask("Wenig Daten");
    await page.locator(".ai-message-assistant").filter({ hasText: "normal" }).waitFor();
    assert((await page.getByRole("alert").filter({ hasText: "abgeschnitten" }).count()) === 0, "no alarm for a short request");
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  await step("answers can be saved", async () => {
    script.push({ lines: sse(["# Dokumentation\nInhalt"]) });
    await ask("Doku bitte");
    await page.locator(".ai-message-assistant").filter({ hasText: "Inhalt" }).waitFor();
    const path = join(root, "doku.md");
    await answerDialogsWith(page, path);
    await page.getByRole("button", { name: "Antwort speichern" }).click();
    for (let i = 0; i < 40; i++) {
      try {
        if (readFileSync(path, "utf8").includes("# Dokumentation")) break;
      } catch {
        await wait(50);
      }
    }
    assert(readFileSync(path, "utf8").includes("Inhalt"), "file content");
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  const keyInStore = () => execFileSync("cmdkey", ["/list"], { encoding: "utf8" }).includes(KEYRING_SERVICE);

  await step("the key goes into the key store of the system and nowhere else", async () => {
    await page.getByLabel("Neuer Schlüssel").fill(TEST_KEY);
    await page.getByRole("button", { name: "Schlüssel speichern" }).click();
    await page.getByText("Ein Schlüssel liegt im Schlüsselspeicher des Betriebssystems.").waitFor();
    assert((await page.getByLabel("Neuer Schlüssel").inputValue()) === "", "the field is cleared");
    assert(keyInStore(), "in the Windows credential store");
    const settings = await invoke(page, "ai_settings");
    assert(!JSON.stringify(settings).includes(TEST_KEY), "never comes back to the interface");
    assert(settings.has_key === true, "only that it exists");
    script.push({ lines: sse(["mit Schluessel"]) });
    await ask("Test");
    await page.locator(".ai-message-assistant").filter({ hasText: "mit Schluessel" }).waitFor();
    assert(chats().at(-1).headers.authorization === `Bearer ${TEST_KEY}`, "sent as a header");
    assert(!chats().at(-1).body.includes(TEST_KEY), "not in the body");
    await page.keyboard.press("Control+s");
    await statusIs(page, "Gespeichert");
    await wait(300);
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  await step("deleting the key removes it from the store", async () => {
    await page.getByRole("button", { name: "Schlüssel löschen" }).click();
    await page.getByText("Kein Schlüssel gespeichert.").waitFor();
    assert(!keyInStore(), "gone");
    const before = chats().length;
    script.push({ lines: sse(["ohne"]) });
    await ask("Test");
    await page.locator(".ai-message-assistant").filter({ hasText: "ohne" }).waitFor();
    assert(chats().length === before + 1 && !chats().at(-1).headers.authorization, "no key any more");
    await page.getByRole("button", { name: "Neues Gespräch" }).click();
  });

  await step("addresses that would expose data are refused", async () => {
    const refused = await safeInvoke("ai_set_provider", { config: { kind: "open_ai_compatible", base_url: "http://example.com", model: "m", max_tokens: null } });
    assert(refused.rejected?.code === "ai.insecureUrl", JSON.stringify(refused));
    await page.getByLabel("Adresse").fill("http://example.com");
    await page.getByRole("button", { name: "Speichern", exact: true }).nth(1).click();
    await page.getByRole("alert").filter({ hasText: "Externe Anbieter müssen https verwenden" }).waitFor();
    await page.getByLabel("Adresse").fill(serverUrl);
  });

  if (lanAddress) {
    const external = `https://${lanAddress}:${externalServer.address().port}`;
    const prepare = () =>
      invoke(page, "ai_prepare", {
        ask: { task: "free", text: "x", document: "", sections: { schematic: true, erc: false, board: false, drc: false, library: false }, language: "de-DE", history: [] },
      });

    await step("an external provider gets nothing without consent, enforced by the backend", async () => {
      await page.getByLabel("Betriebsart").selectOption("custom");
      await page.getByLabel("Adresse").fill(external);
      await page.getByLabel("Modell", { exact: true }).fill("fern");
      await page.getByRole("button", { name: "Speichern", exact: true }).nth(1).click();
      await page.getByText("Extern: Die Daten verlassen diesen Rechner, nur mit Ihrer Zustimmung.").waitFor();

      // The interface asks, shows the data, and cancelling sends nothing.
      await ask("Geheime Frage");
      const dialog = page.getByRole("dialog", { name: "Projektdaten an externen Anbieter senden?" });
      await dialog.waitFor();
      const shown = await dialog.getByTestId("ai-payload").textContent();
      assert(shown.includes("Geheime Frage") && shown.includes("R1 | device:resistor"), "shows what would go out");
      assert((await dialog.textContent()).includes(lanAddress), "names the destination");
      await shot("consent");
      await dialog.getByRole("button", { name: "Abbrechen" }).click();
      await wait(400);
      assert(externalConnections === 0, `${externalConnections} connections after cancelling`);

      // Going around the interface does not help.
      const prepared = await prepare();
      assert(prepared.needs_consent === true && prepared.locality === "external", JSON.stringify(prepared).slice(0, 200));
      const refused = await safeInvoke("ai_send", { id: prepared.id, consent: "none" });
      assert(refused.rejected?.code === "ai.consentRequired", JSON.stringify(refused));
      await wait(400);
      assert(externalConnections === 0, `${externalConnections} connections without consent`);
    });

    await step("consent once allows one request, and only that", async () => {
      await ask("Einmal");
      const dialog = page.getByRole("dialog");
      await dialog.getByRole("button", { name: "Einmal senden" }).click();
      await page.getByRole("alert").filter({ hasText: "nicht erreichbar" }).waitFor();
      assert(externalConnections > 0, "the request did go out");
      const count = externalConnections;
      const prepared = await prepare();
      assert(prepared.needs_consent === true, "asks again");
      const refused = await safeInvoke("ai_send", { id: prepared.id, consent: "none" });
      assert(refused.rejected?.code === "ai.consentRequired", JSON.stringify(refused));
      await wait(300);
      assert(externalConnections === count, "nothing more went out");
    });

    await step("consent for the project lasts until the project closes", async () => {
      await ask("Dauerhaft");
      await page.getByRole("dialog").getByRole("button", { name: "Für dieses Projekt erlauben" }).click();
      await page.getByRole("alert").filter({ hasText: "nicht erreichbar" }).waitFor();
      const prepared = await prepare();
      assert(prepared.needs_consent === false, "no question any more");
      await page.keyboard.press("Control+s");
      await statusIs(page, "Gespeichert");
      await page.getByRole("button", { name: "Schließen", exact: true }).click();
      await statusIs(page, "Kein Projekt geöffnet");
      await invoke(page, "project_open", { path: projectPath });
      const again = await prepare();
      assert(again.needs_consent === true, "closing the project ended the consent");
      await safeInvoke("project_close", {});
      await answerDialogsWith(page, projectPath);
      await page.getByRole("button", { name: "Öffnen", exact: true }).click();
      await statusIs(page, "Projekt geöffnet");
      await module("KI-Assistent");
      await page.getByLabel("Betriebsart").selectOption("rules");
      await page.getByText("Die Regelanalyse arbeitet ohne Modell").waitFor();
    });
  } else {
    console.log("      no network address on this computer: the external provider steps did not run");
    results.push({ name: "external provider steps", ok: true, skipped: true });
  }

  await step("the assistant fits in all four languages", async () => {
    for (const language of ["en-US", "zh-CN", "fr-FR", "de-DE"]) {
      await selectLanguage(page, language);
      await settled();
      await assertNoOverflow(page, `assistant ${language}`);
      await shot(`lang-${language}`);
    }
  });

  await step("no errors or CSP violations in the console", async () => {
    assert(problems.length === 0, problems.join(" | "));
  });
} catch {
  await page.screenshot({ path: join(artifacts, "ai-failure.png") }).catch(() => undefined);
  exitCode = 1;
} finally {
  await app.browser.close().catch(() => undefined);
  try {
    execFileSync("taskkill", ["/PID", String(app.child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // The process has already ended.
  }
  await wait(600);
  // The settings database stays locked while the application runs, so the search
  // for the key happens here, with every file readable.
  if (results.some((result) => result.name.startsWith("the key goes into"))) {
    try {
      await step("the key is in no file of the application or the project", async () => {
        const scan = (directory) => {
          let files = 0;
          for (const entry of readdirSync(directory, { withFileTypes: true })) {
            const path = join(directory, entry.name);
            if (entry.isDirectory()) files += scan(path);
            else {
              assert(!readFileSync(path).includes(TEST_KEY), `the key is in ${path}`);
              files += 1;
            }
          }
          return files;
        };
        const seen = scan(dataDir) + scan(root);
        assert(seen >= 2, `only ${seen} files searched`);
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
  server.closeAllConnections?.();
  server.close();
  externalServer.close();
  cleanUp(root);
  cleanUp(dataDir);
}

const passed = results.filter((result) => result.ok).length;
writeFileSync(join(artifacts, "ai-results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(`\n${passed}/${results.length} steps passed`);
process.exit(exitCode);
