import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const [chromium, profile, size, destination, theme, output, hub, credential, connected, interaction] = process.argv.slice(2);
const semanticApproval = interaction === 'pair-approve' && process.env.PROTECTORATE_BROWSER_SEMANTICS === '1';
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
  const tapAccessible = async (label, role = 'button') => {
    for (let attempt = 0; attempt < 50; attempt++) {
      const tree = await call('Accessibility.getFullAXTree');
      const button = tree.nodes.find(node =>
        !node.ignored && node.role?.value === role &&
        node.name?.value?.includes(label) && node.backendDOMNodeId);
      if (button) {
        const { model } = await call('DOM.getBoxModel', { backendNodeId: button.backendDOMNodeId });
        const [left, top, right, , , bottom] = model.content;
        await tap((left + right) / 2, (top + bottom) / 2);
        return;
      }
      const box = await evaluate(`(() => {
        const candidates = window.screenshotElements().filter(el => {
          const name = String(el.getAttribute('aria-label') || el.textContent || '').trim();
          const rect = el.getBoundingClientRect();
          return name === ${JSON.stringify(label)} && rect.width > 0 && rect.height > 0;
        });
        candidates.sort((a, b) => {
          const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
          return ra.width * ra.height - rb.width * rb.height;
        });
        const rect = candidates[0]?.getBoundingClientRect();
        return rect ? { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 } : null;
      })()`);
      if (box) {
        await tap(box.x, box.y);
        return;
      }
      await delay(200);
    }
    throw new Error(`Timed out waiting for ${label} ${role}`);
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
  if (interaction && (interaction !== 'pair-approve' || semanticApproval)) {
    await evaluate(`window.screenshotElements().find(el => el.tagName === 'FLT-SEMANTICS-PLACEHOLDER')?.click(); true`);
    if (!['startup-failure', 'pair-approve'].includes(interaction)) {
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
  } else if (interaction === 'pair-approve') {
    if (semanticApproval) {
      await tapAccessible('demo hub', 'group');
      await tapAccessible('APPROVE');
    } else {
      await tap(500, 135);
      await delay(1000);
      await tap(240, 124);
      await evaluate(`window.screenshotElements().find(el => el.tagName === 'FLT-SEMANTICS-PLACEHOLDER')?.click(); true`);
    }
    await tapAccessible('CODE MATCHES · APPROVE');
    const token = (await readFile(credential, 'utf8')).trim();
    for (let attempt = 0; attempt < 50; attempt++) {
      const response = await fetch(`${hub.replace(/\/$/, '')}/protectorate.v1.DeviceService/ListDeviceAuth`, {
        method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: '{}', signal: AbortSignal.timeout(8000),
      });
      if (!response.ok) throw new Error(`Checking browser approval failed: HTTP ${response.status}`);
      const pending = (await response.json()).requests || [];
      if (pending.length === 0) break;
      if (attempt === 49) throw new Error('Browser approval did not clear the pending request');
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
      const accessibility = ['startup-failure', 'pair-approve'].includes(interaction) ? await call('Accessibility.getFullAXTree') : undefined;
      const approveElements = interaction === 'pair-approve' ? await call('Runtime.evaluate', {
        expression: `window.screenshotElements().map(el => {
          const rect = el.getBoundingClientRect();
          return { tag: el.tagName, role: el.getAttribute('role'), aria: el.getAttribute('aria-label'), text: String(el.textContent || '').slice(0, 120), x: rect.x, y: rect.y, width: rect.width, height: rect.height };
        }).filter(el => /APPROVE|browser-approval/.test(String(el.aria || '') + el.text)).slice(0, 30)`,
        returnByValue: true,
      }) : undefined;
      await writeFile(`${failurePath}.json`, JSON.stringify({ error: error.message, state: inspect ? await inspect() : null, accessibility, approveElements: approveElements?.result?.value }, null, 2));
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
