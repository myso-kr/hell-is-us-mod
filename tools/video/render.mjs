// Renders scene.html to the site's media (docs/assets/media), one frame at a time:
// seek(t), screenshot, pipe to ffmpeg — no frames on disk. Deterministic, so a
// re-render of the same scene is the same video.
//
//   cd tools/video && npm install && npm run render            (all outputs)
//   node render.mjs --only poster                              (just the poster)
//   node render.mjs --stills 1,5,10                            (keyframes to check)
//
// Needs Microsoft Edge (or Chrome via CHROME=path) and ffmpeg on PATH.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import puppeteer from 'puppeteer-core';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const out = path.join(root, 'docs', 'assets', 'media');   // served by the site as is (build.py leaves it)
const arg = (k, d) => { const i = process.argv.indexOf(`--${k}`); return i > 0 ? process.argv[i + 1] : d; };
const FPS = Number(arg('fps', 30));
const ONLY = arg('only', 'all');
const BROWSER = process.env.CHROME ||
  ['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Google/Chrome/Application/chrome.exe']
    .find((p) => fs.existsSync(p));

// the repository, served as is: the scene reads /assets and /tools/video/node_modules
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = http.createServer((req, res) => {
  const file = path.join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { 'content-type': TYPES[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const url = `http://127.0.0.1:${server.address().port}/tools/video/scene.html`;

const browser = await puppeteer.launch({
  executablePath: BROWSER, headless: true,
  args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--hide-scrollbars', '--force-device-scale-factor=1'],
});
const page = await browser.newPage();
await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
page.on('pageerror', (e) => console.error('page:', e.message));
await page.goto(url, { waitUntil: 'networkidle0' });
await page.waitForFunction('window.READY === true', { timeout: 60000 });
const duration = await page.evaluate('window.DURATION');
fs.mkdirSync(out, { recursive: true });

const frame = async (t) => { await page.evaluate((t) => window.seek(t), t); return page.screenshot({ type: 'png' }); };

// --stills 1,5,10: check keyframes before a full render (stills/, not committed)
const STILLS = arg('stills', '');
if (STILLS) {
  fs.mkdirSync(path.join(here, 'stills'), { recursive: true });
  for (const t of STILLS.split(',').map(Number)) {
    fs.writeFileSync(path.join(here, 'stills', `${String(t).padStart(5, '0')}.png`), await frame(t));
    console.log('still', t);
  }
  await browser.close(); server.close(); process.exit(0);
}
if (ONLY === 'all' || ONLY === 'poster') {
  fs.writeFileSync(path.join(out, 'poster.png'), await frame(5.2));
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', path.join(out, 'poster.png'), '-q:v', '3', path.join(out, 'poster.jpg')]);
  fs.unlinkSync(path.join(out, 'poster.png'));
  console.log('poster.jpg');
}
if (ONLY === 'all' || ONLY === 'video') {
  const master = path.join(here, 'master.mp4');   // high quality, not committed
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(FPS), '-i', '-',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '14', '-pix_fmt', 'yuv420p', master], { stdio: ['pipe', 'inherit', 'inherit'] });
  const total = Math.round(duration * FPS);
  const started = Date.now();
  for (let i = 0; i < total; i++) {
    const png = await frame(i / FPS);
    if (!ff.stdin.write(png)) await new Promise((r) => ff.stdin.once('drain', r));
    if (i % FPS === 0) process.stdout.write(`\r${i}/${total} frames, ${((Date.now() - started) / 1000).toFixed(0)} s`);
  }
  ff.stdin.end();
  await new Promise((r) => ff.on('close', r));
  console.log(`\nmaster.mp4 (${total} frames)`);
  // what the site and README use
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', master, '-c:v', 'libx264', '-preset', 'slow', '-crf', '24',
    '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', path.join(out, 'hiumod-intro.mp4')]);
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', master, '-c:v', 'libvpx-vp9', '-b:v', '0', '-crf', '38',
    '-row-mt', '1', '-an', path.join(out, 'hiumod-intro.webm')]);
  for (const f of ['hiumod-intro.mp4', 'hiumod-intro.webm']) {
    console.log(f, (fs.statSync(path.join(out, f)).size / 1048576).toFixed(1), 'MB');
  }
}
await browser.close();
server.close();

function run(cmd, args) {
  return new Promise((resolve, reject) => spawn(cmd, args, { stdio: 'inherit' })
    .on('close', (c) => (c === 0 ? resolve() : reject(new Error(`${cmd} exited ${c}`)))));
}
