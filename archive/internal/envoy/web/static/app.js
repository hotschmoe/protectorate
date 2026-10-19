let currentConnection = null;
let currentCloneJobId = null;
let lastFetchAllTime = 0;
const FETCH_ALL_THROTTLE_MS = 60000;

// Notification system
const notify = (function() {
    let counter = 0;
    const dismissed = new Set();

    const ICONS = {
        error: '[x]',
        warning: '[!]',
        success: '[*]',
        info: '[i]'
    };

    const AUTO_DISMISS = {
        error: 0,
        warning: 10000,
        info: 5000,
        success: 3000
    };

    function show(type, title, options = {}) {
        const id = ++counter;
        const container = document.getElementById('notify-container');
        if (!container) return id;

        const box = document.createElement('div');
        box.className = `notify-box ${type}`;
        box.dataset.notifyId = id;

        let html = `
            <div class="notify-header">
                <span class="notify-icon">${ICONS[type]}</span>
                <div class="notify-content">
                    <div class="notify-title">${escapeHtml(title)}</div>
                    ${options.detail ? `<div class="notify-detail">${escapeHtml(options.detail)}</div>` : ''}
                </div>
                <button class="notify-dismiss" onclick="notify.dismiss(${id})">[x]</button>
            </div>
        `;

        if (options.action) {
            html += `
                <div class="notify-action">
                    <button class="notify-action-btn" data-notify-action="${id}">${escapeHtml(options.action.text)}</button>
                </div>
            `;
        }

        box.innerHTML = html;

        if (options.action && options.action.onclick) {
            const actionBtn = box.querySelector('[data-notify-action]');
            if (actionBtn) {
                actionBtn.addEventListener('click', () => {
                    options.action.onclick();
                    dismiss(id);
                });
            }
        }

        container.appendChild(box);

        const autoDismissTime = options.autoDismiss !== undefined ? options.autoDismiss : AUTO_DISMISS[type];
        if (autoDismissTime > 0) {
            setTimeout(() => dismiss(id), autoDismissTime);
        }

        return id;
    }

    function dismiss(id) {
        const container = document.getElementById('notify-container');
        if (!container) return;

        const box = container.querySelector(`[data-notify-id="${id}"]`);
        if (box) {
            box.classList.add('removing');
            setTimeout(() => box.remove(), 200);
        }
    }

    function clear() {
        const container = document.getElementById('notify-container');
        if (container) {
            container.innerHTML = '';
        }
    }

    function isDismissed(key) {
        return dismissed.has(key);
    }

    function markDismissed(key) {
        dismissed.add(key);
    }

    return {
        error: (title, options) => show('error', title, options),
        warn: (title, options) => show('warning', title, options),
        info: (title, options) => show('info', title, options),
        success: (title, options) => show('success', title, options),
        dismiss,
        clear,
        isDismissed,
        markDismissed
    };
})();

const MSG_DATA = 0x30;
const MSG_RESIZE = 0x31;

function buildTerminalMessage(type, data) {
    const msg = new Uint8Array(data.length + 1);
    msg[0] = type;
    for (let i = 0; i < data.length; i++) {
        msg[i + 1] = data.charCodeAt(i);
    }
    return msg.buffer;
}

function switchTab(tabName) {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));

    const tabs = ['sleeves', 'needlecast', 'logs', 'config', 'workspaces', 'doctor'];
    const tabIndex = tabs.indexOf(tabName);
    if (tabIndex >= 0) {
        document.querySelectorAll('.tab')[tabIndex].classList.add('active');
        document.getElementById(`tab-${tabName}`).classList.add('active');
    }

    if (tabName === 'workspaces') {
        refreshWorkspacesTable();
        triggerFetchAllThrottled();
    } else if (tabName === 'doctor') {
        refreshDoctor();
    } else if (tabName === 'needlecast') {
        updateNeedlecastSleeveList();
    } else if (tabName === 'config') {
        refreshConfig();
        refreshAuthStatus();
    }
}

async function refreshDoctor() {
    const container = document.getElementById('doctor-checks');
    container.innerHTML = '<div class="empty-state"><span class="inline-spinner"></span> Running checks...</div>';

    try {
        const resp = await fetch('/api/doctor');
        const checks = await resp.json();

        if (checks.length === 0) {
            container.innerHTML = '<div class="empty-state">No checks available</div>';
            return;
        }

        container.innerHTML = checks.map(check => {
            const icon = check.status === 'pass' ? '[*]' : check.status === 'warning' ? '[!]' : '[x]';
            const suggestion = check.suggestion
                ? `<div class="doctor-check-suggestion">${escapeHtml(check.suggestion)}</div>`
                : '';
            return `
                <div class="doctor-check">
                    <div class="doctor-check-icon ${check.status}">${icon}</div>
                    <div class="doctor-check-content">
                        <div class="doctor-check-header">
                            <span class="doctor-check-name">${escapeHtml(check.name)}</span>
                            <span class="doctor-check-status ${check.status}">${check.status}</span>
                        </div>
                        <div class="doctor-check-message">${escapeHtml(check.message)}</div>
                        ${suggestion}
                    </div>
                </div>
            `;
        }).join('');
    } catch (e) {
        console.error('Failed to fetch doctor checks:', e);
        container.innerHTML = '<div class="empty-state">Failed to load checks</div>';
    }
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

async function triggerFetchAllThrottled() {
    const now = Date.now();
    if (now - lastFetchAllTime < FETCH_ALL_THROTTLE_MS) {
        return;
    }
    lastFetchAllTime = now;

    const indicator = document.getElementById('fetch-indicator');
    indicator.style.display = 'inline';
    const startTime = Date.now();
    const minDisplayMs = 1500;

    try {
        await fetch('/api/workspaces/branches?action=fetch-all', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' }
        });
        refreshWorkspacesTable();
    } catch (e) {
        console.error('Fetch all failed:', e);
    } finally {
        const elapsed = Date.now() - startTime;
        const remaining = minDisplayMs - elapsed;
        if (remaining > 0) {
            setTimeout(() => { indicator.style.display = 'none'; }, remaining);
        } else {
            indicator.style.display = 'none';
        }
    }
}

class TerminalConnection {
    constructor(container, wsPath, title, readOnly = false) {
        this.wsPath = wsPath;
        this.title = title;
        this.container = container;
        this.readOnly = readOnly;
        this.term = null;
        this.ws = null;
        this.fitAddon = null;
        this.webLinksAddon = null;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 10;
        this.baseDelay = 1000;
        this.maxDelay = 30000;
        this.shouldReconnect = true;
        this.resizeTimeout = null;
        this.resizeHandler = null;
    }

    initTerminal() {
        this.container.innerHTML = '';
        this.term = new Terminal({
            cursorBlink: true,
            scrollback: 10000,
            altScrollPassthrough: true,
            fontFamily: '"JetBrains Mono", "Fira Code", "SF Mono", Menlo, Monaco, "Courier New", monospace',
            fontSize: 14,
            fontWeight: 400,
            fontWeightBold: 600,
            letterSpacing: 0,
            lineHeight: 1.2,
            theme: {
                background: '#0a0a0f',
                foreground: '#e0e0e0',
                cursor: '#00f5ff',
                cursorAccent: '#0a0a0f',
                selectionBackground: '#004d4f',
                selectionForeground: '#ffffff',
                selectionInactiveBackground: '#004d4f50',
                black: '#505060',
                red: '#ff0055',
                green: '#00f5ff',
                yellow: '#ffaa00',
                blue: '#00a5aa',
                magenta: '#ff0055',
                cyan: '#00f5ff',
                white: '#e0e0e0',
                brightBlack: '#808090',
                brightRed: '#ff3377',
                brightGreen: '#33ffff',
                brightYellow: '#ffcc33',
                brightBlue: '#33cccc',
                brightMagenta: '#ff3377',
                brightCyan: '#33ffff',
                brightWhite: '#ffffff'
            }
        });
        this.fitAddon = new FitAddon.FitAddon();
        this.term.loadAddon(this.fitAddon);
        this.webLinksAddon = new WebLinksAddon.WebLinksAddon();
        this.term.loadAddon(this.webLinksAddon);
        this.term.open(this.container);
        this.fitAddon.fit();

        this.term.element.addEventListener('wheel', (e) => {
            const buffer = this.term.buffer.active;
            if (buffer.type === 'alternate') {
                return;
            }
            const atTop = buffer.viewportY === 0;
            const atBottom = buffer.viewportY >= buffer.baseY;
            const scrollingUp = e.deltaY < 0;
            const scrollingDown = e.deltaY > 0;

            if ((scrollingDown && atBottom) || (scrollingUp && atTop)) {
                e.stopPropagation();
                e.preventDefault();
            }
        }, { capture: true, passive: false });

        if (!this.readOnly) {
            this.term.onData((data) => {
                if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                    this.ws.send(buildTerminalMessage(MSG_DATA, data));
                }
            });
        }

        this.resizeHandler = () => {
            this.fitAddon.fit();
            this.sendResize();
        };
        window.addEventListener('resize', this.resizeHandler);
    }

    connect() {
        if (!this.term) {
            this.initTerminal();
        }

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        this.ws = new WebSocket(`${protocol}//${window.location.host}${this.wsPath}`);
        this.ws.binaryType = 'arraybuffer';

        this.updateStatus('connecting');

        this.ws.onopen = () => {
            this.reconnectAttempts = 0;
            this.updateStatus('connected');
            const initData = JSON.stringify({
                cols: this.term.cols,
                rows: this.term.rows
            });
            this.ws.send(initData);
        };

        this.ws.onmessage = (event) => {
            const data = new Uint8Array(event.data);
            if (data.length > 0 && data[0] === MSG_DATA) {
                this.term.write(data.slice(1));
            }
        };

        this.ws.onerror = () => {
            this.updateStatus('error');
        };

        this.ws.onclose = (event) => {
            this.updateStatus('disconnected');
            if (this.shouldReconnect && !event.wasClean) {
                this.scheduleReconnect();
            } else if (this.shouldReconnect) {
                this.term.write('\r\n[Connection closed]\r\n');
            }
        };
    }

    scheduleReconnect() {
        if (this.reconnectAttempts >= this.maxReconnectAttempts) {
            this.term.write('\r\n[Max reconnection attempts reached]\r\n');
            this.updateReconnectInfo('Max attempts reached');
            return;
        }

        const delay = Math.min(
            this.baseDelay * Math.pow(2, this.reconnectAttempts),
            this.maxDelay
        );
        this.reconnectAttempts++;

        this.updateReconnectInfo(`Reconnecting in ${delay/1000}s (${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

        setTimeout(() => {
            if (this.shouldReconnect) {
                this.updateReconnectInfo('');
                this.connect();
            }
        }, delay);
    }

    updateStatus(status) {
        const statusEl = document.getElementById('terminal-status');
        statusEl.className = 'terminal-status ' + status;
    }

    updateReconnectInfo(text) {
        const infoEl = document.getElementById('reconnect-info');
        infoEl.textContent = text;
    }

    sendResize() {
        clearTimeout(this.resizeTimeout);
        this.resizeTimeout = setTimeout(() => {
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                const json = JSON.stringify({columns: this.term.cols, rows: this.term.rows});
                this.ws.send(buildTerminalMessage(MSG_RESIZE, json));
            }
        }, 100);
    }

    dispose() {
        this.shouldReconnect = false;
        clearTimeout(this.resizeTimeout);
        if (this.resizeHandler) {
            window.removeEventListener('resize', this.resizeHandler);
            this.resizeHandler = null;
        }
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        if (this.term) {
            this.term.dispose();
            this.term = null;
        }
    }
}

let sleevesCache = [];
let pendingSpawns = [];
let sseConnected = false;

function getWorkspaceName(workspacePath) {
    if (!workspacePath) return 'unknown';
    const parts = workspacePath.split('/');
    return parts[parts.length - 1] || workspacePath;
}

function renderSpawningCard(pending) {
    const wsName = getWorkspaceName(pending.workspace);
    const displayName = pending.name || 'allocating...';
    return `
    <div class="sleeve-card spawning">
        <div class="sleeve-header">
            <span class="sleeve-name">SLEEVE: ${escapeHtml(displayName)}</span>
            <span class="sleeve-status spawning">SPAWNING</span>
        </div>
        <div class="sleeve-body">
            <div class="sleeve-row">
                <span class="sleeve-label">DHF</span>
                <span class="sleeve-value">Claude Code</span>
            </div>
            <div class="sleeve-row">
                <span class="sleeve-label">Workspace</span>
                <span class="sleeve-value">${escapeHtml(wsName)}</span>
            </div>
            <div class="spawning-indicator">
                <span class="inline-spinner"></span>
                <span class="spawning-text">${escapeHtml(pending.message || 'Spawning sleeve...')}</span>
            </div>
        </div>
    </div>
    `;
}

async function refreshSleeves() {
    // Sleeves are now updated via SSE, this function only updates local cache
    try {
        const resp = await fetch('/api/sleeves');
        const sleeves = await resp.json();
        sleevesCache = sleeves;
        updateNeedlecastSleeveList();
    } catch (e) {
        console.error('Failed to fetch sleeves:', e);
    }
}

function updateSleeveCount() {
    const grid = document.getElementById('sleeves-grid');
    const cards = grid.querySelectorAll('.sleeve-card:not(.spawning)');
    const statActive = document.getElementById('stat-active');
    statActive.textContent = cards.length;
}

function renderPendingSpawns() {
    const grid = document.getElementById('sleeves-grid');
    // Remove existing spawning cards
    grid.querySelectorAll('.sleeve-card.spawning').forEach(el => el.remove());

    // Add pending spawn cards at the beginning
    if (pendingSpawns.length > 0) {
        const pendingHtml = pendingSpawns.map(p => renderSpawningCard(p)).join('');
        grid.insertAdjacentHTML('afterbegin', pendingHtml);
    }
}

function updateNeedlecastSleeveList() {
    const list = document.getElementById('needlecast-sleeve-list');
    if (!list) return;
    list.innerHTML = sleevesCache.map(s =>
        `<div class="needlecast-sleeve-item">${escapeHtml(s.name)}</div>`
    ).join('');
}

let workspacesCache = [];

async function refreshWorkspaces() {
    try {
        const resp = await fetch('/api/workspaces');
        workspacesCache = await resp.json();
        updateWorkspaceSelect();
    } catch (e) {
        console.error('Failed to fetch workspaces:', e);
    }
}

async function refreshWorkspacesTable() {
    try {
        const resp = await fetch('/api/workspaces');
        const workspaces = await resp.json();
        const tbody = document.querySelector('#workspaces-table tbody');
        const emptyState = document.getElementById('workspaces-empty');
        const table = document.getElementById('workspaces-table');

        if (workspaces.length === 0) {
            table.classList.add('hidden');
            emptyState.classList.remove('hidden');
            return;
        }

        table.classList.remove('hidden');
        emptyState.classList.add('hidden');

        tbody.innerHTML = workspaces.map(ws => {
            const branch = formatGitBranch(ws.git, ws.path);
            const gitStatus = formatGitStatus(ws.git, ws.in_use);
            const cstackStatus = formatCstackStatus(ws.cstack, ws);
            const lastCommit = formatLastCommit(ws.git);
            const sleeve = ws.sleeve_name || '-';
            const actions = formatWorkspaceActions(ws);
            return `
            <tr>
                <td>${ws.name}</td>
                <td>${branch}</td>
                <td>${gitStatus}</td>
                <td>${cstackStatus}</td>
                <td>${lastCommit}</td>
                <td>${sleeve}</td>
                <td>${actions}</td>
            </tr>
            `;
        }).join('');
    } catch (e) {
        console.error('Failed to fetch workspaces:', e);
    }
}

function formatGitBranch(git, wsPath) {
    if (!git) return '<span class="git-no-repo">-</span>';
    if (switchingBranchPaths.has(wsPath)) {
        return `<span style="color:var(--text-secondary)"><span class="inline-spinner"></span> Switching...</span>`;
    }
    const branchClass = git.is_detached ? 'git-detached' : 'git-branch';
    const prefix = git.is_detached ? 'detached@' : '';
    return `<span class="${branchClass}">${prefix}${git.branch}</span>`;
}

function formatGitStatus(git, inUse) {
    if (!git) return '-';
    const parts = [];

    if (git.is_dirty) {
        parts.push(`<span class="git-dirty">${git.uncommitted_count} uncommitted</span>`);
    } else {
        parts.push('<span class="git-clean">Clean</span>');
    }

    if (git.ahead_count > 0 || git.behind_count > 0) {
        const syncParts = [];
        if (git.ahead_count > 0) syncParts.push(`${git.ahead_count} ahead`);
        if (git.behind_count > 0) syncParts.push(`${git.behind_count} behind`);
        const syncClass = git.behind_count > 0 ? 'git-behind' : 'git-ahead';
        parts.push(`<span class="${syncClass}">${syncParts.join(', ')}</span>`);
    }

    return parts.join(' / ');
}

function formatLastCommit(git) {
    if (!git || !git.last_commit_hash) return '-';
    const msg = git.last_commit_msg.length > 30
        ? git.last_commit_msg.substring(0, 30) + '...'
        : git.last_commit_msg;
    return `<span class="git-commit-hash">${git.last_commit_hash}</span> ${msg} <span class="git-commit-time">(${git.last_commit_time})</span>`;
}

function formatWorkspaceActions(ws) {
    const isGitRepo = !!ws.git;
    const canSwitch = isGitRepo && !ws.in_use && !ws.git.is_dirty;
    const canPull = isGitRepo && !ws.in_use && !ws.git.is_dirty && ws.git.ahead_count === 0 && ws.git.behind_count > 0;
    const canCommit = isGitRepo && !ws.in_use && ws.git.is_dirty;
    const canPush = isGitRepo && !ws.in_use && ws.git.ahead_count > 0;

    const switchDisabled = !canSwitch ? 'disabled' : '';
    const fetchDisabled = !isGitRepo ? 'disabled' : '';
    const pullDisabled = !canPull ? 'disabled' : '';
    const commitDisabled = !canCommit ? 'disabled' : '';
    const pushDisabled = !canPush ? 'disabled' : '';

    let switchTitle = 'Switch branch';
    if (!isGitRepo) {
        switchTitle = 'Not a git repository';
    } else if (ws.in_use) {
        switchTitle = 'Workspace in use by ' + ws.sleeve_name;
    } else if (ws.git.is_dirty) {
        switchTitle = 'Has uncommitted changes';
    }

    let pullTitle = 'Pull from remote';
    if (!isGitRepo) {
        pullTitle = 'Not a git repository';
    } else if (ws.in_use) {
        pullTitle = 'Workspace in use by ' + ws.sleeve_name;
    } else if (ws.git.is_dirty) {
        pullTitle = 'Has uncommitted changes';
    } else if (ws.git.ahead_count > 0) {
        pullTitle = 'Has local commits - push first';
    } else if (ws.git.behind_count === 0) {
        pullTitle = 'Already up to date';
    }

    let commitTitle = 'Commit all changes';
    if (!isGitRepo) {
        commitTitle = 'Not a git repository';
    } else if (ws.in_use) {
        commitTitle = 'Workspace in use by ' + ws.sleeve_name;
    } else if (!ws.git.is_dirty) {
        commitTitle = 'No uncommitted changes';
    }

    let pushTitle = 'Push to remote';
    if (!isGitRepo) {
        pushTitle = 'Not a git repository';
    } else if (ws.in_use) {
        pushTitle = 'Workspace in use by ' + ws.sleeve_name;
    } else if (ws.git.ahead_count === 0) {
        pushTitle = 'No commits to push';
    }

    const escapedPath = ws.path.replace(/'/g, "\\'");
    const escapedName = ws.name.replace(/'/g, "\\'");

    return `<div class="action-buttons">
        <button class="btn btn-small" ${commitDisabled} title="${commitTitle}"
                onclick="commitChanges('${escapedPath}', this)">Commit</button>
        <button class="btn btn-small" ${pushDisabled} title="${pushTitle}"
                onclick="pushChanges('${escapedPath}', this)">Push</button>
        <button class="btn btn-small" ${switchDisabled} title="${switchTitle}"
                onclick="showSwitchModal('${escapedPath}', '${escapedName}')">Switch</button>
        <button class="btn btn-small" ${fetchDisabled} title="${isGitRepo ? 'Fetch from remote' : 'Not a git repository'}"
                onclick="fetchRemote('${escapedPath}', this)">Fetch</button>
        <button class="btn btn-small" ${pullDisabled} title="${pullTitle}"
                onclick="pullRemote('${escapedPath}', this)">Pull</button>
        <button class="btn btn-small" disabled title="Open in IDE (coming soon)"
                onclick="viewWorkspace('${escapedPath}')">View</button>
    </div>`;
}

let initializingCstackPaths = new Set();
let switchingBranchPaths = new Set();

function formatCstackStatus(cstack, ws) {
    if (initializingCstackPaths.has(ws.path)) {
        return `<span style="color:var(--text-secondary)"><span class="inline-spinner"></span> Initializing...</span>`;
    }

    if (!cstack || !cstack.exists) {
        const escapedPath = ws.path.replace(/'/g, "\\'");
        return `<button class="btn btn-small"
                onclick="showCstackInitModal('${escapedPath}', '${ws.name}')">
            Init Stack
        </button>`;
    }

    const parts = [];
    if (cstack.ready > 0) {
        parts.push(`<span style="color:var(--cyan-glow);font-weight:600">${cstack.ready} ready</span>`);
    }
    if (cstack.in_progress > 0) {
        parts.push(`<span style="color:var(--cyan-dim)">${cstack.in_progress} active</span>`);
    }
    if (cstack.blocked > 0) {
        parts.push(`<span style="color:var(--magenta-glow)">${cstack.blocked} blocked</span>`);
    }
    if (cstack.open > 0 && cstack.ready === 0 && cstack.in_progress === 0) {
        parts.push(`<span style="color:var(--amber-glow)">${cstack.open} open</span>`);
    }

    if (parts.length === 0) {
        return `<span style="color:var(--text-secondary)">No tasks</span>`;
    }

    return parts.join(' / ');
}

async function showSwitchModal(wsPath, wsName) {
    document.getElementById('switch-ws-name').textContent = wsName;
    document.getElementById('switch-ws-path').value = wsPath;
    document.getElementById('switch-error').classList.add('hidden');
    document.getElementById('branch-select').innerHTML = '<option value="">Loading branches...</option>';
    document.getElementById('switch-branch-modal').classList.add('active');

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}`);
        if (!resp.ok) {
            const err = await resp.text();
            document.getElementById('switch-error').textContent = err;
            document.getElementById('switch-error').classList.remove('hidden');
            return;
        }

        const branches = await resp.json();
        const select = document.getElementById('branch-select');
        let options = '';

        if (branches.local && branches.local.length > 0) {
            options += '<optgroup label="Local Branches">';
            for (const b of branches.local) {
                const selected = b === branches.current ? 'selected' : '';
                const current = b === branches.current ? ' (current)' : '';
                options += `<option value="${b}" ${selected}>${b}${current}</option>`;
            }
            options += '</optgroup>';
        }

        if (branches.remote && branches.remote.length > 0) {
            options += '<optgroup label="Remote Branches">';
            for (const b of branches.remote) {
                options += `<option value="${b}">${b}</option>`;
            }
            options += '</optgroup>';
        }

        if (options === '') {
            options = '<option value="">No branches found</option>';
        }

        select.innerHTML = options;
    } catch (e) {
        document.getElementById('switch-error').textContent = e.message;
        document.getElementById('switch-error').classList.remove('hidden');
    }
}

function hideSwitchModal() {
    document.getElementById('switch-branch-modal').classList.remove('active');
    document.getElementById('switch-form').reset();
    document.getElementById('switch-error').classList.add('hidden');
}

async function switchBranch(e) {
    e.preventDefault();
    const wsPath = document.getElementById('switch-ws-path').value;
    const branch = document.getElementById('branch-select').value;

    if (!branch) {
        document.getElementById('switch-error').textContent = 'Please select a branch';
        document.getElementById('switch-error').classList.remove('hidden');
        return;
    }

    hideSwitchModal();
    switchingBranchPaths.add(wsPath);
    await refreshWorkspacesTable();

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}&action=switch`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ workspace: wsPath, branch: branch })
        });

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Branch switch failed', { detail: err });
        }
    } catch (err) {
        notify.error('Branch switch failed', { detail: err.message });
    } finally {
        switchingBranchPaths.delete(wsPath);
        await refreshWorkspacesTable();
    }
}

async function fetchRemote(wsPath, btn) {
    const originalText = btn.textContent;
    btn.textContent = 'Fetching...';
    btn.disabled = true;

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}&action=fetch`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ workspace: wsPath })
        });

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Fetch failed', { detail: err });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        btn.textContent = 'Done!';
        setTimeout(() => {
            btn.textContent = originalText;
            btn.disabled = false;
            refreshWorkspacesTable();
        }, 1000);
    } catch (e) {
        notify.error('Fetch failed', { detail: e.message });
        btn.textContent = originalText;
        btn.disabled = false;
    }
}

async function pullRemote(wsPath, btn) {
    const originalText = btn.textContent;
    btn.textContent = 'Pulling...';
    btn.disabled = true;

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}&action=pull`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ workspace: wsPath })
        });

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Pull failed', { detail: err });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        const result = await resp.json();
        if (!result.success) {
            notify.error('Pull failed', { detail: result.message || 'unknown error' });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        btn.textContent = 'Done!';
        setTimeout(() => {
            btn.textContent = originalText;
            btn.disabled = false;
            refreshWorkspacesTable();
        }, 1000);
    } catch (e) {
        notify.error('Pull failed', { detail: e.message });
        btn.textContent = originalText;
        btn.disabled = false;
    }
}

async function commitChanges(wsPath, btn) {
    const originalText = btn.textContent;
    btn.textContent = 'Committing...';
    btn.disabled = true;

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}&action=commit`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' }
        });

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Commit failed', { detail: err });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        const result = await resp.json();
        if (!result.success) {
            notify.error('Commit failed', { detail: result.message || 'unknown error' });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        btn.textContent = 'Done!';
        setTimeout(() => {
            btn.textContent = originalText;
            btn.disabled = false;
            refreshWorkspacesTable();
        }, 1000);
    } catch (e) {
        notify.error('Commit failed', { detail: e.message });
        btn.textContent = originalText;
        btn.disabled = false;
    }
}

async function pushChanges(wsPath, btn) {
    const originalText = btn.textContent;
    btn.textContent = 'Pushing...';
    btn.disabled = true;

    try {
        const resp = await fetch(`/api/workspaces/branches?workspace=${encodeURIComponent(wsPath)}&action=push`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' }
        });

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Push failed', { detail: err });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        const result = await resp.json();
        if (!result.success) {
            notify.error('Push failed', { detail: result.message || 'unknown error' });
            btn.textContent = originalText;
            btn.disabled = false;
            return;
        }

        btn.textContent = 'Done!';
        setTimeout(() => {
            btn.textContent = originalText;
            btn.disabled = false;
            refreshWorkspacesTable();
        }, 1000);
    } catch (e) {
        notify.error('Push failed', { detail: e.message });
        btn.textContent = originalText;
        btn.disabled = false;
    }
}

function viewWorkspace(wsPath) {
    notify.info('View workspace coming soon', { detail: 'Options: LazyGit, VS Code (code-server), or web file browser' });
}

function showCstackInitModal(wsPath, wsName) {
    document.getElementById('cstack-init-ws-name').textContent = wsName;
    document.getElementById('cstack-init-ws-path').value = wsPath;
    document.getElementById('cstack-init-error').classList.add('hidden');
    document.getElementById('cstack-init-modal').classList.add('active');
}

function hideCstackInitModal() {
    document.getElementById('cstack-init-modal').classList.remove('active');
    document.getElementById('cstack-init-form').reset();
    document.getElementById('cstack-init-error').classList.add('hidden');
}

async function initCstack(e) {
    e.preventDefault();
    const wsPath = document.getElementById('cstack-init-ws-path').value;
    const mode = document.querySelector('input[name="cstack-mode"]:checked').value;

    hideCstackInitModal();
    initializingCstackPaths.add(wsPath);
    await refreshWorkspacesTable();

    try {
        const resp = await fetch(
            `/api/workspaces/cstack?workspace=${encodeURIComponent(wsPath)}&action=init`,
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ mode: mode })
            }
        );

        const result = await resp.json();
        if (!result.success && result.error && !result.error.includes('already')) {
            notify.error('Cstack init failed', { detail: result.error });
        }
    } catch (err) {
        notify.error('Cstack init failed', { detail: err.message });
    } finally {
        initializingCstackPaths.delete(wsPath);
        await refreshWorkspacesTable();
    }
}

function updateWorkspaceSelect() {
    const select = document.getElementById('existing-workspace-select');
    const pendingPaths = new Set(pendingSpawns.map(p => p.workspace));
    const available = workspacesCache.filter(ws => !ws.in_use && !pendingPaths.has(ws.path));
    if (available.length === 0) {
        select.innerHTML = '<option value="">No available workspaces</option>';
        return;
    }
    select.innerHTML = available.map(ws =>
        `<option value="${ws.path}">${ws.name}</option>`
    ).join('');
}

let hostLimitsCache = null;

async function loadHostLimits() {
    try {
        const resp = await fetch('/api/host/limits');
        hostLimitsCache = await resp.json();

        const memSelect = document.getElementById('memory-limit-select');
        const cpuSelect = document.getElementById('cpu-limit-select');

        memSelect.innerHTML = '<option value="0">Unlimited</option>';
        if (hostLimitsCache.memory_options) {
            hostLimitsCache.memory_options.forEach(mb => {
                const gb = (mb / 1024).toFixed(1);
                memSelect.innerHTML += `<option value="${mb}">${gb} GB</option>`;
            });
        }

        cpuSelect.innerHTML = '<option value="0">Unlimited</option>';
        if (hostLimitsCache.cpu_options) {
            hostLimitsCache.cpu_options.forEach(cpu => {
                cpuSelect.innerHTML += `<option value="${cpu}">${cpu} cores</option>`;
            });
        }
    } catch (e) {
        console.error('Failed to load host limits:', e);
    }
}

function toggleAdvancedOptions() {
    const advanced = document.getElementById('advanced-options');
    const toggle = document.getElementById('advanced-toggle');
    if (toggle.checked) {
        advanced.classList.remove('hidden');
        loadHostLimits();
    } else {
        advanced.classList.add('hidden');
    }
}

async function showSpawnModal() {
    await refreshWorkspaces();
    document.getElementById('advanced-toggle').checked = false;
    document.getElementById('advanced-options').classList.add('hidden');
    document.getElementById('spawn-modal').classList.add('active');
}

function hideSpawnModal() {
    document.getElementById('spawn-modal').classList.remove('active');
    document.getElementById('spawn-form').reset();
    hideSpawnLoading();
}

function hideSpawnLoading() {
    document.getElementById('spawn-loading').classList.remove('active');
}

function showCloneModal() {
    document.getElementById('clone-modal').classList.add('active');
    document.getElementById('clone-error').classList.add('hidden');
}

function hideCloneModal() {
    document.getElementById('clone-modal').classList.remove('active');
    document.getElementById('clone-form').reset();
    document.getElementById('clone-error').classList.add('hidden');
    hideCloneLoading();
    currentCloneJobId = null;
}

function showCloneLoading(msg) {
    document.querySelector('.clone-loading-text').textContent = msg || 'Cloning repository...';
    document.getElementById('clone-loading').classList.add('active');
    document.getElementById('clone-submit-btn').disabled = true;
}

function hideCloneLoading() {
    document.getElementById('clone-loading').classList.remove('active');
    document.getElementById('clone-submit-btn').disabled = false;
}

function showCreateWorkspaceModal() {
    document.getElementById('create-workspace-modal').classList.add('active');
    document.getElementById('create-workspace-error').classList.add('hidden');
}

function hideCreateWorkspaceModal() {
    document.getElementById('create-workspace-modal').classList.remove('active');
    document.getElementById('create-workspace-form').reset();
    document.getElementById('create-workspace-error').classList.add('hidden');
}

async function createWorkspace(e) {
    e.preventDefault();
    const name = document.getElementById('create-workspace-name').value.trim();

    if (!name) {
        document.getElementById('create-workspace-error').textContent = 'Please enter a workspace name';
        document.getElementById('create-workspace-error').classList.remove('hidden');
        return;
    }

    const btn = document.getElementById('create-workspace-btn');
    const originalText = btn.textContent;
    btn.textContent = 'Creating...';
    btn.disabled = true;

    try {
        const resp = await fetch('/api/workspaces', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: name })
        });

        if (!resp.ok) {
            const err = await resp.text();
            document.getElementById('create-workspace-error').textContent = err;
            document.getElementById('create-workspace-error').classList.remove('hidden');
            return;
        }

        hideCreateWorkspaceModal();
        notify.success('Workspace created', { detail: name });
        refreshWorkspacesTable();
        refreshWorkspaces();
    } catch (err) {
        document.getElementById('create-workspace-error').textContent = err.message;
        document.getElementById('create-workspace-error').classList.remove('hidden');
    } finally {
        btn.textContent = originalText;
        btn.disabled = false;
    }
}

function handleCloneProgress(data) {
    if (data.id !== currentCloneJobId) return;

    if (data.status === 'completed') {
        hideCloneLoading();
        hideCloneModal();
        refreshWorkspacesTable();
        refreshWorkspaces();
    } else if (data.status === 'failed') {
        hideCloneLoading();
        document.getElementById('clone-error').textContent = data.error || 'Clone failed';
        document.getElementById('clone-error').classList.remove('hidden');
    }
}

async function cloneWorkspace(e) {
    e.preventDefault();
    const repoUrl = document.getElementById('clone-url-input').value;
    const name = document.getElementById('clone-name-input').value;

    if (!repoUrl) {
        notify.warn('Please enter a repository URL');
        return;
    }

    showCloneLoading('Starting clone...');
    document.getElementById('clone-error').classList.add('hidden');

    try {
        const resp = await fetch('/api/workspaces/clone', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ repo_url: repoUrl, name: name || undefined })
        });

        if (!resp.ok) {
            const err = await resp.text();
            document.getElementById('clone-error').textContent = err;
            document.getElementById('clone-error').classList.remove('hidden');
            hideCloneLoading();
            return;
        }

        const job = await resp.json();
        currentCloneJobId = job.id;
        showCloneLoading('Cloning repository...');
        // Clone progress will be received via SSE clone:progress events

    } catch (e) {
        hideCloneLoading();
        document.getElementById('clone-error').textContent = e.message;
        document.getElementById('clone-error').classList.remove('hidden');
    }
}

function removePendingSpawn(pendingId) {
    pendingSpawns = pendingSpawns.filter(p => p.id !== pendingId);
}

async function spawnSleeve(e) {
    e.preventDefault();
    const name = document.getElementById('name-input').value;
    const workspace = document.getElementById('existing-workspace-select').value;
    let pendingId = null;

    if (!workspace) {
        notify.warn('Please select a workspace');
        return;
    }

    try {
        const body = { workspace, name: name || undefined };

        if (document.getElementById('advanced-toggle').checked) {
            const memLimit = parseInt(document.getElementById('memory-limit-select').value);
            const cpuLimit = parseInt(document.getElementById('cpu-limit-select').value);
            if (memLimit > 0) body.memory_limit_mb = memLimit;
            if (cpuLimit > 0) body.cpu_limit = cpuLimit;
        }

        pendingId = Date.now().toString();
        pendingSpawns.push({
            id: pendingId,
            workspace: workspace,
            name: name || null,
            message: 'Spawning sleeve...'
        });

        hideSpawnModal();
        renderPendingSpawns();

        const resp = await fetch('/api/sleeves', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        });

        removePendingSpawn(pendingId);
        renderPendingSpawns();

        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Failed to spawn sleeve', { detail: err });
            return;
        }

        const sleeve = await resp.json();
        notify.success('Sleeve spawned', { detail: sleeve.name });
        refreshSleeves(); // Update cache for needlecast
    } catch (err) {
        if (pendingId) {
            removePendingSpawn(pendingId);
            renderPendingSpawns();
        }
        console.error('Failed to spawn sleeve:', err);
        notify.error('Failed to spawn sleeve', { detail: err.message });
    }
}

async function killSleeve(name) {
    if (!confirm(`Are you sure you want to kill sleeve "${name}"?`)) {
        return;
    }

    try {
        const resp = await fetch(`/api/sleeves/${name}`, { method: 'DELETE' });
        if (!resp.ok) {
            const err = await resp.text();
            notify.error('Failed to kill sleeve', { detail: err });
            return;
        }
        // Sleeve removal will be handled via SSE sleeve:remove event
        refreshSleeves(); // Update cache for needlecast
    } catch (e) {
        console.error('Failed to kill sleeve:', e);
        notify.error('Failed to kill sleeve', { detail: e.message });
    }
}

function openTerminalWithPath(title, wsPath, readOnly = false) {
    document.getElementById('terminal-title').textContent = title;
    document.getElementById('terminal-modal').classList.add('active');
    document.getElementById('reconnect-info').textContent = '';

    const readOnlyBadge = document.getElementById('terminal-readonly-badge');
    if (readOnly) {
        readOnlyBadge.classList.add('active');
    } else {
        readOnlyBadge.classList.remove('active');
    }

    const container = document.getElementById('terminal-container');

    if (currentConnection) {
        currentConnection.dispose();
    }

    currentConnection = new TerminalConnection(container, wsPath, title, readOnly);
    currentConnection.connect();
}

function openTerminal(name) {
    openTerminalWithPath(`Terminal - ${name}`, `/sleeves/${name}/terminal`);
}

function openTerminalObserve(name) {
    openTerminalWithPath(`Terminal - ${name} (Observe)`, `/sleeves/${name}/terminal?mode=observe`, true);
}

function openEnvoyTerminal() {
    openTerminalWithPath('Terminal - Envoy (Poe)', '/envoy/terminal');
}

function hideTerminalModal() {
    document.getElementById('terminal-modal').classList.remove('active');
    if (currentConnection) {
        currentConnection.dispose();
        currentConnection = null;
    }
}

// Auth status check
async function checkAuthStatus() {
    if (notify.isDismissed('auth-warning')) return;

    try {
        const resp = await fetch('/api/auth/check');
        const result = await resp.json();

        if (result.expired) {
            notify.warn('Authentication expired', {
                detail: 'Claude credentials need renewal',
                action: {
                    text: 'View Details',
                    onclick: () => {
                        switchTab('doctor');
                        notify.markDismissed('auth-warning');
                    }
                },
                autoDismiss: 15000
            });
        } else if (result.expiring_soon) {
            notify.warn('Authentication expiring soon', {
                detail: 'Check Doctor tab for details',
                action: {
                    text: 'View Details',
                    onclick: () => {
                        switchTab('doctor');
                        notify.markDismissed('auth-warning');
                    }
                },
                autoDismiss: 15000
            });
        }
    } catch (e) {
        console.error('Failed to check auth status:', e);
    }
}

// Process OOB HTML fragment from SSE
function processOOBSwap(html) {
    const temp = document.createElement('div');
    temp.innerHTML = html;

    const oobElements = temp.querySelectorAll('[hx-swap-oob]');
    oobElements.forEach(el => {
        const swapSpec = el.getAttribute('hx-swap-oob');
        const match = swapSpec.match(/^(outerHTML|innerHTML):#(.+)$/);
        if (match) {
            const swapType = match[1];
            const targetId = match[2];
            const target = document.getElementById(targetId);
            if (target) {
                el.removeAttribute('hx-swap-oob');
                if (swapType === 'outerHTML') {
                    target.outerHTML = el.outerHTML;
                } else {
                    target.innerHTML = el.innerHTML;
                }
            }
        }
    });
}

// Native SSE connection for reliable event handling
let sseSource = null;
let sseReconnectAttempts = 0;
const SSE_MAX_RECONNECT = 10;
const SSE_RECONNECT_BASE_DELAY = 1000;

function connectSSE() {
    if (sseSource) {
        sseSource.close();
    }

    sseSource = new EventSource('/api/events');

    sseSource.onopen = () => {
        sseConnected = true;
        sseReconnectAttempts = 0;
        console.log('SSE connected');
    };

    sseSource.onerror = () => {
        sseConnected = false;
        sseSource.close();
        sseSource = null;

        if (sseReconnectAttempts < SSE_MAX_RECONNECT) {
            const delay = Math.min(SSE_RECONNECT_BASE_DELAY * Math.pow(2, sseReconnectAttempts), 30000);
            sseReconnectAttempts++;
            console.log(`SSE disconnected, reconnecting in ${delay}ms...`);
            setTimeout(connectSSE, delay);
        } else {
            console.error('SSE max reconnect attempts reached');
        }
    };

    // Handle connected event (immediate response before data loads)
    sseSource.addEventListener('connected', (e) => {
        const grid = document.getElementById('sleeves-grid');
        if (grid) {
            grid.innerHTML = '<div class="empty-state"><span class="inline-spinner"></span> Loading sleeves...</div>';
        }
    });

    // Handle init event
    sseSource.addEventListener('init', (e) => {
        const grid = document.getElementById('sleeves-grid');
        if (grid) {
            grid.innerHTML = e.data;
            updateSleeveCount();
            renderPendingSpawns();
        }
        refreshSleeves();
    });

    // Handle sleeve:add event
    sseSource.addEventListener('sleeve:add', (e) => {
        const grid = document.getElementById('sleeves-grid');
        if (grid) {
            const emptyState = grid.querySelector('.empty-state');
            if (emptyState) {
                emptyState.remove();
            }
            grid.insertAdjacentHTML('beforeend', e.data);
            updateSleeveCount();
        }
        refreshSleeves();
    });

    // Handle sleeve:update event
    sseSource.addEventListener('sleeve:update', (e) => {
        processOOBSwap(e.data);
    });

    // Handle sleeve:remove event
    sseSource.addEventListener('sleeve:remove', (e) => {
        const data = JSON.parse(e.data);
        const el = document.getElementById(`sleeve-${data.name}`);
        if (el) {
            el.classList.add('removing');
            setTimeout(() => {
                el.remove();
                updateSleeveCount();
                const grid = document.getElementById('sleeves-grid');
                if (grid && grid.querySelectorAll('.sleeve-card').length === 0) {
                    grid.innerHTML = '<div class="empty-state">No sleeves running. Click "+ SPAWN SLEEVE" to create one.</div>';
                }
            }, 200);
        }
        refreshSleeves();
    });

    // Handle host:stats event
    sseSource.addEventListener('host:stats', (e) => {
        processOOBSwap(e.data);
    });

    // Handle clone:progress event
    sseSource.addEventListener('clone:progress', (e) => {
        const data = JSON.parse(e.data);
        handleCloneProgress(data);
    });
}

// ============================================================================
// CONFIG TAB
// ============================================================================

async function refreshConfig() {
    try {
        const resp = await fetch('/api/config');
        if (!resp.ok) throw new Error('Failed to fetch config');
        const config = await resp.json();

        // Populate sleeves settings
        setInputValue('config-sleeves-max', config.sleeves?.max);
        setInputValue('config-sleeves-poll_interval', config.sleeves?.poll_interval);
        setInputValue('config-sleeves-idle_threshold', config.sleeves?.idle_threshold);
        setInputValue('config-sleeves-image', config.sleeves?.image);

        // Populate docker settings (read-only)
        setInputValue('config-docker-network', config.docker?.network);
        setInputValue('config-docker-workspace_root', config.docker?.workspace_root);

        // Populate git settings
        setInputValue('config-git-clone_protocol', config.git?.clone_protocol);
        setInputValue('config-git-committer-name', config.git?.committer?.name || '');
        setInputValue('config-git-committer-email', config.git?.committer?.email || '');

        // Populate server settings (read-only)
        setInputValue('config-server-port', config.server?.port);
    } catch (err) {
        console.error('Failed to refresh config:', err);
        notify.error('Config Error', { detail: 'Failed to load configuration' });
    }
}

function setInputValue(id, value) {
    const el = document.getElementById(id);
    if (el) {
        el.value = value ?? '';
    }
}

async function saveConfig(key) {
    const inputId = 'config-' + key.replace(/\./g, '-');
    const input = document.getElementById(inputId);
    if (!input) {
        notify.error('Config Error', { detail: `Input not found for ${key}` });
        return;
    }

    const value = input.value;
    const statusEl = document.getElementById('config-save-status');

    try {
        const resp = await fetch(`/api/config/${key}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ value: String(value) })
        });

        if (!resp.ok) {
            const errText = await resp.text();
            throw new Error(errText);
        }

        const result = await resp.json();
        if (statusEl) {
            statusEl.textContent = 'Saved - restart required';
            statusEl.className = 'config-save-status saved';
            setTimeout(() => { statusEl.textContent = ''; }, 3000);
        }
        notify.success('Config Saved', { detail: `${key} = ${result.value}` });
    } catch (err) {
        if (statusEl) {
            statusEl.textContent = 'Save failed';
            statusEl.className = 'config-save-status error';
        }
        notify.error('Config Error', { detail: err.message });
    }
}

async function refreshAuthStatus() {
    try {
        const resp = await fetch('/api/auth/status');
        if (!resp.ok) throw new Error('Failed to fetch auth status');
        const status = await resp.json();

        updateAuthBadge('claude', status.providers?.claude);
        updateAuthBadge('gemini', status.providers?.gemini);
        updateAuthBadge('codex', status.providers?.codex);
        updateAuthBadge('git', status.providers?.git);
    } catch (err) {
        console.error('Failed to refresh auth status:', err);
    }
}

function updateAuthBadge(provider, info) {
    const badge = document.getElementById(`auth-${provider}-status`);
    if (!badge) return;

    if (!info) {
        badge.className = 'auth-status auth-status-unknown';
        badge.textContent = 'unknown';
        return;
    }

    if (info.authenticated) {
        badge.className = 'auth-status auth-status-authenticated';
        badge.textContent = info.method || 'authenticated';
    } else {
        badge.className = 'auth-status auth-status-missing';
        badge.textContent = 'not configured';
    }
}

async function saveAuthKey(provider) {
    const input = document.getElementById(`auth-${provider}-key`);
    if (!input) return;

    const token = input.value.trim();
    if (!token) {
        notify.error('Auth Error', { detail: 'Please enter an API key' });
        return;
    }

    try {
        const resp = await fetch(`/api/auth/${provider}/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ token })
        });

        const result = await resp.json();
        if (!resp.ok || !result.success) {
            throw new Error(result.error || 'Failed to save credentials');
        }

        input.value = '';
        notify.success('Auth Saved', { detail: `${provider} credentials saved successfully` });
        refreshAuthStatus();
    } catch (err) {
        notify.error('Auth Error', { detail: err.message });
    }
}

async function revokeAuth(provider) {
    if (!confirm(`Revoke ${provider} credentials? This cannot be undone.`)) {
        return;
    }

    try {
        const resp = await fetch(`/api/auth/${provider}`, {
            method: 'DELETE'
        });

        const result = await resp.json();
        if (!resp.ok || !result.success) {
            throw new Error(result.error || 'Failed to revoke credentials');
        }

        notify.success('Auth Revoked', { detail: `${provider} credentials removed` });
        refreshAuthStatus();
    } catch (err) {
        notify.error('Auth Error', { detail: err.message });
    }
}

async function restartEnvoy() {
    if (!confirm('Restart Envoy? This will briefly disconnect all sleeves and the dashboard.')) {
        return;
    }

    try {
        const resp = await fetch('/api/restart', { method: 'POST' });
        const result = await resp.json();

        notify.info('Restarting', { detail: result.message || 'Envoy is restarting...' });

        // Wait a moment then start checking for reconnection
        setTimeout(() => {
            checkReconnection();
        }, 2000);
    } catch (err) {
        notify.error('Restart Failed', { detail: err.message });
    }
}

function checkReconnection() {
    let attempts = 0;
    const maxAttempts = 30;

    const check = async () => {
        attempts++;
        try {
            const resp = await fetch('/health');
            if (resp.ok) {
                notify.success('Reconnected', { detail: 'Envoy is back online' });
                location.reload();
                return;
            }
        } catch (e) {
            // Still down, keep trying
        }

        if (attempts < maxAttempts) {
            setTimeout(check, 1000);
        } else {
            notify.error('Connection Lost', { detail: 'Could not reconnect to Envoy. Please refresh manually.' });
        }
    };

    check();
}

// ============================================================================
// INITIALIZATION
// ============================================================================

// Initialize on page load
connectSSE();
refreshSleeves();
checkAuthStatus();

// Check auth status every 5 minutes
setInterval(() => {
    checkAuthStatus();
}, 5 * 60 * 1000);
