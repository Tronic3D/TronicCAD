#!/usr/bin/env node
// Builds the download package of a beta. Publishes nothing.
//
//   node scripts/build_beta.mjs --label "Beta 1" [--support <contact>]
//
// The result lands in dist-beta/<label>: the installer or package, checksums, the guide
// for testers and the bill of materials. It refuses a source tree that git does not
// account for.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function option(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

function stop(message) {
  console.error(message);
  process.exit(1);
}

const label = option("label");
const support = option("support");
if (!label || !/^[A-Za-z0-9 .-]{1,30}$/.test(label)) stop('--label is needed, for example "Beta 1"');

const git = (...args) => execFileSync("git", args, { cwd: repo, encoding: "utf8" }).trim();
if (git("status", "--porcelain") !== "" && !process.argv.includes("--allow-changes")) {
  stop("The working tree has changes. A beta must be traceable to a commit.");
}
const commit = git("rev-parse", "HEAD");
if (!support) console.warn("note: no --support given. The program will say that there is no contact.");

const windows = process.platform === "win32";
const bundle = windows ? "nsis" : "deb";
execFileSync(process.execPath, [join(repo, "scripts/release_build.mjs")], {
  stdio: "inherit",
  env: {
    ...process.env,
    TRONICCAD_RELEASE_BUNDLES: bundle,
    TRONICCAD_BETA_LABEL: label,
    ...(support ? { TRONICCAD_SUPPORT_CONTACT: support } : {}),
  },
});

const release = join(repo, "target/release");
execFileSync(process.execPath, [join(repo, "scripts/check_release_binary.mjs"), join(release, windows ? "troniccad.exe" : "troniccad")], {
  stdio: "inherit",
});

const slug = label.toLowerCase().replace(/[^a-z0-9.]+/g, "-");
const out = join(repo, "dist-beta", slug);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const built = join(release, "bundle", bundle);
const source = readdirSync(built).find((name) => name.endsWith(windows ? "-setup.exe" : ".deb"));
if (!source) stop(`no package in ${built}`);
const name = `TronicCAD-${slug}-${windows ? "windows-x64-setup.exe" : "linux-amd64.deb"}`;
copyFileSync(join(built, source), join(out, name));

const guide = join(repo, "docs/BETA_TESTERS.md");
if (existsSync(guide)) copyFileSync(guide, join(out, "ANLEITUNG-README.md"));
copyFileSync(join(repo, "sbom.cdx.json"), join(out, "sbom.cdx.json"));

const hash = createHash("sha256").update(readFileSync(join(out, name))).digest("hex");
writeFileSync(join(out, `SHA256SUMS-${windows ? "windows" : "linux"}.txt`), `${hash}  ${name}\n`);
writeFileSync(join(out, `build-${windows ? "windows" : "linux"}.json`), `${JSON.stringify({ label, commit, file: name, sha256: hash }, null, 2)}\n`);
console.log(`\n${out}\n${name}\nsha256 ${hash}\ncommit ${commit}\nunsigned: nothing was published`);
