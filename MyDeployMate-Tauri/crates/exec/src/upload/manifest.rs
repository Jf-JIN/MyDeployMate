//! Scans an upload source into an [`ArtifactManifest`] (architecture ARC-28).
//!
//! Hidden files are included. Symbolic links, Windows junctions and other reparse points, and
//! special files (devices, sockets, pipes) are rejected anywhere in the tree: P0 uploads only
//! regular files and directories (STEP-03).

use std::fs::{self, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use mdm_core::{ArtifactManifest, ManifestEntry, Message, SourceKind};
use sha2::{Digest, Sha256};

const READ_CHUNK: usize = 64 * 1024;

/// Scan and hash `source`. Blocking; run it on a blocking thread. Fails on the first entry that
/// cannot be uploaded, naming its local path.
pub fn scan(source: &Path, kind: SourceKind) -> Result<ArtifactManifest, Message> {
    let meta = match fs::symlink_metadata(source) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Message::SourceMissing {
                path: display(source),
            });
        }
        Err(e) => return Err(read_error(source, &e)),
    };
    check_plain(source, &meta)?;
    let mut manifest = ArtifactManifest {
        kind,
        files: Vec::new(),
        dirs: Vec::new(),
    };
    match kind {
        SourceKind::File => {
            if !meta.is_file() {
                return Err(Message::SourceNotFile {
                    path: display(source),
                });
            }
            manifest.files.push(entry(source, String::new(), &meta)?);
        }
        SourceKind::Directory => {
            if !meta.is_dir() {
                return Err(Message::SourceNotDir {
                    path: display(source),
                });
            }
            walk(source, "", &mut manifest)?;
            manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
        }
    }
    Ok(manifest)
}

/// A SHA-256 digest as lowercase hex, the form stored in a manifest.
pub fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;
    digest.as_ref().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn walk(dir: &Path, prefix: &str, manifest: &mut ArtifactManifest) -> Result<(), Message> {
    let mut children: Vec<_> = fs::read_dir(dir)
        .map_err(|e| read_error(dir, &e))?
        .collect::<Result<_, _>>()
        .map_err(|e| read_error(dir, &e))?;
    children.sort_by_key(|c| c.file_name());
    for child in children {
        let path = child.path();
        let Some(name) = child.file_name().to_str().map(str::to_owned) else {
            return Err(Message::SourceRead {
                path: display(&path),
                detail: "file name is not valid Unicode".into(),
            });
        };
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let meta = fs::symlink_metadata(&path).map_err(|e| read_error(&path, &e))?;
        check_plain(&path, &meta)?;
        if meta.is_dir() {
            manifest.dirs.push(rel.clone());
            walk(&path, &rel, manifest)?;
        } else if meta.is_file() {
            manifest.files.push(entry(&path, rel, &meta)?);
        } else {
            return Err(Message::SourceSpecial {
                path: display(&path),
            });
        }
    }
    Ok(())
}

/// Reject links and reparse points before anything follows them.
fn check_plain(path: &Path, meta: &Metadata) -> Result<(), Message> {
    if meta.file_type().is_symlink() || is_reparse_point(meta) {
        return Err(Message::SourceLink {
            path: display(path),
        });
    }
    if !meta.is_file() && !meta.is_dir() {
        return Err(Message::SourceSpecial {
            path: display(path),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &Metadata) -> bool {
    false
}

fn entry(path: &Path, rel: String, meta: &Metadata) -> Result<ManifestEntry, Message> {
    let mut file = fs::File::open(path).map_err(|e| read_error(path, &e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; READ_CHUNK];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| read_error(path, &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok());
    Ok(ManifestEntry {
        path: rel,
        size,
        modified_ms,
        sha256: hex_digest(hasher.finalize()),
    })
}

fn read_error(path: &Path, e: &std::io::Error) -> Message {
    Message::SourceRead {
        path: display(path),
        detail: e.to_string(),
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // SHA-256 of "hello".
    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn file_source() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("app.jar");
        fs::write(&f, "hello").unwrap();
        let m = scan(&f, SourceKind::File).unwrap();
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].path, "");
        assert_eq!(m.files[0].size, 5);
        assert_eq!(m.files[0].sha256, HELLO);
        assert!(m.dirs.is_empty());
    }

    #[test]
    fn directory_includes_hidden_files_and_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("css/empty")).unwrap();
        fs::write(root.join(".env"), "hello").unwrap();
        fs::write(root.join("css/site.css"), "").unwrap();
        fs::write(root.join("index.html"), "hi").unwrap();
        let m = scan(root, SourceKind::Directory).unwrap();
        let paths: Vec<_> = m.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec![".env", "css/site.css", "index.html"]);
        assert_eq!(m.dirs, vec!["css", "css/empty"]);
        assert_eq!(m.total_bytes(), 7);
    }

    #[test]
    fn missing_and_wrong_kind() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(matches!(
            scan(&missing, SourceKind::File),
            Err(Message::SourceMissing { .. })
        ));
        assert!(matches!(
            scan(dir.path(), SourceKind::File),
            Err(Message::SourceNotFile { .. })
        ));
        let f = dir.path().join("f");
        fs::write(&f, "").unwrap();
        assert!(matches!(
            scan(&f, SourceKind::Directory),
            Err(Message::SourceNotDir { .. })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_junctions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dist");
        fs::create_dir_all(root.join("real")).unwrap();
        let junction = root.join("junction");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(root.join("real"))
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        assert_eq!(
            scan(&root, SourceKind::Directory).unwrap_err(),
            Message::SourceLink {
                path: display(&junction)
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_and_special_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dist");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a"), "").unwrap();
        std::os::unix::fs::symlink(root.join("a"), root.join("link")).unwrap();
        let err = scan(&root, SourceKind::Directory).unwrap_err();
        assert_eq!(
            err,
            Message::SourceLink {
                path: display(&root.join("link"))
            }
        );
        // The source itself being a link is rejected too, without following it.
        assert!(matches!(
            scan(&root.join("link"), SourceKind::File),
            Err(Message::SourceLink { .. })
        ));

        fs::remove_file(root.join("link")).unwrap();
        let sock = root.join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        assert_eq!(
            scan(&root, SourceKind::Directory).unwrap_err(),
            Message::SourceSpecial {
                path: display(&sock)
            }
        );
    }
}
