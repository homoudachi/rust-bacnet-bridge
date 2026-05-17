# Handover — `ui-testing-pass` branch

**Date:** 2026-05-18
**Branch:** `ui-testing-pass` (from `master`)
**Status:** Exe launches, dashboard works, tray works — 4 open issues remain

## What works

- Double-click `bacnet-bridge.exe` → tray icon appears (red=Stopped) + browser opens `http://127.0.0.1:28821`
- Web dashboard loads, status card shows state/transport/uptime
- Network interfaces detected and shown as 3-column table
- Config form renders all sections, transport switch toggles SC/Tailscale fields
- Save config persists to `config.toml`
- Start/Stop/Switch transport buttons gated by state machine
- Run from terminal: `cargo build --release --features windows-tray` then test
- If exe panics: check `bacnet-bridge-panic.log` next to the exe
- **95 tests passing**, clippy clean

## Open issues on `ui-testing-pass`

| # | Title | Priority |
|---|-------|----------|
| 99 | ANSI escape codes visible in log viewer | High |
| 100 | Interface dropdown value not persisting to config (Tailscale IP stays empty) | High |
| 101 | Log viewer text deselects on new entries, copy unreliable | Medium |
| 102 | Log entries duplicated in viewer | Medium |

## Architecture notes for next session

- `src/router_cmd.rs` — app startup, event loop, command handling
- `src/web/api.rs` — all REST handlers, WS log streaming, config update
- `src/assets/app.js` — frontend: polling, config form, log viewer, transport buttons
- `crates/bridge-core/src/logbuf.rs` — `LogRingBuffer` + `LogBufWriter` (tracing → buffer)
- `crates/bridge-core/src/state.rs` — `AppState` state machine (valid transitions)
- `crates/bridge-core/src/router.rs` — `start_router()`, LAN + remote transport setup
- `src/tray.rs` — Windows tray icon + right-click menu
- `src/main.rs` — CLI dispatch, `windows_subsystem = "windows"`, panic hook

## Quick test script (PowerShell)

```powershell
$p = Start-Process -FilePath "target/release/bacnet-bridge.exe" -PassThru
Start-Sleep 3
try {
    $r = Invoke-WebRequest "http://localhost:28821/api/status" -UseBasicParsing -TimeoutSec 3
    $body = $r.Content | ConvertFrom-Json
    Write-Host "State: $($body.state), Transport: $($body.transport)"
} catch { Write-Host "FAILED: $_" }
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
```

## Key fixes applied in this branch

- #85: Tray icon panics (RawIcon vs Resource)
- #87: Double-click defaults to router subcommand
- #88: Console window hidden (windows_subsystem)
- #89: 15s timeout on router start
- #90: Windows adapter GUIDs → IP labels
- #91: Skip auto-start on launch, auto-open browser
- #92: Interface list overhaul (IPv4, 3-col, type badges)
- #93: Hide hub card in tailscale, pulse state dot
- #94: Live log streaming + config save
- #95: Illegal Stopped→Stopped transition + panic hook
- #97: Browser URL 127.0.0.1, clear logs, Starting→Stopped transition
- #98: Interface IP dropdowns, CIDR strip, copy button

## Build command

```powershell
cargo build --release --features windows-tray
```

Binary: `target\release\bacnet-bridge.exe`
