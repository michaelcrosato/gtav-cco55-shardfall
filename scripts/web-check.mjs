#!/usr/bin/env node
// Drives a single-file web build in headless Chromium (WebGPU on SwiftShader) and checks it:
// screenshots, console log, optional key/tap steps and expected log lines. PLAN.md §7.4.
//
//   node scripts/web-check.mjs <page.html> [--out DIR] [--mobile] [--wait MS]
//        [--step "press:KEY:WAIT_MS"]... [--step "tap:X:Y:WAIT_MS"]... [--step "wait:MS"]...
//        [--expect TEXT]... [--forbid TEXT]... [--query "a=1&b=2"]
//
// Each step runs after the initial wait, in order, with a screenshot after it (step-<n>.png).
// --expect: the console must contain TEXT by the end (exit 1 if not); --forbid: it must not.
// SwiftShader renders roughly 0.5-2 frames per second here, so waits are long and state is read
// from the game's console log rather than from smooth video. Example (attract mode):
//   node scripts/web-check.mjs dist/GrandTheftFreeway-BX.html --wait 15000 \
//     --expect "frontend: attract" --step "press:Space:4000" --expect "frontend: title"
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';

const PLAYWRIGHT = process.env.PLAYWRIGHT_MODULE || '/opt/node22/lib/node_modules/playwright/index.mjs';
const { chromium } = await import(PLAYWRIGHT);

const args = process.argv.slice(2);
if (!args.length || args[0].startsWith('--')) {
  console.error('usage: node scripts/web-check.mjs <page.html> [--out DIR] [--mobile] [--wait MS] [--step ...] [--expect TEXT] [--forbid TEXT] [--query Q]');
  process.exit(2);
}
const page_path = resolve(args[0]);
let out = 'out/web-check', mobile = false, wait = 20000, query = '';
const steps = [], expects = [], forbids = [];
for (let i = 1; i < args.length; i++) {
  const a = args[i], v = () => {
    if (i + 1 >= args.length) { console.error(`${a} needs a value`); process.exit(2); }
    return args[++i];
  };
  if (a === '--out') out = v();
  else if (a === '--mobile') mobile = true;
  else if (a === '--wait') wait = Number(v());
  else if (a === '--step') steps.push(v());
  else if (a === '--expect') expects.push(v());
  else if (a === '--forbid') forbids.push(v());
  else if (a === '--query') query = v();
  else { console.error(`unknown option ${a}`); process.exit(2); }
}
mkdirSync(out, { recursive: true });

const browser = await chromium.launch({
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-vulkan=swiftshader',
         '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
});
const context = mobile
  ? await browser.newContext({ viewport: { width: 844, height: 390 }, isMobile: true, hasTouch: true, deviceScaleFactor: 1 })
  : await browser.newContext({ viewport: { width: 960, height: 540 } });
const page = await context.newPage();
const logs = [];
const stamp = () => (performance.now() / 1000).toFixed(1).padStart(6);
page.on('console', m => logs.push(`${stamp()} [${m.type()}] ${m.text()}`));
page.on('pageerror', e => logs.push(`${stamp()} [pageerror] ${e.message}`));

await page.goto('file://' + page_path + (query ? `?${query}` : ''));
await page.waitForTimeout(wait);
await page.screenshot({ path: join(out, 'start.png') });

let n = 0;
for (const s of steps) {
  n += 1;
  const [kind, ...rest] = s.split(':');
  if (kind === 'press') {
    const [key, ms] = rest;
    await page.keyboard.press(key);
    await page.waitForTimeout(Number(ms || 3000));
  } else if (kind === 'tap') {
    const [x, y, ms] = rest.map(Number);
    if (mobile) await page.touchscreen.tap(x, y); else await page.mouse.click(x, y);
    await page.waitForTimeout(ms || 3000);
  } else if (kind === 'wait') {
    await page.waitForTimeout(Number(rest[0] || 3000));
  } else {
    console.error(`unknown step ${s}`);
    process.exit(2);
  }
  await page.screenshot({ path: join(out, `step-${n}.png`) });
}
await browser.close();

const text = logs.join('\n');
writeFileSync(join(out, 'console.log'), text + '\n');
let ok = true;
for (const e of expects) if (!text.includes(e)) { console.error(`missing expected log: ${e}`); ok = false; }
for (const f of forbids) if (text.includes(f)) { console.error(`forbidden log present: ${f}`); ok = false; }
if (text.includes('[pageerror]')) { console.error('page errors (see console.log)'); ok = false; }
console.log(`${ok ? 'ok' : 'FAILED'}: ${logs.length} console lines, ${n} steps, screenshots and console.log in ${out}`);
process.exit(ok ? 0 : 1);
