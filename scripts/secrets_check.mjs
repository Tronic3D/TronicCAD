#!/usr/bin/env node
// Looks through everything git would commit for what must never be there: private keys
// and credentials. A guard against mistakes, not proof that nothing is there.
import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const listed = execFileSync("git", ["ls-files", "--cached", "--others", "--exclude-standard", "-z"], {
  cwd: repo,
  maxBuffer: 64 * 1024 * 1024,
}).toString();

const FORBIDDEN_NAMES = [/\.key$/i, /\.pem$/i, /\.pfx$/i, /\.p12$/i, /(^|\/)\.env$/, /(^|\/)id_(rsa|ed25519)$/, /\.pw$/i];
const CONTENT = [
  ["private key block", /-----BEGIN [A-Z ]*PRIVATE KEY-----/],
  ["Paddle live key", /\bpdl_live_[A-Za-z0-9_]{10,}/],
  ["Anthropic key", /\bsk-ant-[A-Za-z0-9_-]{20,}/],
  ["OpenAI key", /\bsk-[A-Za-z0-9]{32,}/],
  ["AWS access key", /\bAKIA[0-9A-Z]{16}\b/],
  ["GitHub token", /\bgh[pousr]_[A-Za-z0-9]{36,}/],
  ["password in a database address", /postgres(ql)?:\/\/[^:\s/$"'`<{]+:[^@\s$"'`<{]{6,}@(?!db:)/],
];
const MAX_BYTES = 8 * 1024 * 1024;
// This file names the patterns it looks for.
const OWN = "scripts/secrets_check.mjs";

const problems = [];
const names = listed.split("\0").filter(Boolean);
for (const name of names) {
  for (const pattern of FORBIDDEN_NAMES) {
    if (pattern.test(name)) problems.push(`${name}: this kind of file must not be committed`);
  }
  if (name === OWN) continue;
  const path = join(repo, name);
  let text;
  try {
    if (statSync(path).size > MAX_BYTES) continue;
    text = readFileSync(path, "latin1");
  } catch {
    continue;
  }
  for (const [what, pattern] of CONTENT) {
    if (pattern.test(text)) problems.push(`${name}: looks like a ${what}`);
  }
}

console.log(`${names.length} files searched, ${problems.length} problems`);
for (const problem of problems) console.error(problem);
process.exit(problems.length === 0 ? 0 : 1);
