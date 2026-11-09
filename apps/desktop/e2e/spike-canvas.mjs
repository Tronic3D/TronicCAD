// Measurement for the rendering decision (risk R3): how fast can the web view
// draw a worst-case schematic with Canvas 2D, and is WebGPU available at all?
// Runs inside the real application window. Results: docs/adr/0001-*.md
import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const binary = process.env.TRONICCAD_BIN ?? join(repo, "target/debug/troniccad.exe");
const child = spawn(binary, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9334" },
  stdio: "ignore",
});
let browser;
for (let i = 0; i < 60 && !browser; i++) {
  try {
    browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
  } catch {
    await new Promise((done) => setTimeout(done, 500));
  }
}
const page = browser.contexts()[0].pages()[0];
await page.waitForSelector(".toolbar");

const result = await page.evaluate(async () => {
  const webgpu = Boolean(navigator.gpu) && Boolean(await navigator.gpu.requestAdapter().catch(() => null));
  const canvas = document.createElement("canvas");
  canvas.width = 1920;
  canvas.height = 1080;
  canvas.style.cssText = "position:fixed;inset:0;width:100vw;height:100vh;z-index:99";
  document.body.append(canvas);
  const ctx = canvas.getContext("2d", { alpha: false });

  // Symbols: 8 strokes and 2 texts each. Wires: 1 stroke each.
  const measure = async (count, scale) => {
    const items = Array.from({ length: count }, (_, i) => ({
      x: (i % 120) * 16,
      y: Math.floor(i / 120) * 12,
    }));
    const times = [];
    for (let frame = 0; frame < 30; frame++) {
      const start = performance.now();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = "#0f1a1f";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.setTransform(scale, 0, 0, scale, -frame, 0);
      ctx.lineWidth = 0.4;
      ctx.strokeStyle = "#35c6a8";
      ctx.beginPath();
      items.forEach((item, i) => {
        if (i % 2) {
          ctx.moveTo(item.x, item.y);
          ctx.lineTo(item.x + 12, item.y);
        } else {
          for (let s = 0; s < 8; s++) {
            ctx.moveTo(item.x + s, item.y + (s % 2) * 3);
            ctx.lineTo(item.x + s + 1, item.y + ((s + 1) % 2) * 3);
          }
        }
      });
      ctx.stroke();
      ctx.fillStyle = "#e6eef0";
      ctx.font = "2px sans-serif";
      items.forEach((item, i) => {
        if (!(i % 2)) {
          ctx.fillText("R1", item.x, item.y - 1);
          ctx.fillText("10k", item.x, item.y + 5);
        }
      });
      await new Promise((done) => requestAnimationFrame(done));
      times.push(performance.now() - start);
    }
    times.sort((a, b) => a - b);
    return { count, medianMs: times[15], p95Ms: times[28] };
  };

  const measurements = [await measure(1000, 1), await measure(10000, 1), await measure(50000, 1)];
  canvas.remove();
  return { webgpu, userAgent: navigator.userAgent, measurements };
});
console.log(JSON.stringify(result, null, 2));
await browser.close();
child.kill();
process.exit(0);
