#!/usr/bin/env node
// Builds the application for release so that the same source gives the same bytes:
// paths and link times stay out of the program. Keys and addresses come from the
// environment, see docs/RELEASE.md. Usage: release_build.mjs [target directory]
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = resolve(process.argv[2] ?? join(repo, "target"));
const home = process.env.USERPROFILE ?? process.env.HOME ?? repo;
const flags = [`--remap-path-prefix=${target}=/target`, `--remap-path-prefix=${repo}=/src`, `--remap-path-prefix=${home}=/home`];
if (process.platform === "win32") flags.push("-C", "link-arg=/Brepro", "-C", "link-arg=/PDBALTPATH:%_PDB%");

execFileSync(process.platform === "win32" ? "npx.cmd" : "npx", ["tauri", "build", ...(process.env.TRONICCAD_RELEASE_BUNDLES ? ["--bundles", process.env.TRONICCAD_RELEASE_BUNDLES] : ["--no-bundle"])], {
  cwd: join(repo, "apps/desktop"),
  stdio: "inherit",
  shell: process.platform === "win32",
  env: { ...process.env, RUSTFLAGS: flags.join(" "), CARGO_TARGET_DIR: target, SOURCE_DATE_EPOCH: "0" },
});
