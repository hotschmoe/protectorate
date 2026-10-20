import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const [chromium, profile, size, destination, theme, output, hub, credential, connected, interaction] = process.argv.slice(2);
const browser = spawn(chromium, [
  '--headless=new', '--disable-gpu', '--no-sandbox',
  `--user-data-dir=${profile}`, '--remote-debugging-port=0',
  `--window-size=${size}`, 'about:blank',
], { stdio: ['ignore', 'ignore', 'ignore'] });
let socket;
const deadline = setTimeout(() => browser.kill('SIGKILL'), 45_000);
try {
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
      break;
    } catch {
      if (browser.exitCode !== null) throw new Error(`Chromium exited with ${browser.exitCode}`);
      await delay(100);
    }
  }
  if (!port) throw new Error('Chromium debugging endpoint did not start');
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  let nextID = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const response = JSON.parse(event.data);
    const waiter = pending.get(response.id);
    if (!waiter) return;
    pending.delete(response.id);
    if (response.error) waiter.reject(new Error(response.error.message));
    else waiter.resolve(response.result);
  });
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++nextID;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
  await call('Page.enable');
  await call('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: theme }],
  });
  if (connected === 'true') {
    const token = (await readFile(credential, 'utf8')).trim();
    const hubs = JSON.stringify([{ id: 'screenshots', name: 'demo hub', baseUrl: hub, token }]);
    await call('Page.addScriptToEvaluateOnNewDocument', {
      source: `if (location.origin === ${JSON.stringify(new URL(hub).origin)}) localStorage.setItem('flutter.hubs', ${JSON.stringify(JSON.stringify(hubs))});`,
    });
  }
  await call('Page.navigate', { url: destination });
  await delay(12_000);
  const [width, height] = size.split(',').map(Number);
  const tap = async (x, y) => {
    for (const type of ['mousePressed', 'mouseReleased']) {
      await call('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
    }
    await delay(250);
  };
  if (interaction === 'nodes') {
    await tap(width * 5 / 8, height - 36);
    await delay(1000);
  } else if (interaction === 'draft') {
    await tap(70, height - 108);
    await call('Input.insertText', { text: 'Check the node again later.' });
    await tap(width * 5 / 8, height - 36);
    await tap(width / 8, height - 36);
    await tap(70, height - 108);
    const draft = await call('Runtime.evaluate', {
      expression: `(() => { let el = document.activeElement; while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement; return el?.value; })()`,
      returnByValue: true,
    });
    if (draft.result.value !== 'Check the node again later.') {
      throw new Error('Phone draft was lost while switching tabs');
    }
  }
  const screenshot = await call('Page.captureScreenshot', { format: 'png' });
  await writeFile(output, Buffer.from(screenshot.data, 'base64'));
  await call('Browser.close');
} finally {
  clearTimeout(deadline);
  socket?.close();
  if (browser.exitCode === null) browser.kill('SIGTERM');
}
