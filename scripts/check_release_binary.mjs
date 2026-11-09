#!/usr/bin/env node
// Checks a release build before it is signed: no development switch may be in it, and
// the keys and addresses it needs must be. Usage: check_release_binary.mjs <program>
import { readFileSync } from "node:fs";

const path = process.argv[2];
if (!path) {
  console.error("usage: check_release_binary.mjs <program>");
  process.exit(2);
}
const content = readFileSync(path);
const has = (text) => content.includes(Buffer.from(text));

const SWITCHES = [
  "TRONICCAD_TEST_APP_VERSION",
  "TRONICCAD_TEST_UPDATE_URL",
  "TRONICCAD_TEST_UPDATE_KEYS",
];
const problems = SWITCHES.filter(has).map((name) => `the development switch ${name} is in the program`);

// Without these strings, the search above proves nothing about this file.
for (const marker of ["update.badSignature"]) {
  if (!has(marker)) problems.push(`${marker} is missing: is this the TronicCAD program?`);
}
for (const [name, value] of Object.entries({
  TRONICCAD_BETA_LABEL: process.env.TRONICCAD_BETA_LABEL,
  TRONICCAD_UPDATE_URL: process.env.TRONICCAD_UPDATE_URL,
  TRONICCAD_UPDATE_PUBLIC_KEYS: process.env.TRONICCAD_UPDATE_PUBLIC_KEYS,
})) {
  if (!value) console.log(`note: ${name} is not set here, so its presence was not checked`);
  else if (!has(value)) problems.push(`${name} is not in the program`);
}

console.log(`${SWITCHES.length} switches searched, ${problems.length} problems`);
for (const problem of problems) console.error(problem);
process.exit(problems.length === 0 ? 0 : 1);
