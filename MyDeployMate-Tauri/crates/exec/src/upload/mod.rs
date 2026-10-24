//! SFTP upload (SRS STEP-03/04, SEC-04; architecture ARC-28–30).
//!
//! Each file is written to an exclusively created temporary file next to its destination,
//! checked for length, and renamed onto the destination. Conflict policy "error" commits with
//! the standard SFTP rename, which refuses an existing destination; "overwrite" uses
//! `posix-rename@openssh.com` and fails on servers without it, never deleting the original
//! first. Everything runs with the login account's permissions: uploads never use sudo.
//!
//! A directory upload is not a transaction. It stops at the first failure and reports which
//! files were committed, which one failed or has an unknown result, and which were not reached.
//! Only this task's own temporary files are cleaned up.

pub mod manifest;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mdm_core::log::size_text;
use mdm_core::upload::{ancestors, join, parent, remote_path, temp_path};
use mdm_core::{
    ArtifactManifest, ConflictPolicy, ManifestEntry, Message, SourceKind, UploadPhase,
    UploadProgress,
};
use russh_sftp::client::RawSftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::ssh::SshSession;

pub use manifest::scan;

const POSIX_RENAME: &str = "posix-rename@openssh.com";
const STATVFS: &str = "statvfs@openssh.com";
/// Payload of one write request; every SFTP server accepts 32 KiB.
const CHUNK: usize = 32 * 1024;
/// Write requests sent before waiting for the first answer.
const IN_FLIGHT: usize = 16;
/// How long one SFTP request may take before the connection counts as lost.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// An SFTP session on an open SSH connection, with the extensions the server announced.
pub struct Sftp {
    raw: Arc<RawSftpSession>,
    posix_rename: bool,
    statvfs: bool,
}

impl Sftp {
    pub async fn open(session: &SshSession) -> Result<Self, Message> {
        let unavailable = |e: &dyn std::fmt::Display| Message::SftpUnavailable {
            detail: e.to_string(),
        };
        let channel = session.sftp_channel().await.map_err(|e| unavailable(&e))?;
        let raw = RawSftpSession::new(channel.into_stream());
        raw.set_timeout(REQUEST_TIMEOUT_SECS);
        let version = raw.init().await.map_err(|e| unavailable(&e))?;
        Ok(Self {
            raw: Arc::new(raw),
            posix_rename: version.extensions.contains_key(POSIX_RENAME),
            statvfs: version.extensions.contains_key(STATVFS),
        })
    }

    pub fn supports_posix_rename(&self) -> bool {
        self.posix_rename
    }

    /// Behave as a server without `posix-rename@openssh.com` (tests of AC-13).
    #[doc(hidden)]
    pub fn without_posix_rename(mut self) -> Self {
        self.posix_rename = false;
        self
    }

    pub fn close(&self) {
        let _ = self.raw.close_session();
    }
}

/// One upload task: a scanned source to one target.
pub struct UploadJob<'a> {
    /// The local path the manifest was scanned from.
    pub source: &'a Path,
    pub manifest: &'a ArtifactManifest,
    /// Absolute remote path: the file itself, or the directory receiving the contents.
    pub target: &'a str,
    pub conflict: ConflictPolicy,
    /// Unique per task; names the temporary files.
    pub token: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadEnd {
    Completed,
    Failed(Message),
    /// A rename was sent but its answer never came: the file may or may not be in place.
    Unknown(Message),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadReport {
    pub end: UploadEnd,
    /// Remote paths committed, in upload order.
    pub committed: Vec<String>,
    /// The remote path that failed, or whose commit is unknown.
    pub stopped_at: Option<String>,
    /// Remote paths not reached.
    pub unprocessed: Vec<String>,
    /// Temporary files of this task that could not be removed ([`Message::TempLeft`]).
    pub temp_left: Vec<Message>,
}

impl UploadReport {
    fn new(unprocessed: Vec<String>) -> Self {
        Self {
            end: UploadEnd::Completed,
            committed: Vec::new(),
            stopped_at: None,
            unprocessed,
            temp_left: Vec::new(),
        }
    }
}

/// Upload `job`, reporting progress after every acknowledged write.
pub async fn upload(
    sftp: &Sftp,
    job: &UploadJob<'_>,
    cancel: &CancellationToken,
    mut progress: impl FnMut(UploadProgress),
) -> UploadReport {
    let kind = job.manifest.kind;
    let files: Vec<(&ManifestEntry, String)> = job
        .manifest
        .files
        .iter()
        .map(|f| (f, remote_path(kind, job.target, &f.path)))
        .collect();
    let mut report = UploadReport::new(files.iter().map(|(_, p)| p.clone()).collect());
    if let Err(e) = prepare(sftp, job, &files).await {
        report.end = UploadEnd::Failed(e);
        return report;
    }
    let mut state = UploadProgress {
        phase: UploadPhase::Transferring,
        files_done: 0,
        files_total: u32::try_from(files.len()).unwrap_or(u32::MAX),
        bytes_done: 0,
        bytes_total: job.manifest.total_bytes(),
    };
    progress(state);
    for (entry, remote) in &files {
        if cancel.is_cancelled() {
            report.end = UploadEnd::Cancelled;
            return report;
        }
        let before = state.bytes_done;
        let put = put_file(sftp, job, entry, remote, cancel, |n| {
            state.bytes_done = before + n;
            progress(state);
        })
        .await;
        match put {
            Put::Committed => {
                report.unprocessed.remove(0);
                report.committed.push(remote.clone());
                state.files_done += 1;
                state.bytes_done = before + entry.size;
                progress(state);
            }
            Put::Failed(reason, left) => {
                report.unprocessed.remove(0);
                report.stopped_at = Some(remote.clone());
                report.temp_left = left;
                report.end = UploadEnd::Failed(reason);
                return report;
            }
            Put::Unknown(reason) => {
                report.unprocessed.remove(0);
                report.stopped_at = Some(remote.clone());
                report.end = UploadEnd::Unknown(reason);
                return report;
            }
            Put::Cancelled(left) => {
                report.temp_left = left;
                report.end = UploadEnd::Cancelled;
                return report;
            }
        }
    }
    report
}

/// Everything checked before the first byte is sent: the destinations, free space, and the
/// directories, which are created with the login account's permissions (STEP-03, ARC-30).
async fn prepare(
    sftp: &Sftp,
    job: &UploadJob<'_>,
    files: &[(&ManifestEntry, String)],
) -> Result<(), Message> {
    let kind = job.manifest.kind;
    let target = stat(sftp, job.target).await?;
    match (&target, kind) {
        (Some(a), SourceKind::File) if a.is_dir() => {
            return Err(Message::TargetIsDir {
                path: job.target.into(),
            });
        }
        (Some(a), SourceKind::Directory) if !a.is_dir() => {
            return Err(Message::TargetNotDir {
                path: job.target.into(),
            });
        }
        _ => {}
    }
    // Refuse before uploading anything rather than after part of a directory is committed.
    // The commit itself still refuses to overwrite (ARC-29).
    if target.is_some() {
        for (_, remote) in files {
            let existing = match kind {
                SourceKind::File => target.clone(),
                SourceKind::Directory => stat(sftp, remote).await?,
            };
            match existing {
                Some(a) if a.is_dir() => {
                    return Err(Message::TargetIsDir {
                        path: remote.clone(),
                    });
                }
                Some(_) if job.conflict == ConflictPolicy::Error => {
                    return Err(Message::TargetExists {
                        path: remote.clone(),
                    });
                }
                Some(_) if !sftp.posix_rename => return Err(Message::NoPosixRename),
                _ => {}
            }
        }
    }

    let chain = match kind {
        SourceKind::File => parent(job.target).map(ancestors).unwrap_or_default(),
        SourceKind::Directory => ancestors(job.target),
    };
    check_space(sftp, &chain, job.manifest.total_bytes()).await?;
    let subdirs = job.manifest.dirs.iter().map(|d| join(job.target, d));
    for dir in chain.iter().cloned().chain(subdirs) {
        match stat(sftp, &dir).await? {
            Some(a) if a.is_dir() => {}
            Some(_) => return Err(Message::TargetNotDir { path: dir }),
            None => {
                sftp.raw
                    .mkdir(dir.as_str(), FileAttributes::default())
                    .await
                    .map_err(|e| remote_error(&dir, e))?;
            }
        }
    }
    Ok(())
}

/// Fail when the file system that will hold the files has less room than they need. Skipped on
/// servers without `statvfs@openssh.com`.
async fn check_space(sftp: &Sftp, chain: &[String], need: u64) -> Result<(), Message> {
    if !sftp.statvfs || need == 0 {
        return Ok(());
    }
    let mut base = "/".to_owned();
    for dir in chain.iter().rev() {
        if let Some(a) = stat(sftp, dir).await?
            && a.is_dir()
        {
            base = dir.clone();
            break;
        }
    }
    let Ok(vfs) = sftp.raw.statvfs(base.as_str()).await else {
        return Ok(());
    };
    let avail = vfs.blocks_avail.saturating_mul(vfs.fragment_size);
    if avail < need {
        return Err(Message::NoSpace {
            path: base,
            need: size_text(need),
            avail: size_text(avail),
        });
    }
    Ok(())
}

enum Put {
    Committed,
    Failed(Message, Vec<Message>),
    Unknown(Message),
    Cancelled(Vec<Message>),
}

async fn put_file(
    sftp: &Sftp,
    job: &UploadJob<'_>,
    entry: &ManifestEntry,
    remote: &str,
    cancel: &CancellationToken,
    on_bytes: impl FnMut(u64),
) -> Put {
    let local = local_path(job.source, &entry.path);
    let mut file = match tokio::fs::File::open(&local).await {
        Ok(f) => f,
        Err(e) => {
            let path = local.to_string_lossy().into_owned();
            let reason = if e.kind() == std::io::ErrorKind::NotFound {
                Message::SourceMissing { path }
            } else {
                Message::SourceRead {
                    path,
                    detail: e.to_string(),
                }
            };
            return Put::Failed(reason, Vec::new());
        }
    };
    let tmp = temp_path(remote, job.token);
    let flags = OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE;
    let handle = match sftp
        .raw
        .open(tmp.as_str(), flags, FileAttributes::default())
        .await
    {
        Ok(h) => h.handle,
        // Nothing was created, so there is nothing to clean up. Errors name the destination,
        // not the temporary file the operator never configured.
        Err(e) => return Put::Failed(remote_error(remote, e), Vec::new()),
    };
    let written = write_all(
        sftp, &handle, &mut file, &local, remote, entry, cancel, on_bytes,
    )
    .await;
    let closed = sftp.raw.close(handle).await;
    match (written, closed) {
        (Err(Stop::Cancelled), _) => return Put::Cancelled(remove_temp(sftp, &tmp).await),
        (Err(Stop::Failed(reason)), _) => {
            return Put::Failed(reason, remove_temp(sftp, &tmp).await);
        }
        (Ok(()), Err(e)) => {
            return Put::Failed(remote_error(remote, e), remove_temp(sftp, &tmp).await);
        }
        (Ok(()), Ok(_)) => {}
    }
    match stat(sftp, &tmp).await {
        Ok(Some(a)) if a.size == Some(entry.size) => {}
        Ok(_) => {
            return Put::Failed(
                Message::SizeMismatch {
                    path: remote.into(),
                },
                remove_temp(sftp, &tmp).await,
            );
        }
        Err(_) => {
            let reason = Message::SizeMismatch {
                path: remote.into(),
            };
            return Put::Failed(reason, remove_temp(sftp, &tmp).await);
        }
    }
    if cancel.is_cancelled() {
        return Put::Cancelled(remove_temp(sftp, &tmp).await);
    }

    let committed = match job.conflict {
        ConflictPolicy::Error => sftp.raw.rename(tmp.as_str(), remote).await.map(drop),
        ConflictPolicy::Overwrite if !sftp.posix_rename => {
            return Put::Failed(Message::NoPosixRename, remove_temp(sftp, &tmp).await);
        }
        ConflictPolicy::Overwrite => posix_rename(&sftp.raw, &tmp, remote).await,
    };
    match committed {
        Ok(()) => Put::Committed,
        // The request may have been carried out (ARC-30).
        Err(e) if is_transport(&e) => Put::Unknown(Message::CommitUnknown {
            path: remote.into(),
        }),
        Err(e) => {
            let left = remove_temp(sftp, &tmp).await;
            // Created by someone else since the check in `prepare`: still not overwritten.
            let reason = if job.conflict == ConflictPolicy::Error
                && matches!(stat(sftp, remote).await, Ok(Some(_)))
            {
                Message::TargetExists {
                    path: remote.into(),
                }
            } else {
                remote_error(remote, e)
            };
            Put::Failed(reason, left)
        }
    }
}

enum Stop {
    Cancelled,
    Failed(Message),
}

/// Stream the local file into the open remote handle, hashing what is read; the content must
/// match the manifest, otherwise the file is not committed (ARC-28).
#[allow(clippy::too_many_arguments)]
async fn write_all(
    sftp: &Sftp,
    handle: &str,
    file: &mut tokio::fs::File,
    local: &Path,
    remote: &str,
    entry: &ManifestEntry,
    cancel: &CancellationToken,
    mut on_bytes: impl FnMut(u64),
) -> Result<(), Stop> {
    let local_name = || local.to_string_lossy().into_owned();
    let mut hasher = Sha256::new();
    let mut offset = 0u64;
    let mut acked = 0u64;
    let mut pending: JoinSet<(u64, Result<(), SftpError>)> = JoinSet::new();
    let mut eof = false;
    let mut buf = vec![0; CHUNK];
    loop {
        while !eof && pending.len() < IN_FLIGHT {
            let n = file.read(&mut buf).await.map_err(|e| {
                Stop::Failed(Message::SourceRead {
                    path: local_name(),
                    detail: e.to_string(),
                })
            })?;
            if n == 0 {
                eof = true;
                break;
            }
            hasher.update(&buf[..n]);
            let raw = Arc::clone(&sftp.raw);
            let (handle, data, at) = (handle.to_owned(), buf[..n].to_vec(), offset);
            pending.spawn(async move { (n as u64, raw.write(handle, at, data).await.map(drop)) });
            offset += n as u64;
        }
        if pending.is_empty() {
            break;
        }
        let joined = tokio::select! {
            j = pending.join_next() => j,
            () = cancel.cancelled() => {
                pending.abort_all();
                return Err(Stop::Cancelled);
            }
        };
        match joined {
            Some(Ok((n, Ok(())))) => {
                acked += n;
                on_bytes(acked);
            }
            Some(Ok((_, Err(e)))) => {
                pending.abort_all();
                return Err(Stop::Failed(remote_error(remote, e)));
            }
            Some(Err(e)) => {
                pending.abort_all();
                return Err(Stop::Failed(Message::Internal {
                    detail: e.to_string(),
                }));
            }
            None => break,
        }
    }
    if offset != entry.size || manifest::hex_digest(hasher.finalize()) != entry.sha256 {
        return Err(Stop::Failed(Message::ContentChanged { path: local_name() }));
    }
    Ok(())
}

async fn posix_rename(raw: &RawSftpSession, from: &str, to: &str) -> Result<(), SftpError> {
    let mut payload = Vec::with_capacity(8 + from.len() + to.len());
    for s in [from, to] {
        payload.extend_from_slice(&u32::try_from(s.len()).unwrap_or(u32::MAX).to_be_bytes());
        payload.extend_from_slice(s.as_bytes());
    }
    match raw.extended(POSIX_RENAME, payload).await? {
        Packet::Status(s) if s.status_code == StatusCode::Ok => Ok(()),
        Packet::Status(s) => Err(SftpError::Status(s)),
        _ => Err(SftpError::UnexpectedPacket),
    }
}

/// Remove this task's temporary file; returns why it could not be removed, if so.
async fn remove_temp(sftp: &Sftp, tmp: &str) -> Vec<Message> {
    match sftp.raw.remove(tmp).await {
        Ok(_) => Vec::new(),
        Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => Vec::new(),
        Err(e) => vec![Message::TempLeft {
            path: tmp.into(),
            detail: e.to_string(),
        }],
    }
}

/// `Ok(None)` when nothing exists at `path`. Follows links, like the server will when writing.
async fn stat(sftp: &Sftp, path: &str) -> Result<Option<FileAttributes>, Message> {
    match sftp.raw.stat(path).await {
        Ok(a) => Ok(Some(a.attrs)),
        Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => Ok(None),
        Err(e) => Err(remote_error(path, e)),
    }
}

/// No answer from the server: the session or connection is gone, or a request timed out.
fn is_transport(e: &SftpError) -> bool {
    !matches!(e, SftpError::Status(_) | SftpError::Limited(_))
}

fn remote_error(path: &str, e: SftpError) -> Message {
    match e {
        SftpError::Status(s) if s.status_code == StatusCode::PermissionDenied => {
            Message::PermissionDenied { path: path.into() }
        }
        e if is_transport(&e) => Message::ConnectionLost.with_detail(e.to_string()),
        e => Message::RemoteFailed {
            path: path.into(),
            detail: e.to_string(),
        },
    }
}

fn local_path(source: &Path, relative: &str) -> PathBuf {
    if relative.is_empty() {
        return source.to_path_buf();
    }
    relative
        .split('/')
        .fold(source.to_path_buf(), |p, part| p.join(part))
}
