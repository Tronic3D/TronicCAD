#!/usr/bin/env node
// Runs every automated check and writes reports/QUALITY_REPORT.md from the
// actual results. Exit code 1 if any required check fails.
//
//   node scripts/quality.mjs          all checks
//   node scripts/quality.mjs --no-e2e without the application end-to-end test
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ui = join(repo, "apps/desktop");
const skipE2e = process.argv.includes("--no-e2e") || process.platform !== "win32";
const npx = process.platform === "win32" ? "npx.cmd" : "npx";
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

const checks = [
  ["Rust-Formatierung", "Qualität", "cargo", ["fmt", "--all", "--check"], repo],
  ["Rust-Lint (clippy, Warnungen sind Fehler)", "Qualität", "cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], repo],
  ["Rust-Tests", "Tests", "cargo", ["test", "--workspace", "--no-fail-fast"], repo],
  ["Rust-Schwachstellen (cargo audit)", "Sicherheit", "cargo", ["audit"], repo],
  ["Rust-Lizenzen und Quellen (cargo deny)", "Lizenz", "cargo", ["deny", "check", "licenses", "sources", "bans"], repo],
  ["TypeScript-Typprüfung", "Qualität", npx, ["tsc", "--noEmit"], ui],
  ["UI-Tests (vitest)", "Tests", npx, ["vitest", "run"], ui],
  ["npm-Schwachstellen, ausgelieferte Pakete", "Sicherheit", npm, ["audit", "--omit=dev"], ui],
  ["Lizenzliste, SBOM und Positivliste", "Lizenz", "node", ["scripts/licenses.mjs", "--check"], repo],
  ["Keine Schlüssel und Zugangsdaten im Repository", "Sicherheit", "node", ["scripts/secrets_check.mjs"], repo],
];
if (!skipE2e) {
  checks.push(["Anwendung und Installer bauen", "Tests", npx, ["tauri", "build", "--debug", "--bundles", "nsis"], ui]);
  checks.push(["End-to-End gegen die echte Anwendung", "Tests", "node", ["e2e/smoke.mjs"], ui]);
  checks.push(["End-to-End Schaltplan mit Maus und Tastatur", "Tests", "node", ["e2e/schematic.mjs"], ui]);
  checks.push(["End-to-End Absturz und Wiederherstellung", "Tests", "node", ["e2e/recovery.mjs"], ui]);
  checks.push(["End-to-End Leiterplatte mit Maus und Tastatur", "Tests", "node", ["e2e/pcb.mjs"], ui]);
  checks.push(["End-to-End Autorouter", "Tests", "node", ["e2e/router.mjs"], ui]);
  checks.push(["End-to-End Simulation und 3D", "Tests", "node", ["e2e/sim3d.mjs"], ui]);
  checks.push(["End-to-End KI-Assistent", "Tests", "node", ["e2e/ai.mjs"], ui]);
  checks.push(["End-to-End Cloud mit zwei Geräten", "Tests", "node", ["e2e/cloud.mjs"], ui]);
  checks.push(["End-to-End Team mit zwei Konten", "Tests", "node", ["e2e/team.mjs"], ui]);
  checks.push(["End-to-End Updates mit manipulierten Paketen", "Tests", "node", ["e2e/update.mjs"], ui]);
  checks.push(["End-to-End Erweiterungen und Fremdkomponenten", "Tests", "node", ["e2e/extensions.mjs"], ui]);
  checks.push(["End-to-End Installer", "Tests", "node", ["e2e/installer.mjs"], ui]);
  checks.push(["Website bauen", "Tests", "node", ["website/build.mjs"], repo]);
  checks.push(["Website im Browser prüfen", "Tests", "node", ["website/check.mjs"], repo]);
}

// Independent check of the manufacturing files, if the parser is available.
const verifyPython = process.env.TRONICCAD_VERIFY_PYTHON;
if (verifyPython) {
  const samples = join(repo, "target", "fab-sample");
  for (const layers of [1, 2, 4, 6, 8]) {
    const directory = join(samples, String(layers));
    checks.push([`Gerber erzeugen, ${layers} Lagen`, "Tests", "cargo", ["run", "-q", "-p", "tronic-fab", "--example", "fab_sample", "--", directory, String(layers)], repo]);
    checks.push([`Gerber mit fremdem Parser prüfen, ${layers} Lagen`, "Tests", verifyPython, ["scripts/verify_gerber.py", directory, String(layers)], repo]);
  }
}

// Independent check of the simulator, if ngspice is available. Development only:
// it is neither shipped nor linked.
const ngspice = process.env.TRONICCAD_NGSPICE;
if (ngspice) {
  const decks = join(repo, "target", "sim-decks");
  checks.push(["Simulationsfälle erzeugen", "Tests", "cargo", ["run", "-q", "-p", "tronic-sim", "--example", "sim_decks", "--", decks], repo]);
  checks.push(["Simulation gegen ngspice prüfen", "Tests", verifyPython ?? "python", ["scripts/verify_sim.py", decks, ngspice], repo]);
}

const results = [];
for (const [name, area, command, args, cwd] of checks) {
  const started = Date.now();
  process.stdout.write(`… ${name}\n`);
  const run = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    shell: process.platform === "win32" && command.endsWith(".cmd"),
    env: { ...process.env, NO_COLOR: "1", CARGO_TERM_COLOR: "never" },
  });
  const output = `${run.stdout ?? ""}${run.stderr ?? ""}`;
  const ok = run.status === 0;
  results.push({ name, area, ok, seconds: (Date.now() - started) / 1000, output });
  console.log(`${ok ? "OK    " : "FEHLER"} ${name} (${((Date.now() - started) / 1000).toFixed(1)} s)`);
  if (!ok) console.log(output.split("\n").slice(-30).join("\n"));
}

function detail(result) {
  const { name, output } = result;
  if (name === "Rust-Tests") {
    let passed = 0;
    let failed = 0;
    for (const match of output.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)) {
      passed += Number(match[1]);
      failed += Number(match[2]);
    }
    return `${passed} bestanden, ${failed} fehlgeschlagen`;
  }
  if (name.startsWith("UI-Tests")) return output.match(/Tests\s+(.*)/)?.[1]?.trim() ?? "";
  if (name.startsWith("End-to-End")) return output.match(/\d+\/\d+ steps passed/)?.[0] ?? "";
  if (name.startsWith("Lizenzliste")) return output.split("\n").slice(0, 2).join("; ");
  if (name.startsWith("npm-Schw")) return output.match(/found \d+ vulnerabilities/)?.[0] ?? "";
  return "";
}

const cargoVersion = spawnSync("rustc", ["--version"], { encoding: "utf8" }).stdout.trim();
const nodeVersion = process.version;
const passed = results.filter((result) => result.ok);
const failed = results.filter((result) => !result.ok);
const inArea = (area) => results.filter((result) => result.area === area);
const knownIssues = join(repo, "reports/KNOWN_ISSUES.md");

let report = `# Qualitätsbericht\n\n<!-- Generated by scripts/quality.mjs. Do not edit by hand. -->\n\n`;
report += `- **Zeitpunkt:** ${new Date().toISOString()}\n- **Plattform:** ${process.platform} ${process.arch}\n- **Werkzeuge:** ${cargoVersion}, Node ${nodeVersion}\n`;
report += `- **Ergebnis:** ${passed.length} von ${results.length} Prüfungen bestanden\n\n`;
report += "Dieser Bericht gilt nur für die genannte Plattform. Andere Plattformen prüft die CI.\n\n";
report += "| Prüfung | Bereich | Ergebnis | Details | Dauer |\n|---|---|---|---|---|\n";
for (const result of results) {
  report += `| ${result.name} | ${result.area} | ${result.ok ? "bestanden" : "**fehlgeschlagen**"} | ${detail(result).replace(/\|/g, "/")} | ${result.seconds.toFixed(1)} s |\n`;
}
report += `\n## Fehlgeschlagen\n\n${failed.length ? failed.map((r) => `- ${r.name}`).join("\n") : "Keine."}\n`;
report += `\n## Sicherheitsprobleme\n\n${inArea("Sicherheit").some((r) => !r.ok) ? "Siehe fehlgeschlagene Sicherheitsprüfungen." : "Keine bekannten Schwachstellen in den geprüften Datenbanken. Das ersetzt keine manuelle Sicherheitsprüfung."}\n`;
report += `\n## Lizenzprobleme\n\n${inArea("Lizenz").some((r) => !r.ok) ? "Siehe fehlgeschlagene Lizenzprüfungen." : "Keine Verstöße gegen die Positivliste. Die rechtliche Freigabe steht aus, siehe IP_COMPLIANCE.md."}\n`;
report += `\n## Nicht ausgeführt\n\n${skipE2e ? "- Anwendungs-Build und End-to-End-Test\n" : ""}${verifyPython ? "" : "- Prüfung der Gerber-Dateien mit fremdem Parser: TRONICCAD_VERIFY_PYTHON ist nicht gesetzt\n"}${ngspice ? "" : "- Vergleich der Simulation mit ngspice: TRONICCAD_NGSPICE ist nicht gesetzt\n"}${process.env.TRONICCAD_RCLONE ? "" : "- S3 und WebDAV gegen echte Server: TRONICCAD_RCLONE ist nicht gesetzt\n"}${process.env.TRONICCAD_TEST_DATABASE_URL ? "" : "- Cloud-Dienst gegen PostgreSQL: TRONICCAD_TEST_DATABASE_URL ist nicht gesetzt\n"}- Speichertests: noch nicht eingerichtet, siehe TESTPLAN.md\n- Prüfung der Fertigungsdaten durch einen Fertiger: steht aus\n- Tests auf macOS und Linux: nur über die CI vorgesehen\n`;
report += `\n## Bekannte Fehler, Performanceprobleme und offene Aufgaben\n\n`;
report += existsSync(knownIssues) ? readFileSync(knownIssues, "utf8") : "Keine Datei reports/KNOWN_ISSUES.md gefunden.\n";

mkdirSync(join(repo, "reports"), { recursive: true });
writeFileSync(join(repo, "reports/QUALITY_REPORT.md"), report);
console.log(`\n${passed.length}/${results.length} bestanden. Bericht: reports/QUALITY_REPORT.md`);
process.exit(failed.length ? 1 : 0);
