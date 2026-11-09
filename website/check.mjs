#!/usr/bin/env node
// Checks the built website in a real browser: all pages in all languages, links, images,
// no request to anywhere else, no horizontal overflow on a phone, and the honest status
// that nothing has been released. Skips when no Edge or Chrome is there.
import { createReadStream, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..");
const dist = join(here, "dist");
const { chromium } = createRequire(join(repo, "apps/desktop/package.json"))("playwright-core");
const content = JSON.parse(readFileSync(join(here, "content.json"), "utf8"));
const artifacts = join(repo, "e2e-artifacts");
mkdirSync(artifacts, { recursive: true });
const TYPES = { ".html": "text/html; charset=utf-8", ".css": "text/css", ".png": "image/png" };

const server = createServer((request, response) => {
  const path = normalize(join(dist, decodeURIComponent(new URL(request.url, "http://x").pathname)));
  const file = path.endsWith("\\") || path.endsWith("/") ? join(path, "index.html") : path;
  if (!file.startsWith(dist) || !existsSync(file) || !statSync(file).isFile()) {
    response.writeHead(404).end();
    return;
  }
  response.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(response);
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const base = `http://127.0.0.1:${server.address().port}`;

let browser;
for (const channel of ["msedge", "chrome"]) {
  browser = await chromium.launch({ channel, headless: true }).catch(() => null);
  if (browser) break;
}
if (!browser) {
  console.log("no browser here: the website was not checked");
  server.close();
  process.exit(0);
}

const problems = [];
let checked = 0;
for (const [viewport, width] of [["desktop", 1280], ["phone", 375]]) {
  const context = await browser.newContext({ viewport: { width, height: 800 } });
  const page = await context.newPage();
  page.on("pageerror", (error) => problems.push(`script error: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(`${page.url()}: ${message.text()}`);
  });
  page.on("request", (request) => {
    if (!request.url().startsWith(base)) problems.push(`${page.url()} asks ${request.url()}`);
  });
  page.on("response", (response) => {
    if (response.status() >= 400) problems.push(`${response.url()}: ${response.status()}`);
  });
  for (const [language, text] of Object.entries(content)) {
    for (const name of ["index.html", "handbook.html"]) {
      await page.goto(`${base}/${text.dir}/${name}`);
      await page.waitForLoadState("networkidle");
      checked += 1;
      const label = `${viewport} ${language} ${name}`;
      if ((await page.getAttribute("html", "lang")) !== language) problems.push(`${label}: wrong language`);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      if (overflow > 0) problems.push(`${label}: ${overflow} px too wide`);
      if ((await page.locator('meta[name="robots"][content="noindex"]').count()) !== 1 && process.env.TRONICCAD_SITE_RELEASED !== "yes") {
        problems.push(`${label}: not marked for search engines`);
      }
      for (const href of await page.locator("a[href]").evaluateAll((links) => links.map((link) => link.href))) {
        if (href.startsWith(base)) {
          const answer = await fetch(href);
          if (!answer.ok) problems.push(`${label}: broken link ${href}`);
        } else {
          problems.push(`${label}: link leaves the site: ${href}`);
        }
      }
      if (name === "index.html") {
        if (!(await page.locator("[data-status]").innerText()).includes(text.status_title)) problems.push(`${label}: no status`);
        const images = await page.locator("img").evaluateAll((all) => all.map((image) => {
          image.loading = "eager";
          return image.src;
        }));
        if (images.length !== 3) problems.push(`${label}: ${images.length} images`);
        await page.waitForFunction(() => [...document.images].every((image) => image.complete && image.naturalWidth > 0));
        await page.screenshot({ path: join(artifacts, `site-${viewport}-${language}.png`), fullPage: true });
      }
    }
  }
  await context.close();
}
await browser.close();
server.close();

// What must never reach a public page.
for (const text of Object.values(content)) {
  for (const name of ["index.html", "handbook.html"]) {
    const html = readFileSync(join(dist, text.dir, name), "utf8");
    if (/C:\\Users|127\.0\.0\.1/.test(html)) problems.push(`${text.dir}/${name} names something local`);
    if (/[€$]\s?\d/.test(html)) problems.push(`${text.dir}/${name} names a price`);
  }
}

console.log(`${checked} pages checked, ${problems.length} problems`);
for (const problem of problems.slice(0, 30)) console.error(problem);
process.exit(problems.length === 0 ? 0 : 1);
