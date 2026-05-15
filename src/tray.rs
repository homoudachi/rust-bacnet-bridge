#![cfg(feature = "windows-tray")]

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};
use tray_item::{IconSource, TrayItem};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResource, DestroyIcon, HICON};

use crate::web::RouterCommand;
use bridge_core::AppState;

fn make_dib_data(r: u8, g: u8, b: u8) -> Vec<u8> {
    let width: u32 = 32;
    let height: u32 = 32;
    let xor_row = width * 4;
    let xor_size = xor_row * height;
    let and_row = ((width + 31) / 32) * 4;
    let and_size = and_row * height;

    let mut data = Vec::with_capacity(40 + (xor_size + and_size) as usize);

    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&width.to_le_bytes());
    data.extend_from_slice(&(height * 2).to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    for _ in 0..height {
        for _ in 0..width {
            data.push(b);
            data.push(g);
            data.push(r);
            data.push(255);
        }
    }

    for _ in 0..and_size {
        data.push(0);
    }

    data
}

unsafe fn make_hicon(r: u8, g: u8, b: u8) -> HICON {
    let data = make_dib_data(r, g, b);
    CreateIconFromResource(data.as_ptr(), data.len() as u32, 1, 0x00030000)
}

struct TrayIcons {
    green: HICON,
    amber: HICON,
    red: HICON,
}

impl TrayIcons {
    fn new() -> Self {
        unsafe {
            Self {
                green: make_hicon(34, 197, 94),
                amber: make_hicon(234, 179, 8),
                red: make_hicon(239, 68, 68),
            }
        }
    }

    fn for_state(&self, state: AppState) -> HICON {
        match state {
            AppState::Running => self.green,
            AppState::Starting | AppState::Stopping => self.amber,
            AppState::Stopped => self.red,
        }
    }
}

impl Drop for TrayIcons {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.green);
            DestroyIcon(self.amber);
            DestroyIcon(self.red);
        }
    }
}

pub fn run_tray(
    state_rx: watch::Receiver<AppState>,
    command_tx: mpsc::Sender<RouterCommand>,
    web_url: String,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut tray =
        TrayItem::new("BACnet Bridge", IconSource::Resource("")).expect("create tray item");

    let icons = Arc::new(TrayIcons::new());

    tray.set_icon(IconSource::RawIcon(icons.red as isize)).ok();
    tray.inner_mut().set_tooltip("BACnet Bridge - Stopped").ok();

    let tray = Arc::new(Mutex::new(tray));

    {
        let mut t = tray.lock().unwrap();
        let url = web_url.clone();
        t.add_menu_item("Open Dashboard", move || {
            let _ = webbrowser::open(&url);
        })
        .ok();
    }

    {
        let mut t = tray.lock().unwrap();
        t.inner_mut().add_separator().ok();
    }

    {
        let mut t = tray.lock().unwrap();
        let tx = command_tx.clone();
        t.add_menu_item("Stop Router", move || {
            let _ = tx.try_send(RouterCommand::Stop);
        })
        .ok();
    }

    {
        let mut t = tray.lock().unwrap();
        let tx = command_tx.clone();
        t.add_menu_item("Start Router", move || {
            let _ = tx.try_send(RouterCommand::Start);
        })
        .ok();
    }

    {
        let mut t = tray.lock().unwrap();
        t.inner_mut().add_separator().ok();
    }

    {
        let mut t = tray.lock().unwrap();
        let tx = command_tx.clone();
        t.add_menu_item("Switch to BACnet/SC", move || {
            let _ = tx.try_send(RouterCommand::SwitchTransport("sc".into()));
        })
        .ok();
    }

    {
        let mut t = tray.lock().unwrap();
        let tx = command_tx.clone();
        t.add_menu_item("Switch to Tailscale", move || {
            let _ = tx.try_send(RouterCommand::SwitchTransport("tailscale".into()));
        })
        .ok();
    }

    {
        let mut t = tray.lock().unwrap();
        t.inner_mut().add_separator().ok();
    }

    {
        let mut t = tray.lock().unwrap();
        let tx = command_tx.clone();
        t.add_menu_item("Exit", move || {
            let _ = tx.try_send(RouterCommand::Exit);
        })
        .ok();
    }

    let tray_watch = tray.clone();
    let icons_watch = icons.clone();
    let mut state = state_rx;
    loop {
        match shutdown_rx.try_recv() {
            Ok(_) | Err(oneshot::error::TryRecvError::Closed) => break,
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        if state.has_changed().unwrap_or(false) {
            let app_state = *state.borrow_and_update();
            let hicon = icons_watch.for_state(app_state);
            let label = match app_state {
                AppState::Running => "BACnet Bridge - Running",
                AppState::Starting | AppState::Stopping => "BACnet Bridge - Reconnecting",
                AppState::Stopped => "BACnet Bridge - Stopped",
            };
            if let Ok(mut guard) = tray_watch.lock() {
                guard.set_icon(IconSource::RawIcon(hicon as isize)).ok();
                guard.inner_mut().set_tooltip(label).ok();
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}
