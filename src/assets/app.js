let configData = null;

function showToast(msg, type) {
    const t = document.getElementById('toast');
    t.textContent = msg;
    t.className = 'fixed bottom-4 right-4 px-4 py-3 rounded-lg shadow-lg text-white text-sm font-medium translate-y-2 opacity-0 transition-all duration-300 pointer-events-none';
    if (type === 'error') t.classList.add('bg-red-600');
    else if (type === 'success') t.classList.add('bg-green-600');
    else t.classList.add('bg-gray-800');
    requestAnimationFrame(() => {
        t.classList.add('show');
        setTimeout(() => t.classList.remove('show'), 3000);
    });
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
        case 'Stopping': dot.classList.add('bg-yellow-500'); break;
        default: dot.classList.add('bg-red-500');
    }
}

function updateTransportButtons(state, transport) {
    const btnSc = document.getElementById('btn-switch-sc');
    const btnTs = document.getElementById('btn-switch-tailscale');
    const btnStop = document.getElementById('btn-stop');
    const btnStart = document.getElementById('btn-start');

    const isRunning = state === 'Running';
    const isStopped = state === 'Stopped';

    btnStop.disabled = !isRunning;
    btnStart.disabled = isRunning;

    if (isStopped) {
        btnSc.disabled = transport === 'sc';
        btnTs.disabled = transport === 'tailscale';
    } else {
        btnSc.disabled = true;
        btnTs.disabled = true;
    }
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

        updateStateIndicator(data.state);
        updateTransportButtons(data.state, data.transport);
    } catch (e) {
        // silent
    }
}

async function loadInterfaces() {
    try {
        const resp = await fetch('/api/interfaces');
        if (!resp.ok) return;
        const data = await resp.json();
        const list = document.getElementById('interfaces-list');
        if (!data.interfaces || data.interfaces.length === 0) {
            list.innerHTML = '<p class="text-sm text-gray-400">No interfaces configured</p>';
            return;
        }
        list.innerHTML = data.interfaces.map(iface => `
            <div class="flex items-center justify-between p-2 bg-gray-50 rounded-lg">
                <div class="flex items-center gap-2">
                    <span class="text-sm font-medium text-gray-700">${iface.name}</span>
                    ${iface.is_tailscale
                        ? '<span class="px-1.5 py-0.5 text-xs rounded bg-green-100 text-green-700 font-medium">TS</span>'
                        : '<span class="px-1.5 py-0.5 text-xs rounded bg-blue-100 text-blue-700 font-medium">LAN</span>'}
                </div>
                <span class="text-sm font-mono text-gray-600">${iface.ip}</span>
            </div>
        `).join('');
    } catch (e) {
        // silent
    }
}

async function loadConfig() {
    try {
        const resp = await fetch('/api/config');
        if (!resp.ok) return;
        configData = await resp.json();
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
                { key: 'router.lan.interface', label: 'Interface IP', type: 'text', placeholder: 'e.g., 192.168.1.100' },
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
                { key: 'router.tailscale.interface', label: 'Interface IP', type: 'text', placeholder: 'e.g., 100.64.0.1' },
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
                const opts = field.options.map(o =>
                    `<option value="${o.value}" ${val === o.value ? 'selected' : ''}>${o.label}</option>`
                ).join('');
                input = `<select id="cfg-${field.key.replace(/\./g, '-')}" data-key="${field.key}"
                          onchange="onTransportChange(this)">${opts}</select>`;
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
        if (!o[keys[i]]) o[keys[i]] = {};
        o = o[keys[i]];
    }
    if (val === 'true') val = true;
    else if (val === 'false') val = false;
    else if (val === '' && typeof o[keys[keys.length - 1]] === 'string') val = '';
    else if (val !== '' && !isNaN(Number(val)) && val.includes('.') === false) val = Number(val);
    o[keys[keys.length - 1]] = val;
}

async function saveConfig() {
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
let wsConnected = false;

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
            entries.forEach(e => {
                logEntries.push(e);
                if (logEntries.length > 500) {
                    logEntries = logEntries.slice(-500);
                }
            });
            renderLogs();
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
        viewer.innerHTML = '<div class="text-gray-500">No log entries match filters</div>';
        return;
    }

    viewer.innerHTML = last50.map(e => {
        const colorClass = e.level === 'ERROR' ? 'text-red-400' :
                           e.level === 'WARN' ? 'text-yellow-400' :
                           e.level === 'INFO' ? 'text-gray-100' :
                           'text-gray-400';
        return `<div class="${colorClass}">[${e.timestamp}] [${e.level}] [${e.target}] ${e.message}</div>`;
    }).join('');

    viewer.scrollTop = viewer.scrollHeight;
}

function filterLogs() {
    renderLogs();
}

async function downloadLogs() {
    try {
        const resp = await fetch('/api/logs');
        if (!resp.ok) return;
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
    loadInterfaces();
    loadConfig();
    setupWebSocket();
    setInterval(updateStatus, 5000);
    setInterval(updateFdt, 2000);
});
