let configData = null;
let interfaceList = [];

function clearFieldErrors() {
    document.querySelectorAll('.field-error').forEach(el => el.remove());
    document.querySelectorAll('.border-red-500').forEach(el => el.classList.remove('border-red-500'));
}

function showToast(msg, type) {
    // Remove any existing toast
    const existing = document.getElementById('toast');
    if (existing) existing.remove();

    const t = document.createElement('div');
    t.id = 'toast';
    t.className = 'fixed bottom-4 right-4 px-4 py-3 rounded-lg shadow-lg text-white text-sm font-medium translate-y-2 opacity-0 transition-all duration-300 pointer-events-auto z-50 flex items-center gap-3';
    if (type === 'error') t.classList.add('bg-red-600');
    else if (type === 'success') t.classList.add('bg-green-600');
    else t.classList.add('bg-gray-800');

    const span = document.createElement('span');
    span.textContent = msg;
    t.appendChild(span);

    if (type === 'error') {
        const closeBtn = document.createElement('button');
        closeBtn.textContent = '\u2715';
        closeBtn.className = 'text-white opacity-70 hover:opacity-100 font-bold text-lg leading-none ml-1';
        closeBtn.onclick = () => t.remove();
        t.appendChild(closeBtn);
    }

    // Log errors to server
    if (type === 'error') {
        logToServer('ERROR', msg);
    }

    document.body.appendChild(t);

    requestAnimationFrame(() => {
        t.classList.add('show');
        const duration = type === 'error' ? 10000 : 3000;
        const timer = setTimeout(() => {
            if (t.parentNode) t.remove();
        }, duration);
        if (type === 'error') {
            t._dismissTimer = timer;
            const origOnClick = closeBtn.onclick;
            closeBtn.onclick = () => {
                clearTimeout(timer);
                t.remove();
            };
        }
    });
}

async function logToServer(level, message) {
    try {
        await fetch('/api/log', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ level, message }),
        });
    } catch (e) {
        // best-effort; don't loop
    }
}

function formatUptime(secs) {
    if (!secs && secs !== 0) return '--';
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    const parts = [];
    if (d > 0) parts.push(d + 'd');
    if (h > 0) parts.push(h + 'h');
    if (m > 0) parts.push(m + 'm');
    parts.push(s + 's');
    return parts.join(' ');
}

function updateStateIndicator(state) {
    const dot = document.getElementById('state-dot');
    const text = document.getElementById('state-text');
    text.textContent = state;
    dot.className = 'w-3 h-3 rounded-full ';
    switch (state) {
        case 'Running': dot.classList.add('bg-green-500'); break;
        case 'Starting':
        case 'Stopping':
            dot.classList.add('bg-yellow-500', 'animate-pulse');
            break;
        default: dot.classList.add('bg-red-500');
    }
}

function updateTransportButtons(state, transport) {
    const btnSc = document.getElementById('btn-switch-sc');
    const btnTs = document.getElementById('btn-switch-tailscale');
    const btnStop = document.getElementById('btn-stop');
    const btnStart = document.getElementById('btn-start');

    const cfgBtnStart = document.getElementById('cfg-btn-start');
    const cfgBtnStop = document.getElementById('cfg-btn-stop');
    const cfgSwitchRow = document.getElementById('cfg-switch-row');
    const cfgBtnSwitchSc = document.getElementById('cfg-btn-switch-sc');
    const cfgBtnSwitchTs = document.getElementById('cfg-btn-switch-tailscale');

    const isRunning = state === 'Running';
    const isStopped = state === 'Stopped';

    btnStart.disabled = !isStopped;
    btnStop.disabled = !isRunning;
    btnSc.disabled = !isRunning || transport === 'sc';
    btnTs.disabled = !isRunning || transport === 'tailscale';

    cfgBtnStart.style.display = isStopped ? '' : 'none';
    cfgBtnStop.style.display = isRunning ? '' : 'none';
    cfgSwitchRow.style.display = isRunning ? '' : 'none';
    cfgBtnSwitchSc.disabled = transport === 'sc';
    cfgBtnSwitchTs.disabled = transport === 'tailscale';
}

async function updateStatus() {
    try {
        const resp = await fetch('/api/status');
        if (!resp.ok) return;
        const data = await resp.json();

        document.getElementById('stat-transport').textContent = data.transport || '--';
        document.getElementById('stat-uptime').textContent = formatUptime(data.uptime_secs);
        document.getElementById('stat-lan-ip').textContent = data.lan_ip || '(auto)';
        document.getElementById('stat-lan-port').textContent = data.lan_port || '--';
        document.getElementById('stat-url').textContent = data.connected_url || '(none)';
        document.getElementById('stat-device-id').textContent = data.device_id || '--';
        document.getElementById('transport-current').textContent = data.transport || '--';
        document.getElementById('hub-mode-current').textContent = data.hub_mode || '--';

        updateStateIndicator(data.state);
        updateTransportButtons(data.state, data.transport);

        const transportLoading = document.getElementById('transport-loading');
        if (data.state === 'Starting' || data.state === 'Stopping') {
            transportLoading.classList.remove('hidden');
        } else {
            transportLoading.classList.add('hidden');
        }
    } catch (e) {
        // silent
    }
}

async function updateHubStatus() {
    try {
        const hubCard = document.getElementById('hub-card');

        // Don't show hub info when transport is tailscale (SC-only)
        const statusResp = await fetch('/api/status');
        if (statusResp.ok) {
            const statusData = await statusResp.json();
            if (statusData.transport === 'tailscale') {
                hubCard.style.display = 'none';
                return;
            }
            hubCard.style.display = '';
        }

        const resp = await fetch('/api/hub/status');
        if (!resp.ok) return;
        const data = await resp.json();

        const addrRow = document.getElementById('hub-addr-row');
        const spokeRow = document.getElementById('hub-spoke-row');

        document.getElementById('hub-mode-current').textContent = data.mode || '--';

        const btnCloud = document.getElementById('btn-hub-cloud');
        const btnEmbedded = document.getElementById('btn-hub-embedded');

        if (data.mode === 'embedded') {
            btnCloud.disabled = false;
            btnEmbedded.disabled = true;
            addrRow.style.display = 'flex';
            spokeRow.style.display = 'flex';
            document.getElementById('hub-listen-addr').textContent = data.listen_addr || '--';
            document.getElementById('hub-spoke-count').textContent = data.spoke_count || '0';
        } else {
            btnCloud.disabled = true;
            btnEmbedded.disabled = false;
            addrRow.style.display = 'none';
            spokeRow.style.display = 'none';
        }
    } catch (e) {
        // silent
    }
}

async function switchHubMode(mode) {
    const btn = mode === 'cloud' ? document.getElementById('btn-hub-cloud') : document.getElementById('btn-hub-embedded');
    btn.disabled = true;

    try {
        const resp = await fetch('/api/hub/mode', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ mode }),
        });

        if (resp.ok) {
            showToast(`Switched to ${mode} hub mode`, 'success');
            updateHubStatus();
        } else if (resp.status === 409) {
            const err = await resp.json();
            showToast(err.error || 'Cannot switch while router is running', 'error');
        } else {
            const err = await resp.json();
            showToast(err.error || 'Switch failed', 'error');
        }
    } catch (e) {
        logToServer('ERROR', 'Network error switching hub mode: ' + (e.message || e));
        showToast('Network error', 'error');
    } finally {
        btn.disabled = false;
    }
}

async function loadInterfaces() {
    try {
        const resp = await fetch('/api/interfaces');
        if (!resp.ok) return;
        const data = await resp.json();
        interfaceList = data.interfaces || [];
        const list = document.getElementById('interfaces-list');
        if (!data.interfaces || data.interfaces.length === 0) {
            list.innerHTML = '<p class="text-sm text-gray-400">No interfaces configured</p>';
            return;
        }
        const typeBadge = (type) => {
            if (type === 'Tailscale') return '<span class="px-1.5 py-0.5 text-xs rounded bg-green-100 text-green-700 font-medium">TS</span>';
            if (type === 'LAN') return '<span class="px-1.5 py-0.5 text-xs rounded bg-blue-100 text-blue-700 font-medium">LAN</span>';
            return '<span class="px-1.5 py-0.5 text-xs rounded bg-gray-100 text-gray-600 font-medium">' + type + '</span>';
        };
        list.innerHTML = `
            <table class="w-full text-sm">
                <thead>
                    <tr class="text-left text-gray-500 border-b">
                        <th class="pb-2">Interface</th>
                        <th class="pb-2">IP Address</th>
                        <th class="pb-2 w-16">Type</th>
                    </tr>
                </thead>
                <tbody>
                    ${data.interfaces.map(iface => `
                        <tr class="border-b border-gray-100 hover:bg-gray-50">
                            <td class="py-2 font-medium text-gray-700">${iface.name}</td>
                            <td class="py-2 font-mono text-gray-600">${iface.ip}</td>
                            <td class="py-2">${typeBadge(iface.type)}</td>
                        </tr>
                    `).join('')}
                </tbody>
            </table>`;
        if (configData) {
            renderConfigForm(configData);
        }
    } catch (e) {
        // silent
    }
}

async function refreshInterfaces() {
    await loadInterfaces();
}

async function loadConfig() {
    try {
        const [configResp, ifaceResp] = await Promise.all([
            fetch('/api/config'),
            fetch('/api/interfaces'),
        ]);
        if (!configResp.ok) return;
        configData = await configResp.json();
        if (ifaceResp.ok) {
            const data = await ifaceResp.json();
            interfaceList = data.interfaces || [];
        }
        renderConfigForm(configData);
    } catch (e) {
        // silent
    }
}

function renderConfigForm(config) {
    const form = document.getElementById('config-form');
    form.innerHTML = '';

    const sections = [
        {
            title: 'Router',
            fields: [
                { key: 'router.device_id', label: 'Device ID', type: 'number' },
                { key: 'router.vendor_id', label: 'Vendor ID', type: 'number' },
                { key: 'router.device_name', label: 'Device Name', type: 'text' },
                {
                    key: 'router.transport',
                    label: 'Transport',
                    type: 'select',
                    options: [
                        { value: 'sc', label: 'BACnet/SC' },
                        { value: 'tailscale', label: 'Tailscale BBMD' }
                    ]
                },
            ]
        },
        {
            title: 'LAN (BACnet/IP)',
            fields: [
                { key: 'router.lan.interface', label: 'Interface IP', type: 'select',
                    options: () => interfaceList
                        .filter(i => !i.ip.startsWith('100.') && !i.ip.startsWith('127.') && i.ip.includes('.'))
                        .map(i => ({ value: i.ip, label: `${i.name} (${i.ip})` })),
                    placeholder: 'e.g., 192.168.1.100' },
                { key: 'router.lan.port', label: 'Port', type: 'number' },
            ]
        },
        {
            title: 'BACnet/SC',
            fields: [
                { key: 'router.sc.hub_url', label: 'Hub URL', type: 'text', placeholder: 'wss://hub.example.com' },
                { key: 'router.sc.reconnect_initial_ms', label: 'Reconnect Initial (ms)', type: 'number' },
                { key: 'router.sc.reconnect_max_ms', label: 'Reconnect Max (ms)', type: 'number' },
                { key: 'router.sc.reconnect_max_attempts', label: 'Max Attempts (0=infinite)', type: 'number' },
            ]
        },
        {
            title: 'Tailscale',
            fields: [
                { key: 'router.tailscale.interface', label: 'Interface IP', type: 'select',
                    options: () => interfaceList
                        .filter(i => i.ip.startsWith('100.'))
                        .map(i => ({ value: i.ip, label: `${i.name} (${i.ip})` })),
                    placeholder: 'e.g., 100.64.0.1' },
                { key: 'router.tailscale.port', label: 'Port', type: 'number' },
            ]
        },
        {
            title: 'Web Dashboard',
            fields: [
                { key: 'web.host', label: 'Bind Host', type: 'text' },
                { key: 'web.port', label: 'Port', type: 'number' },
                { key: 'web.open_browser', label: 'Open Browser', type: 'checkbox' },
            ]
        },
    ];

    sections.forEach(section => {
        const div = document.createElement('div');
        div.className = 'config-section';
        div.innerHTML = `<h3 class="text-sm font-semibold text-gray-700 mb-3">${section.title}</h3>`;

        section.fields.forEach(field => {
            const val = getNestedValue(config, field.key);
            const isScField = field.key.startsWith('router.sc.');
            const isTsField = field.key.startsWith('router.tailscale.');
            const scHidden = config.router && config.router.transport !== 'sc';
            const tsHidden = config.router && config.router.transport !== 'tailscale';

            const wrapper = document.createElement('div');
            wrapper.className = 'mb-3';
            if ((isScField && scHidden) || (isTsField && tsHidden)) {
                wrapper.classList.add('hidden');
                wrapper.dataset.dependsOn = isScField ? 'transport-sc' : 'transport-tailscale';
            }

            let input;
            if (field.type === 'select') {
                const options = typeof field.options === 'function' ? field.options() : field.options;
                let opts = options.map(o =>
                    `<option value="${o.value}" ${val === o.value ? 'selected' : ''}>${o.label}</option>`
                ).join('');
                if (val != null && val !== '' && !options.some(o => o.value === val)) {
                    opts = `<option value="${val}" selected>${val}</option>` + opts;
                }
                const isTransportSelect = field.key === 'router.transport';
                const changeAttr = isTransportSelect ? 'onchange="onTransportChange(this)"' : '';
                input = `<select id="cfg-${field.key.replace(/\./g, '-')}" data-key="${field.key}"
                          ${changeAttr}>${opts}</select>`;
            } else if (field.type === 'checkbox') {
                input = `<input type="checkbox" id="cfg-${field.key.replace(/\./g, '-')}" data-key="${field.key}"
                          ${val ? 'checked' : ''}
                          class="rounded border-gray-300 text-blue-600 focus:ring-blue-500 h-5 w-5">`;
            } else {
                input = `<input type="${field.type}" id="cfg-${field.key.replace(/\./g, '-')}" data-key="${field.key}"
                          value="${val ?? ''}" ${field.placeholder ? `placeholder="${field.placeholder}"` : ''}>`;
            }

            wrapper.innerHTML = `
                <label for="cfg-${field.key.replace(/\./g, '-')}">${field.label}</label>
                ${input}
            `;
            div.appendChild(wrapper);
        });

        form.appendChild(div);
    });
}

function onTransportChange(select) {
    const transport = select.value;
    document.querySelectorAll('[data-depends-on]').forEach(el => {
        if (el.dataset.dependsOn === 'transport-sc') {
            el.classList.toggle('hidden', transport !== 'sc');
        } else if (el.dataset.dependsOn === 'transport-tailscale') {
            el.classList.toggle('hidden', transport !== 'tailscale');
        }
    });
}

function getNestedValue(obj, path) {
    return path.split('.').reduce((o, k) => (o && o[k] !== undefined ? o[k] : undefined), obj);
}

function setNestedValue(obj, path, val) {
    const keys = path.split('.');
    let o = obj;
    for (let i = 0; i < keys.length - 1; i++) {
        if (o[keys[i]] == null) o[keys[i]] = {};
        o = o[keys[i]];
    }
    if (typeof val === 'string') {
        if (val === 'true') val = true;
        else if (val === 'false') val = false;
        else if (val === '' && typeof o[keys[keys.length - 1]] === 'string') val = '';
        else if (val !== '' && !isNaN(Number(val)) && !val.includes('.')) val = Number(val);
    }
    o[keys[keys.length - 1]] = val;
}

async function saveConfig() {
    if (!configData) {
        showToast('Config not loaded yet. Please wait.', 'error');
        return;
    }

    const btn = document.getElementById('btn-save-config');
    btn.disabled = true;
    btn.textContent = 'Saving...';

    try {
        const config = JSON.parse(JSON.stringify(configData));
        document.querySelectorAll('[data-key]').forEach(el => {
            const key = el.dataset.key;
            const val = el.type === 'checkbox' ? el.checked : el.value;
            setNestedValue(config, key, val);
        });

        // Client-side validation
        clearFieldErrors();
        const errors = [];

        const transport = document.getElementById('cfg-router-transport')?.value;
        if (transport === 'sc') {
            const hubUrl = document.getElementById('cfg-router-sc-hub-url')?.value;
            if (!hubUrl || !hubUrl.trim()) {
                errors.push({ el: document.getElementById('cfg-router-sc-hub-url'), msg: 'Hub URL required for BACnet/SC' });
            }
        } else if (transport === 'tailscale') {
            const tsIface = document.getElementById('cfg-router-tailscale-interface')?.value;
            if (!tsIface || !tsIface.trim()) {
                errors.push({ el: document.getElementById('cfg-router-tailscale-interface'), msg: 'Interface required for Tailscale' });
            }
        }

        const portFields = [
            { id: 'cfg-router-lan-port', label: 'LAN Port' },
            { id: 'cfg-router-tailscale-port', label: 'Tailscale Port' },
            { id: 'cfg-web-port', label: 'Web Port' },
        ];
        portFields.forEach(f => {
            const el = document.getElementById(f.id);
            if (el && (el.value === '' || Number(el.value) <= 0)) {
                errors.push({ el, msg: `${f.label} must be > 0` });
            }
        });

        const devName = document.getElementById('cfg-router-device-name');
        if (devName && !devName.value.trim()) {
            errors.push({ el: devName, msg: 'Device Name must not be empty' });
        }

        if (errors.length > 0) {
            errors.forEach(e => {
                e.el.classList.add('border-red-500');
                const errDiv = document.createElement('div');
                errDiv.className = 'field-error text-red-500 text-xs mt-1';
                errDiv.textContent = e.msg;
                e.el.parentNode.appendChild(errDiv);
            });
            showToast('Please fix validation errors', 'error');
            btn.disabled = false;
            btn.textContent = 'Save Config';
            return;
        }

        const resp = await fetch('/api/config', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(config),
        });

        if (resp.ok) {
            showToast('Configuration saved', 'success');
            configData = config;
        } else {
            const err = await resp.json();
            showToast(err.error || 'Failed to save config', 'error');
        }
    } catch (e) {
        logToServer('ERROR', 'Network error saving config: ' + (e.message || e));
        showToast('Network error saving config', 'error');
    } finally {
        btn.disabled = false;
        btn.textContent = 'Save Config';
    }
}

async function switchTransport(mode) {
    const btn = mode === 'sc' ? document.getElementById('btn-switch-sc') : document.getElementById('btn-switch-tailscale');
    btn.disabled = true;

    try {
        const resp = await fetch('/api/transport/switch', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ mode }),
        });

        if (resp.ok) {
            showToast(`Switched to ${mode}`, 'success');
            updateStatus();
        } else if (resp.status === 409) {
            const err = await resp.json();
            showToast(err.error || 'Cannot switch while router is running', 'error');
        } else {
            const err = await resp.json();
            showToast(err.error || 'Switch failed', 'error');
        }
    } catch (e) {
        logToServer('ERROR', 'Network error switching transport: ' + (e.message || e));
        showToast('Network error', 'error');
    } finally {
        btn.disabled = false;
    }
}

async function stopRouter() {
    try {
        const resp = await fetch('/api/transport/stop', { method: 'POST' });
        if (resp.ok) {
            showToast('Router stopping...', 'success');
            setTimeout(updateStatus, 1000);
        } else {
            const err = await resp.json();
            showToast(err.error || 'Stop failed', 'error');
        }
    } catch (e) {
        logToServer('ERROR', 'Network error stopping router: ' + (e.message || e));
        showToast('Network error', 'error');
    }
}

async function startRouter() {
    try {
        const resp = await fetch('/api/transport/start', { method: 'POST' });
        if (resp.ok) {
            showToast('Router starting...', 'success');
            setTimeout(updateStatus, 1000);
        } else {
            const err = await resp.json();
            showToast(err.error || 'Start failed', 'error');
        }
    } catch (e) {
        logToServer('ERROR', 'Network error starting router: ' + (e.message || e));
        showToast('Network error', 'error');
    }
}

function formatFdtTable(entries) {
    const tbody = document.getElementById('fdt-tbody');
    if (!entries || entries.length === 0) {
        tbody.innerHTML = '<tr><td colspan="3" class="py-4 text-gray-400">No foreign devices registered</td></tr>';
        return;
    }
    tbody.innerHTML = entries.map(e => `
        <tr class="border-b border-gray-100 hover:bg-gray-50">
            <td class="py-2 font-mono">${e.ip}:${e.port}</td>
            <td class="py-2">${e.ttl}</td>
            <td class="py-2 ${e.remaining_ttl > 0 ? '' : 'text-red-500 font-medium'}">${e.remaining_ttl > 0 ? e.remaining_ttl : 'expired'}</td>
        </tr>
    `).join('');
}

let logEntries = [];
let logPaused = false;
let logBuffer = [];
let autoScroll = true;
let wsConnected = false;
let renderedIds = new Set();

function setupWebSocket() {
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${location.host}/ws/logs`;
    let ws = new WebSocket(wsUrl);
    wsConnected = false;

    ws.onopen = () => {
        wsConnected = true;
    };

    ws.onmessage = (event) => {
        try {
            const entries = JSON.parse(event.data);
            if (logPaused) {
                logBuffer.push(...entries);
                if (logBuffer.length > 500) {
                    logBuffer = logBuffer.slice(-500);
                }
            } else {
                entries.forEach(e => {
                    logEntries.push(e);
                    if (logEntries.length > 500) {
                        logEntries = logEntries.slice(-500);
                    }
                });
                renderLogs();
            }
        } catch (e) {
            // ignore parse errors
        }
    };

    ws.onclose = () => {
        wsConnected = false;
        setTimeout(setupWebSocket, 3000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function highlightText(text, search) {
    const escaped = escapeHtml(text);
    const regex = new RegExp('(' + search.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
    return escaped.replace(regex, '<mark class="bg-yellow-300 text-gray-900 rounded px-0.5">$1</mark>');
}

function togglePause() {
    logPaused = !logPaused;
    const btn = document.getElementById('btn-pause');
    btn.textContent = logPaused ? 'Resume' : 'Pause';
    if (!logPaused && logBuffer.length > 0) {
        logBuffer.forEach(e => {
            logEntries.push(e);
            if (logEntries.length > 500) {
                logEntries = logEntries.slice(-500);
            }
        });
        logBuffer = [];
        renderLogs();
    }
}

function clearLogs() {
    logEntries = [];
    logBuffer = [];
    logPaused = false;
    renderedIds = new Set();
    document.getElementById('btn-pause').textContent = 'Pause';
    document.getElementById('log-viewer').innerHTML = '';
    document.getElementById('log-viewer').innerHTML = '<div class="text-gray-500">No log entries match filters</div>';
}

function toggleAutoScroll() {
    autoScroll = !autoScroll;
    const indicator = document.getElementById('auto-scroll-indicator');
    indicator.textContent = autoScroll ? '[Auto-scroll ON]' : '[Auto-scroll OFF]';
}

function renderLogs() {
    const viewer = document.getElementById('log-viewer');
    const levelFilter = document.getElementById('log-level-filter').value;
    const searchText = document.getElementById('log-search').value.toLowerCase();

    const levelOrder = { 'TRACE': 0, 'DEBUG': 1, 'INFO': 2, 'WARN': 3, 'ERROR': 4 };
    const minLevel = levelFilter ? (levelOrder[levelFilter] || 0) : 0;

    const filtered = logEntries.filter(e => {
        const lv = levelOrder[e.level] !== undefined ? levelOrder[e.level] : 0;
        return lv >= minLevel && (!searchText || e.message.toLowerCase().includes(searchText) || e.target.toLowerCase().includes(searchText));
    });

    const last50 = filtered.slice(-50);

    if (last50.length === 0) {
        if (viewer.children.length !== 1 || !viewer.firstChild.textContent.includes('No log entries')) {
            viewer.innerHTML = '<div class="text-gray-500">No log entries match filters</div>';
            renderedIds = new Set();
        }
        return;
    }

    const newIds = new Set(last50.map(e => e.id));

    if (newIds.size !== renderedIds.size || ![...newIds].every(id => renderedIds.has(id))) {
        // Remove placeholder if present
        const placeholder = viewer.querySelector('.text-gray-500');
        if (placeholder) placeholder.remove();

        const children = viewer.querySelectorAll('[data-log-id]');
        children.forEach(child => {
            const id = parseInt(child.dataset.logId);
            if (!newIds.has(id)) {
                child.remove();
            }
        });

        const existingIds = new Set([...viewer.querySelectorAll('[data-log-id]')].map(c => parseInt(c.dataset.logId)));
        const toAdd = last50.filter(e => !existingIds.has(e.id));

        if (toAdd.length > 0) {
            const html = toAdd.map(e => {
                const colorClass = e.level === 'ERROR' ? 'text-red-400' :
                                   e.level === 'WARN' ? 'text-yellow-400' :
                                   e.level === 'INFO' ? 'text-gray-100' :
                                   'text-gray-400';
                const ts = escapeHtml(e.timestamp || '');
                const level = escapeHtml(e.level || '');
                const target = searchText ? highlightText(e.target || '', searchText) : escapeHtml(e.target || '');
                const message = searchText ? highlightText(e.message || '', searchText) : escapeHtml(e.message || '');
                return `<div class="${colorClass}" data-log-id="${e.id}">[${ts}] [${level}] [${target}] ${message}</div>`;
            }).join('');
            viewer.insertAdjacentHTML('beforeend', html);
        }

        const MAX_VISIBLE = 100;
        const logDivs = viewer.querySelectorAll('[data-log-id]');
        while (logDivs.length > MAX_VISIBLE) {
            logDivs[0].remove();
        }

        renderedIds = newIds;

        if (autoScroll) {
            viewer.scrollTop = viewer.scrollHeight;
        }
    }
}

function filterLogs() {
    renderedIds = new Set();
    renderLogs();
}

async function downloadLogs() {
    try {
        const resp = await fetch('/api/logs');
        if (!resp.ok) {
            showToast('Failed to fetch logs', 'error');
            return;
        }
        const entries = await resp.json();
        const text = entries.map(e => `[${e.timestamp}] [${e.level}] [${e.target}] ${e.message}`).join('\n');
        const blob = new Blob([text], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'bacnet-bridge-logs.txt';
        a.click();
        URL.revokeObjectURL(url);
    } catch (e) {
        logToServer('ERROR', 'Network error downloading logs: ' + (e.message || e));
        showToast('Network error downloading logs', 'error');
    }
}

function copyLogs() {
    const levelFilter = document.getElementById('log-level-filter').value;
    const searchText = document.getElementById('log-search').value.toLowerCase();
    const levelOrder = { 'TRACE': 0, 'DEBUG': 1, 'INFO': 2, 'WARN': 3, 'ERROR': 4 };
    const minLevel = levelFilter ? (levelOrder[levelFilter] || 0) : 0;

    const filtered = logEntries.filter(e => {
        const lv = levelOrder[e.level] !== undefined ? levelOrder[e.level] : 0;
        return lv >= minLevel && (!searchText || e.message.toLowerCase().includes(searchText) || e.target.toLowerCase().includes(searchText));
    });

    if (filtered.length === 0) {
        showToast('No log content to copy', 'error');
        return;
    }

    const text = filtered.map(e => `[${e.timestamp}] [${e.level}] [${e.target}] ${e.message}`).join('\n');
    navigator.clipboard.writeText(text).then(() => {
        showToast('Log copied to clipboard', 'success');
    }).catch(() => {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
        showToast('Log copied to clipboard', 'success');
    });
}

async function updateRouterInfo() {
    try {
        const resp = await fetch('/api/router-info');
        if (!resp.ok) return;
        const data = await resp.json();

        document.getElementById('ri-device-id').textContent = data.device_id ?? '--';
        document.getElementById('ri-vendor-id').textContent = data.vendor_id ?? '--';

        const tbody = document.getElementById('ri-networks-tbody');
        if (!data.networks || data.networks.length === 0) {
            tbody.innerHTML = '<tr><td colspan="4" class="py-4 text-gray-400">No networks configured</td></tr>';
            return;
        }
        tbody.innerHTML = data.networks.map(n => {
            const addr = n.type === 'BACnet/SC' && n.hub_url ? n.hub_url : n.ip || '--';
            const port = n.type === 'BACnet/SC' ? '' : n.port || '--';
            return `<tr class="border-b border-gray-100 hover:bg-gray-50">
                <td class="py-2">Network ${n.network}</td>
                <td class="py-2">${n.type}</td>
                <td class="py-2 font-mono text-sm">${addr}</td>
                <td class="py-2 font-mono">${port}</td>
            </tr>`;
        }).join('');
    } catch (e) {
        // silent
    }
}

async function updateFdt() {
    try {
        const resp = await fetch('/api/status');
        if (!resp.ok) return;
        const data = await resp.json();

        const fdtCard = document.getElementById('fdt-card');
        if (data.transport === 'tailscale') {
            fdtCard.style.display = 'block';
            const fdtResp = await fetch('/api/fdt');
            if (fdtResp.ok) {
                const entries = await fdtResp.json();
                formatFdtTable(entries);
            }
        } else {
            fdtCard.style.display = 'none';
        }
    } catch (e) {
        // silent
    }
}

document.addEventListener('DOMContentLoaded', () => {
    updateStatus();
    updateRouterInfo();
    updateHubStatus();
    loadInterfaces();
    loadConfig();
    setupWebSocket();
    setInterval(updateStatus, 5000);
    setInterval(updateRouterInfo, 5000);
    setInterval(updateHubStatus, 5000);
    setInterval(updateFdt, 2000);
});
