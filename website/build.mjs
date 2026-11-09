#!/usr/bin/env node
// Builds the static website into website/dist, in four languages. The handbook comes from
// the same file the application uses, so they cannot drift apart.
// Nothing is loaded from elsewhere: no fonts, no scripts, no tracking.
// While TRONICCAD_SITE_RELEASED is not set, the pages ask search engines to stay away.
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..");
const out = join(here, "dist");
const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const content = read(join(here, "content.json"));
const handbook = read(join(repo, "apps/desktop/src/help/handbook.json"));
const released = process.env.TRONICCAD_SITE_RELEASED === "yes";
const SHOTS = ["board.png", "schematic.png", "view3d.png"];
const languages = Object.keys(content);

const escape = (text) =>
  String(text).replace(/[&<>"']/g, (mark) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[mark]);

function page(language, text, title, active, body) {
  const path = active === "handbook" ? "handbook.html" : "index.html";
  const others = languages
    .map((code) => {
      const other = content[code];
      const current = code === language ? ' aria-current="true"' : "";
      return `<a href="../${other.dir}/${path}" hreflang="${code}" lang="${code}"${current}>${escape(other.name)}</a>`;
    })
    .join("");
  return `<!DOCTYPE html>
<html lang="${language}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self'; style-src 'self'; base-uri 'none'; form-action 'none'">
${released ? "" : '<meta name="robots" content="noindex">\n'}<title>${escape(title)}</title>
<meta name="description" content="${escape(text.description)}">
<link rel="stylesheet" href="../site.css">
<link rel="icon" href="../assets/icon.png">
</head>
<body>
<header class="top"><div class="wrap">
<a class="brand" href="index.html">TronicCAD</a>
<nav class="links" aria-label="${escape(text.home)}">
<a href="index.html"${active === "home" ? ' aria-current="page"' : ""}>${escape(text.home)}</a>
<a href="handbook.html"${active === "handbook" ? ' aria-current="page"' : ""}>${escape(text.handbook)}</a>
</nav>
<nav class="links" aria-label="Language">${others}</nav>
</div></header>
<main>${body}</main>
<footer><div class="wrap">
<p>${escape(text.legal)}</p>
<p>TronicCAD · ${escape(text.third_party)}: THIRD_PARTY_NOTICES.md</p>
</div></footer>
</body>
</html>
`;
}

function home(language, text) {
  const features = text.features
    .map(([name, about]) => `<div class="card"><h3>${escape(name)}</h3><p>${escape(about)}</p></div>`)
    .join("");
  const shots = SHOTS.map(
    (file, index) =>
      `<figure><img src="../assets/${file}" width="1280" height="775" alt="${escape(text.shots[index])}" loading="lazy"><figcaption>${escape(text.shots[index])}</figcaption></figure>`,
  ).join("");
  const body = `
<div class="wrap">
<div class="hero">
<h1>${escape(text.lead)}</h1>
<p class="sub">${escape(text.sub)}</p>
<p class="status" data-status><strong>${escape(text.status_title)}</strong>${escape(text.status)}</p>
</div>
<section><div class="shots">${shots}</div></section>
<section><h2>${escape(text.features_title)}</h2><div class="grid">${features}</div></section>
<section><h2>${escape(text.free_title)}</h2><p>${escape(text.free_text)}</p></section>
<section><h2>${escape(text.limits_title)}</h2><ul>${text.limits.map((item) => `<li>${escape(item)}</li>`).join("")}</ul></section>
</div>`;
  return page(language, text, text.title, "home", body);
}

function manual(language, text) {
  const bold = (paragraph) =>
    paragraph
      .split("**")
      .map((part, index) => (index % 2 === 1 ? `<strong>${escape(part)}</strong>` : escape(part)))
      .join("");
  const sections = handbook[language]
    .map((section) => `<h2 id="${section.id}">${escape(section.title)}</h2>${section.text.map((paragraph) => `<p>${bold(paragraph)}</p>`).join("")}`)
    .join("");
  return page(language, text, `${text.handbook}: TronicCAD`, "handbook", `<div class="wrap manual"><h1>${escape(text.handbook)}</h1>${sections}</div>`);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "assets"), { recursive: true });
copyFileSync(join(here, "site.css"), join(out, "site.css"));
for (const file of SHOTS) copyFileSync(join(here, "assets", file), join(out, "assets", file));
copyFileSync(join(repo, "apps/desktop/src-tauri/icons/32x32.png"), join(out, "assets", "icon.png"));
for (const [language, text] of Object.entries(content)) {
  const directory = join(out, text.dir);
  mkdirSync(directory);
  writeFileSync(join(directory, "index.html"), home(language, text));
  writeFileSync(join(directory, "handbook.html"), manual(language, text));
}
writeFileSync(
  join(out, "index.html"),
  `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=en/index.html"><title>TronicCAD</title>${released ? "" : '<meta name="robots" content="noindex">'}</head><body><a href="en/index.html">TronicCAD</a></body></html>\n`,
);
console.log(`${languages.length} languages built${released ? "" : ", marked as not for search engines"}`);
