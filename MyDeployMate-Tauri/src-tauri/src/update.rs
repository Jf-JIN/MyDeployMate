//! App updates from GitHub Releases (SRS NFR-04; IXD-39; UI-34; ADR-11). The updater plugin
//! reads `latest.json` from the newest release and verifies the downloaded installer against
//! the public key in `tauri.conf.json`; the private key is a GitHub secret, never in the repo.
//! Nothing installs while a run is active, and the configuration is backed up first.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use mdm_core::Message;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

/// The update found by the last check, and the token of a download in progress.
#[derive(Default)]
pub struct UpdateSlot {
    found: Mutex<Option<Update>>,
    download: Mutex<Option<CancellationToken>>,
}

/// A newer release than the running one.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release notes as written on the release; shown as plain text.
    pub notes: Option<String>,
}

/// Download progress sent while `install_update` runs.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateProgress {
    #[ts(type = "number")]
    pub downloaded: u64,
    #[ts(type = "number | null")]
    pub total: Option<u64>,
}

/// How `install_update` ended when the app did not exit for the installer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateOutcome {
    Cancelled,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn failed(message: Message) -> AppError {
    AppError::new(AppErrorKind::Update, message)
}

/// Ask the release endpoint for a newer version (IXD-39). `None` when this is the newest.
/// The UI calls it on start (if `Preferences.check_updates`) and from the settings page; a
/// failure on start is only shown there, never as a dialog.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, AppError> {
    let check = |e: tauri_plugin_updater::Error| {
        failed(Message::UpdateCheckFailed {
            detail: e.to_string(),
        })
    };
    let update = app.updater().map_err(check)?.check().await.map_err(check)?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
    });
    *lock(&app.state::<AppState>().update.found) = update;
    Ok(info)
}

/// Download, verify and install the update found by the last check. On Windows the app exits
/// once the installer starts, so a successful call does not return. Refused while a run is
/// active (NFR-04); the configuration is backed up just before installing (NFR-03).
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    on_progress: Channel<UpdateProgress>,
) -> Result<UpdateOutcome, AppError> {
    let state = app.state::<AppState>();
    ensure_idle(&state)?;
    let update = lock(&state.update.found)
        .clone()
        .ok_or_else(|| failed(Message::UpdateNotAvailable))?;
    let cancel = CancellationToken::new();
    *lock(&state.update.download) = Some(cancel.clone());

    let mut downloaded = 0u64;
    let mut reported = 0u64;
    let download = update.download(
        |chunk, total| {
            downloaded += chunk as u64;
            // A message per network chunk would flood the channel; every 256 KiB is enough.
            if downloaded - reported >= 256 * 1024 || Some(downloaded) == total {
                reported = downloaded;
                let _ = on_progress.send(UpdateProgress { downloaded, total });
            }
        },
        || {},
    );
    let bytes = tokio::select! {
        bytes = download => bytes,
        () = cancel.cancelled() => {
            *lock(&state.update.download) = None;
            return Ok(UpdateOutcome::Cancelled);
        }
    };
    *lock(&state.update.download) = None;
    let bytes = bytes.map_err(|e| match e {
        tauri_plugin_updater::Error::Minisign(_)
        | tauri_plugin_updater::Error::Base64(_)
        | tauri_plugin_updater::Error::SignatureUtf8(_)
        | tauri_plugin_updater::Error::SignedVersionMismatch { .. }
        | tauri_plugin_updater::Error::MissingSignedVersion => {
            failed(Message::UpdateSignatureInvalid)
        }
        e => failed(Message::UpdateDownloadFailed {
            detail: e.to_string(),
        }),
    })?;

    // A run may have started while downloading.
    ensure_idle(&state)?;
    state.store()?.backup_before_update()?;
    update.install(bytes).map_err(|e| {
        failed(Message::UpdateInstallFailed {
            detail: e.to_string(),
        })
    })?;
    // Not reached on Windows: the installer has taken over.
    app.restart()
}

/// Stop a download started by `install_update`; nothing has been installed yet.
#[tauri::command]
pub fn cancel_update(state: State<'_, AppState>) {
    if let Some(cancel) = lock(&state.update.download).take() {
        cancel.cancel();
    }
}

fn ensure_idle(state: &AppState) -> Result<(), AppError> {
    if lock(&state.active).is_some() {
        return Err(AppError::new(
            AppErrorKind::RunActive,
            Message::UpdateRunActive,
        ));
    }
    Ok(())
}
