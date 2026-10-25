// Renders each scene's overlay (frame.html) as PNG frames with a transparent
// window hole, stepping the CSS animations by hand so every frame is exact.
// Usage: node render.mjs <out folder> [scene id ...]
import { createRequire } from 'node:module';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// require() rather than import, so a global Playwright is found via NODE_PATH.
const { chromium } = createRequire(import.meta.url)('playwright');
const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(process.argv[2] ?? 'out');
const only = process.argv.slice(3);
const { fps, width, height, scenes } = JSON.parse(readFileSync(join(here, 'scenes.json'), 'utf8'));
// The intro animations finish within this; compose.py holds the last frame.
const INTRO_SECONDS = 1.6;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width, height } });
for (const scene of scenes) {
  if (only.length && !only.includes(scene.id)) continue;
  const dir = join(out, 'frames', scene.id);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const url = pathToFileURL(join(here, 'frame.html'));
  url.search = new URLSearchParams({ scene: scene.id, scenes: JSON.stringify(scenes) });
  await page.goto(url.href);
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => Promise.all([...document.images].map(i => i.decode())));
  const count = Math.round(INTRO_SECONDS * fps);
  for (let f = 0; f < count; f++) {
    await page.evaluate(t => document.getAnimations().forEach(a => { a.pause(); a.currentTime = t; }), (f * 1000) / fps);
    await page.screenshot({ path: join(dir, `${String(f).padStart(3, '0')}.png`), omitBackground: true });
  }
  console.log(`${scene.id}: ${count} frames`);
}
await browser.close();
