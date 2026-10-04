//! The live device list: watches for devices being connected or disconnected, re-lists them after
//! things go quiet, and tells the window when the list changed.
//!
//! Runs entirely in the app process and registers for no input (ADR 0012), so it works at idle and
//! doesn't depend on the engine. It sends the window device names and ids (the list is the point);
//! the logs only get counts.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use keyclean_win::{DeviceWatch, input_devices};
use tauri::{AppHandle, Emitter, Manager};

use crate::bridge::{DEVICES_EVENT, DevicesDto, ErrorDto};

/// How often, and how long after the first listing, a device without a name is read again.
const NAME_RETRIES: usize = 2;
const NAME_RETRY_DELAY: Duration = Duration::from_secs(1);

/// How long the notifications must stay quiet before the list is read again. One connection fires
/// several (one per interface), and names can lag behind the first.
const QUIET: Duration = Duration::from_millis(400);

/// The latest list, for windows that ask instead of listening.
pub struct DeviceStore(Mutex<DevicesDto>);

/// The running watch. Taken out and dropped on exit, which unregisters it and ends the refresh
/// thread.
pub struct WatchSlot(Mutex<Option<DeviceWatch>>);

impl DeviceStore {
    pub fn snapshot(&self) -> DevicesDto {
        lock(&self.0).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts watching, reads the first list and starts the refresh thread. Failures show up in the
/// list itself (as its `error`); the app keeps running.
pub fn start(app: &AppHandle) {
    let (changed_tx, changed_rx) = mpsc::channel();
    // Watching starts before the first listing, so a device connected in between isn't missed.
    let (watch, watch_error) = match DeviceWatch::start(changed_tx) {
        Ok(watch) => (Some(watch), None),
        Err(e) => {
            eprintln!("[keyclean] device watch failed: {}", e.details());
            (None, Some(ErrorDto::from(&e)))
        }
    };

    let first = read(&DevicesDto::default(), watch_error.as_ref());
    eprintln!("[keyclean] devices: {} listed", first.devices.len());
    app.manage(DeviceStore(Mutex::new(first)));
    app.manage(WatchSlot(Mutex::new(watch)));

    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("keyclean-devices".into())
        .spawn(move || refresh_loop(&handle, &changed_rx, watch_error));
    if let Err(e) = spawned {
        eprintln!("[keyclean] could not start the device refresh thread: {e}");
    }
}

/// Stops watching. The refresh thread ends when the watch (which owns the sender) is gone.
pub fn stop(app: &AppHandle) {
    if let Some(slot) = app.try_state::<WatchSlot>() {
        let watch = lock(&slot.0).take();
        drop(watch); // Outside the lock; unregistering waits for a running callback.
    }
}

fn refresh_loop(app: &AppHandle, changed: &Receiver<()>, watch_error: Option<ErrorDto>) {
    // Blocks until the first notification: no polling, no work while nothing changes.
    while changed.recv().is_ok() {
        if !wait_until_quiet(changed, QUIET) {
            break; // The watch is gone (the app is exiting); nothing left to show.
        }
        let mut unnamed = refresh(app, watch_error.as_ref());
        // A name can lag behind the arrival: read again, a few times at most, while one is missing.
        for _ in 0..NAME_RETRIES {
            if !unnamed {
                break;
            }
            match changed.recv_timeout(NAME_RETRY_DELAY) {
                Err(RecvTimeoutError::Disconnected) => return,
                Ok(()) if !wait_until_quiet(changed, QUIET) => return,
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            }
            unnamed = refresh(app, watch_error.as_ref());
        }
    }
}

/// Swallows notifications until none arrives for `quiet`. Returns false if the sender is gone.
fn wait_until_quiet(changed: &Receiver<()>, quiet: Duration) -> bool {
    loop {
        match changed.recv_timeout(quiet) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => return true,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

/// Lists devices again and, if the result differs, stores it and emits it to the window. Returns
/// whether any listed device has no name yet. Only this thread writes the store, so listing happens
/// outside its lock and `list_devices` never waits for it.
fn refresh(app: &AppHandle, watch_error: Option<&ErrorDto>) -> bool {
    let Some(store) = app.try_state::<DeviceStore>() else {
        return false;
    };
    let previous = store.snapshot();
    let next = read(&previous, watch_error);
    let unnamed = next.devices.iter().any(|d| d.name.is_none());
    if next != previous {
        *lock(&store.0) = next.clone();
        eprintln!("[keyclean] devices: {} listed", next.devices.len());
        let _ = app.emit_to(crate::MAIN_WINDOW, DEVICES_EVENT, next);
    }
    unnamed
}

/// Lists devices. On failure the previous list is kept and the error is shown with it.
fn read(previous: &DevicesDto, watch_error: Option<&ErrorDto>) -> DevicesDto {
    match input_devices() {
        Ok(list) => DevicesDto {
            devices: list.into_iter().map(Into::into).collect(),
            error: watch_error.cloned(),
        },
        Err(e) => {
            eprintln!("[keyclean] device listing failed: {}", e.details());
            DevicesDto {
                devices: previous.devices.clone(),
                error: Some(ErrorDto::from(&e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_notifications_is_one_wait() {
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 {
            tx.send(()).unwrap();
        }
        assert!(wait_until_quiet(&rx, Duration::from_millis(20)));
        assert!(rx.try_recv().is_err(), "the burst was swallowed");
    }

    #[test]
    fn a_closed_channel_is_reported() {
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        assert!(!wait_until_quiet(&rx, Duration::from_millis(20)));
    }

    #[test]
    fn notifications_during_the_wait_extend_it() {
        let (tx, rx) = mpsc::channel();
        let keep_open = tx.clone();
        let sender = std::thread::spawn(move || {
            for _ in 0..4 {
                std::thread::sleep(Duration::from_millis(5));
                tx.send(()).unwrap();
            }
        });
        // Each gap (5 ms) is far shorter than the quiet period (200 ms), so one wait covers them
        // all. `keep_open` keeps the channel from reporting a disconnect.
        assert!(wait_until_quiet(&rx, Duration::from_millis(200)));
        sender.join().unwrap();
        assert!(rx.try_recv().is_err());
        drop(keep_open);
    }
}
