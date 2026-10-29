import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const [chromium, profile, size, destination, theme, output, hub, credential, connected, interaction] = process.argv.slice(2);
const startupFailureMessage = 'Local storage is unavailable for startup verification.';
const browser = spawn(chromium, [
  '--headless=new', '--disable-gpu', '--no-sandbox',
  `--user-data-dir=${profile}`, '--remote-debugging-port=0',
  `--window-size=${size}`, 'about:blank',
], { stdio: ['ignore', 'ignore', 'ignore'] });
let socket;
let call;
let inspect;
let browserError;
const pending = new Map();
const rejectPending = error => {
  for (const waiter of pending.values()) waiter.reject(error);
  pending.clear();
};
browser.on('error', error => {
  browserError = error;
  rejectPending(error);
});
browser.once('exit', (code, signal) => rejectPending(new Error(`Chromium exited: ${signal || code}`)));
const deadline = setTimeout(() => {
  browserError = new Error('Screenshot browser exceeded its 60 second deadline');
  rejectPending(browserError);
  socket?.close();
  browser.kill('SIGKILL');
}, 60_000);
try {
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
      break;
    } catch {
      if (browserError) throw browserError;
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
    socket.addEventListener('close', () => reject(new Error('Chromium debugging connection closed before opening')), { once: true });
  });
  let nextID = 0;
  socket.addEventListener('close', () => rejectPending(new Error('Chromium debugging connection closed')));
  socket.addEventListener('error', () => rejectPending(new Error('Chromium debugging connection failed')));
  socket.addEventListener('message', event => {
    const response = JSON.parse(event.data);
    const waiter = pending.get(response.id);
    if (!waiter) return;
    pending.delete(response.id);
    if (response.error) waiter.reject(new Error(response.error.message));
    else waiter.resolve(response.result);
  });
  call = (method, params = {}) => new Promise((resolve, reject) => {
    if (socket.readyState !== WebSocket.OPEN) {
      reject(browserError || new Error('Chromium debugging connection is not open'));
      return;
    }
    const id = ++nextID;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
  await call('Page.enable');
  const [width, height] = size.split(',').map(Number);
  await call('Emulation.setDeviceMetricsOverride', {
    width, height, deviceScaleFactor: 1, mobile: false,
  });
  await call('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: theme }],
  });
  if (interaction === 'startup-failure') {
    await call('Page.addScriptToEvaluateOnNewDocument', {
      source: `Object.defineProperty(window, 'localStorage', { configurable: true, get() {
        window.screenshotStorageFailure = true;
        throw new Error(${JSON.stringify(startupFailureMessage)});
      } });`,
    });
  }
  if (connected === 'true') {
    const token = (await readFile(credential, 'utf8')).trim();
    const hubs = JSON.stringify([{ id: 'screenshots', name: 'demo hub', baseUrl: hub, token }]);
    await call('Page.addScriptToEvaluateOnNewDocument', {
      source: `if (location.origin === ${JSON.stringify(new URL(hub).origin)}) localStorage.setItem('flutter.hubs', ${JSON.stringify(JSON.stringify(hubs))});`,
    });
  }
  await call('Page.navigate', { url: destination });
  await delay(12_000);
  const evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    }
    return result.result.value;
  };
  await evaluate(`window.screenshotElements = function collect(root = document) {
    return [...root.querySelectorAll('*')].flatMap(el => [el, ...(el.shadowRoot ? collect(el.shadowRoot) : [])]);
  }; true`);
  inspect = () => evaluate(`(() => {
    const elements = window.screenshotElements();
    const label = el => String(el.getAttribute('aria-label') || el.value || el.textContent || '').replace(/\\s+/g, ' ').trim();
    const bounds = el => {
      const rect = el.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    };
    let active = document.activeElement;
    while (active?.shadowRoot?.activeElement) active = active.shadowRoot.activeElement;
    return {
      viewport: { width: innerWidth, height: innerHeight },
      tabs: elements.filter(el => el.hasAttribute('aria-current') || el.hasAttribute('aria-selected'))
        .map(el => ({ label: label(el), selected: (el.getAttribute('aria-current') || el.getAttribute('aria-selected')) === 'true', ...bounds(el) })),
      inputs: elements.filter(el => el.matches('input, textarea') && !el.readOnly && !el.disabled)
        .map(el => ({ value: el.value, ...bounds(el) })),
      startupFailureTriggered: window.screenshotStorageFailure === true,
      startupErrors: elements.filter(el => /^Could not start Protectorate(?: |$)/.test(label(el)) || label(el).includes(${JSON.stringify(startupFailureMessage)}))
        .map(el => ({ label: label(el), ...bounds(el) })),
      active: { tag: active?.tagName, value: active?.value, readOnly: active?.readOnly },
    };
  })()`);
  const tap = async (x, y) => {
    for (const type of ['mousePressed', 'mouseReleased']) {
      await call('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
    }
    await delay(250);
  };
  const waitFor = async (description, test) => {
    for (let attempt = 0; attempt < 30; attempt++) {
      const state = await inspect();
      if (test(state)) return state;
      await delay(100);
    }
    throw new Error(`Timed out waiting for ${description}`);
  };
  const selectTab = async label => {
    const before = await waitFor(`${label} navigation target`, state => state.tabs.some(tab => tab.label === label));
    const tab = before.tabs.find(tab => tab.label === label);
    await tap(tab.x + tab.width / 2, tab.y + tab.height / 2);
    await waitFor(`${label} tab selection`, state => state.tabs.some(tab => tab.label === label && tab.selected));
  };
  const focusComposer = async () => {
    const state = await waitFor('composer', state => state.inputs.some(input => input.width > 0 && input.height > 0));
    const input = state.inputs.filter(input => input.width > 0 && input.height > 0).sort((a, b) => b.y - a.y)[0];
    await tap(input.x + input.width / 2, input.y + input.height / 2);
  };
  const pairState = () => evaluate(`(() => {
    let hubs = [];
    try { hubs = JSON.parse(JSON.parse(localStorage.getItem('flutter.hubs') || '"[]"')); } catch {}
    return { href: location.href, hubs: hubs.map(h => ({ baseUrl: h.baseUrl, token: h.token })) };
  })()`);
  if (interaction && interaction !== 'pair') {
    await evaluate(`window.screenshotElements().find(el => el.tagName === 'FLT-SEMANTICS-PLACEHOLDER')?.click(); true`);
    if (interaction !== 'startup-failure') {
      await waitFor('phone navigation semantics', state => state.tabs.some(tab => tab.label === 'ENVOY'));
    }
  }
  if (interaction === 'startup-failure') {
    const visible = (item, state) => item.width > 0 && item.height > 0 && item.y >= 0 && item.y + item.height <= state.viewport.height;
    const state = await waitFor('visible production startup error title', state =>
      state.startupFailureTriggered &&
      state.startupErrors.some(item => item.label.startsWith('Could not start Protectorate') && visible(item, state)));
    const accessibility = await call('Accessibility.getFullAXTree');
    const message = accessibility.nodes.find(node => !node.ignored && node.role?.value === 'textbox' && node.backendDOMNodeId);
    if (!message) throw new Error('Startup error message has no accessible text field');
    const { model } = await call('DOM.getBoxModel', { backendNodeId: message.backendDOMNodeId });
    const [left, top, right, , , bottom] = model.content;
    if (left < 0 || right > state.viewport.width || top < 0 || bottom > state.viewport.height || right <= left || bottom <= top) {
      throw new Error('Startup error text field is outside the visible viewport');
    }
    // Flutter exposes SelectableText's value after its first pointer interaction.
    await tap((left + right) / 2, (top + bottom) / 2);
    await waitFor('visible startup error text', state => state.startupErrors.some(item => item.label.includes(startupFailureMessage) && visible(item, state)));
    const reported = await call('Accessibility.getFullAXTree');
    if (!reported.nodes.some(node => !node.ignored && node.role?.value === 'textbox' && node.value?.value?.includes(startupFailureMessage) &&
      node.properties?.some(property => ['disabled', 'readonly'].includes(property.name) && property.value.value === true))) {
      throw new Error('Startup error message is not exposed as noneditable accessible text');
    }
  } else if (interaction === 'pair') {
    // The destination is the hub's web pairing link carrying the device token.
    const token = (await readFile(credential, 'utf8')).trim();
    const base = hub.replace(/\/$/, '');
    for (let attempt = 0; ; attempt++) {
      const state = await pairState();
      const cleared = !state.href.includes(token);
      const saved = state.hubs.some(item => item.baseUrl === base && item.token === token);
      if (cleared && saved) break;
      if (attempt === 100) {
        throw new Error(cleared ? 'Browser pairing did not save the hub with its token' : 'Browser pairing left the token in the address bar');
      }
      await delay(200);
    }
  } else if (interaction === 'draft') {
    const text = 'Check the sleeve again later.';
    await focusComposer();
    await call('Input.insertText', { text });
    await waitFor('draft insertion', state => state.active.value === text);
    await selectTab('QUOTAS');
    await selectTab('ENVOY');
    await focusComposer();
    const after = await inspect();
    if (after.active.value !== text) {
      throw new Error('Phone draft was lost while switching tabs');
    }
  }
  const screenshot = await call('Page.captureScreenshot', { format: 'png' });
  await writeFile(output, Buffer.from(screenshot.data, 'base64'));
  await call('Browser.close').catch(() => {});
} catch (error) {
  if (call && socket?.readyState === WebSocket.OPEN) {
    try {
      const screenshot = await call('Page.captureScreenshot', { format: 'png' });
      const failurePath = output.replace(/\.png$/, '-failure.png');
      await writeFile(failurePath, Buffer.from(screenshot.data, 'base64'));
      const accessibility = interaction === 'startup-failure' ? await call('Accessibility.getFullAXTree') : undefined;
      const pairing = interaction === 'pair' ? await pairState() : undefined;
      await writeFile(`${failurePath}.json`, JSON.stringify({ error: error.message, state: inspect ? await inspect() : null, accessibility, pairing }, null, 2));
      console.error(`Screenshot failure evidence: ${failurePath}`);
    } catch (captureError) {
      console.error(`Could not capture screenshot failure: ${captureError.message}`);
    }
  }
  throw error;
} finally {
  clearTimeout(deadline);
  socket?.close();
  if (browser.exitCode === null) browser.kill('SIGTERM');
}
