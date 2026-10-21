//! Phase 0 spike: checks the russh / russh-sftp capabilities the plan depends on,
//! against the Docker targets in `../../test-env`. Prints one PASS/FAIL line per check.
//!
//! Run: `cargo run -- <path to test-env/keys>`

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use russh::client::{self, Handle};
use russh::keys::{self, Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{ChannelMsg, Disconnect, Preferred, Sig};
use russh_sftp::client::{RawSftpSession, SftpSession};
use russh_sftp::protocol::{OpenFlags, Packet, StatusCode};

const MODERN: (&str, u16) = ("127.0.0.1", 2222);
const LEGACY: (&str, u16) = ("127.0.0.1", 2223);

/// Records the server key instead of trusting blindly, as SRV-03 requires.
struct Recorder {
    seen: Arc<Mutex<Option<PublicKey>>>,
    trusted: Option<String>,
}

impl client::Handler for Recorder {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let keys::PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Ok(false);
        };
        *self.seen.lock().unwrap() = Some(key.clone());
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        Ok(self.trusted.as_deref().map_or(true, |t| t == fp))
    }
}

/// The default key list minus plain `ssh-rsa` (SHA-1); the "allow legacy" toggle adds it back.
fn preferred(allow_legacy: bool) -> Preferred {
    let keys: Vec<Algorithm> = Preferred::DEFAULT
        .key
        .iter()
        .filter(|a| allow_legacy || !matches!(a, Algorithm::Rsa { hash: None }))
        .cloned()
        .collect();
    Preferred { key: Cow::Owned(keys), ..Preferred::DEFAULT }
}

async fn connect(
    addr: (&str, u16),
    allow_legacy: bool,
    trusted: Option<String>,
) -> Result<(Handle<Recorder>, Arc<Mutex<Option<PublicKey>>>)> {
    let seen = Arc::new(Mutex::new(None));
    let config = client::Config {
        preferred: preferred(allow_legacy),
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let handler = Recorder { seen: seen.clone(), trusted };
    let h = tokio::time::timeout(Duration::from_secs(10), client::connect(Arc::new(config), addr, handler))
        .await
        .context("connect timeout")??;
    Ok((h, seen))
}

async fn auth_key(h: &mut Handle<Recorder>, user: &str, key: PrivateKey) -> Result<()> {
    let hash = h.best_supported_rsa_hash().await?.flatten();
    let res = h.authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await?;
    if !res.success() {
        bail!("publickey auth rejected");
    }
    Ok(())
}

async fn session(user: &str, key: &Path) -> Result<Handle<Recorder>> {
    let (mut h, _) = connect(MODERN, false, None).await?;
    auth_key(&mut h, user, keys::load_secret_key(key, None)?).await?;
    Ok(h)
}

#[derive(Debug, Default)]
struct Exec {
    stdout: String,
    stderr: String,
    status: Option<u32>,
    signal: Option<String>,
}

/// Runs one command on its own channel, optionally feeding `stdin` then EOF.
async fn exec(h: &Handle<Recorder>, cmd: &str, stdin: Option<&[u8]>) -> Result<Exec> {
    let mut ch = h.channel_open_session().await?;
    ch.exec(true, cmd).await?;
    if let Some(data) = stdin {
        ch.data_bytes(data.to_vec()).await?;
        ch.eof().await?;
    }
    let mut out = Exec::default();
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.stdout.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::ExtendedData { data, .. } => out.stderr.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::ExitStatus { exit_status } => out.status = Some(exit_status),
            ChannelMsg::ExitSignal { signal_name, .. } => out.signal = Some(format!("{signal_name:?}")),
            _ => {}
        }
    }
    Ok(out)
}

fn report(name: &str, r: Result<String>) -> bool {
    match r {
        Ok(detail) => {
            println!("PASS  {name}: {detail}");
            true
        }
        Err(e) => {
            println!("FAIL  {name}: {e:#}");
            false
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "../../test-env/keys".into()));
    let k = |n: &str| dir.join(n);
    let mut ok = true;

    // 1. Host key callback + SHA-256 fingerprint, compared with ssh-keyscan/ssh-keygen output.
    ok &= report("host key fingerprint", async {
        let (h, seen) = connect(MODERN, false, None).await?;
        let key = seen.lock().unwrap().clone().ok_or(anyhow!("no key seen"))?;
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        h.disconnect(Disconnect::ByApplication, "", "").await?;
        let scan = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("ssh-keyscan -p {} -t {} {} 2>/dev/null | ssh-keygen -lf - -E sha256", MODERN.1, key.algorithm(), MODERN.0))
            .output()?;
        let scan = String::from_utf8_lossy(&scan.stdout);
        if !scan.contains(&fp) {
            bail!("russh {fp} vs ssh-keygen {scan}");
        }
        Ok(format!("{} {fp} matches ssh-keygen", key.algorithm()))
    }.await);

    // 2. A changed/unknown key is rejected when it does not match the trusted fingerprint.
    ok &= report("host key mismatch rejected", async {
        match connect(MODERN, false, Some("SHA256:not-the-real-key".into())).await {
            Ok(_) => bail!("connected despite mismatch"),
            Err(e) => Ok(format!("connect refused ({e})")),
        }
    }.await);

    // 3. Plain OpenSSH ed25519 key.
    ok &= report("auth ed25519", async {
        session("deploy", &k("id_ed25519")).await?;
        Ok("ok".into())
    }.await);

    // 4. Encrypted OpenSSH key: right and wrong passphrase.
    ok &= report("auth encrypted key", async {
        if keys::load_secret_key(k("id_ed25519_enc"), Some("wrong")).is_ok() {
            bail!("wrong passphrase accepted");
        }
        let err = keys::load_secret_key(k("id_ed25519_enc"), None).err().map(|e| e.to_string());
        let key = keys::load_secret_key(k("id_ed25519_enc"), Some("test-passphrase"))?;
        let (mut h, _) = connect(MODERN, false, None).await?;
        auth_key(&mut h, "deploy", key).await?;
        Ok(format!("wrong passphrase rejected; missing passphrase error: {err:?}"))
    }.await);

    // 5. PuTTY .ppk v2 (unencrypted rsa) and v3 (Argon2-encrypted ed25519) via ssh-key's ppk feature.
    ok &= report("auth .ppk v2/v3", async {
        let v2 = ssh_key::PrivateKey::from_ppk(std::fs::read_to_string(k("id_rsa_v2.ppk"))?, None)?;
        let v3 = ssh_key::PrivateKey::from_ppk(std::fs::read_to_string(k("id_ed25519_v3_enc.ppk"))?, Some("test-passphrase".into()))?;
        let bad = ssh_key::PrivateKey::from_ppk(std::fs::read_to_string(k("id_ed25519_v3_enc.ppk"))?, Some("wrong".into()));
        for key in [v2, v3] {
            let (mut h, _) = connect(MODERN, false, None).await?;
            auth_key(&mut h, "deploy", key).await?;
        }
        Ok(format!("both accepted; wrong v3 passphrase -> {:?}", bad.err().map(|e| e.to_string())))
    }.await);

    // 6. Password auth.
    ok &= report("auth password", async {
        let (mut h, _) = connect(MODERN, false, None).await?;
        if !h.authenticate_password("deploy", "deploy-pass").await?.success() {
            bail!("rejected");
        }
        Ok("ok".into())
    }.await);

    // 7. Legacy server: refused without the toggle, works with it (host key and RSA/SHA-1 user auth).
    ok &= report("legacy ssh-rsa toggle", async {
        let off = connect(LEGACY, false, None).await.err().map(|e| e.to_string());
        if off.is_none() {
            bail!("connected to ssh-rsa-only server without the toggle");
        }
        let (mut h, seen) = connect(LEGACY, true, None).await?;
        let alg = seen.lock().unwrap().as_ref().map(|k| k.algorithm().to_string());
        let hash = h.best_supported_rsa_hash().await?;
        auth_key(&mut h, "deploy", keys::load_secret_key(k("id_rsa"), None)?).await?;
        Ok(format!("toggle off -> {off:?}; on -> host key {alg:?}, server rsa hash {hash:?}, rsa user auth ok"))
    }.await);

    let h = session("deploy", &k("id_ed25519")).await?;

    // 8. Exit status, stderr separation, and "unknown" result when the process dies by signal.
    ok &= report("exit status / signal", async {
        let a = exec(&h, "echo out; echo err >&2; exit 7", None).await?;
        let b = exec(&h, "kill -9 $$", None).await?;
        if a.status != Some(7) || a.stdout.trim() != "out" || a.stderr.trim() != "err" {
            bail!("{a:?}");
        }
        if b.status.is_some() {
            bail!("expected no exit-status for a killed shell, got {b:?}");
        }
        Ok(format!("exit 7 ok; killed -> status None, signal {:?}", b.signal))
    }.await);

    // 9. sudo modes. Password goes over stdin; the prompt is a random marker so it can be detected.
    ok &= report("sudo -n / -S", async {
        let np = session("nopass", &k("id_ed25519")).await?;
        let a = exec(&np, "sudo -n -u root -- id -u", None).await?;
        let ns = session("nosudo", &k("id_ed25519")).await?;
        let b = exec(&ns, "sudo -n -u root -- id -u", None).await?;
        let c = exec(&h, "sudo -S -p 'MDM-PROMPT-7f3a:' -u root -- sh -c 'id -u; tr \"\\0\" \" \" < /proc/$PPID/cmdline'", Some(b"deploy-pass\n")).await?;
        let d = exec(&h, "sudo -S -p 'MDM-PROMPT-7f3a:' -u root -- id -u", Some(b"wrong\n")).await?;
        let tty = session("ttyuser", &k("id_ed25519")).await?;
        let e = exec(&tty, "sudo -S -p 'MDM-PROMPT-7f3a:' -u root -- id -u", Some(b"ttyuser-pass\n")).await?;
        if a.status != Some(0) || b.status == Some(0) || c.status != Some(0) || c.stdout.contains("deploy-pass") || d.status == Some(0) {
            bail!("{a:?} {b:?} {c:?} {d:?}");
        }
        Ok(format!(
            "NOPASSWD ok; no-rights -> {:?}; -S ok, sudo cmdline = {:?} (no password); wrong pw -> {:?}; requiretty -> {:?}",
            b.stderr.trim(), c.stdout.lines().nth(1).unwrap_or(""), d.stderr.trim(), e.stderr.trim()
        ))
    }.await);

    // 10. Remote cancel: closing the channel leaves the process running; the signal request and a
    //     process-group kill from a second channel are both checked.
    ok &= report("remote cancel", async {
        let wrapped = "exec setsid sh -c 'echo MDM_PGID $$ >&2; sleep 300 & sleep 300'";
        let mut ch = h.channel_open_session().await?;
        ch.exec(true, wrapped).await?;
        let mut pgid = None;
        while pgid.is_none() {
            match ch.wait().await {
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    pgid = String::from_utf8_lossy(&data).split_whitespace().nth(1).map(str::to_owned)
                }
                Some(_) => {}
                None => bail!("channel closed before pgid"),
            }
        }
        let pgid = pgid.unwrap();
        // Signal request (OpenSSH honours it for non-pty sessions since 7.9 only for the direct child).
        ch.signal(Sig::TERM).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let after_signal = exec(&h, &format!("pgrep -g {pgid} | wc -l"), None).await?.stdout.trim().to_owned();
        ch.close().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let after_close = exec(&h, &format!("pgrep -g {pgid} | wc -l"), None).await?.stdout.trim().to_owned();
        let killed = exec(&h, &format!("kill -TERM -- -{pgid}; sleep 1; pgrep -g {pgid} | wc -l"), None).await?.stdout.trim().to_owned();
        if killed != "0" {
            bail!("process group still alive after kill: {killed}");
        }
        Ok(format!("pgid {pgid}: after signal {after_signal} alive, after channel close {after_close} alive, after group kill {killed}"))
    }.await);

    // 11. SFTP: posix-rename over an existing file, plain rename refusal, permission errors.
    ok &= report("sftp atomic replace", async {
        let ch = h.channel_open_session().await?;
        ch.request_subsystem(true, "sftp").await?;
        let raw = RawSftpSession::new(ch.into_stream());
        let version = raw.init().await?;
        let posix = version.extensions.get("posix-rename@openssh.com").cloned();
        let ch2 = h.channel_open_session().await?;
        ch2.request_subsystem(true, "sftp").await?;
        let sftp = SftpSession::new(ch2.into_stream()).await?;

        let dir = "/home/deploy/mdm-spike";
        let _ = sftp.create_dir(dir).await;
        sftp.write(format!("{dir}/target"), b"old").await.ok();
        sftp.create(format!("{dir}/target")).await?; // truncate
        sftp.write(format!("{dir}/target"), b"old").await?;
        let tmp = format!("{dir}/.target.mdm-tmp");
        let mut f = sftp.open_with_flags(&tmp, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE).await?;
        tokio::io::AsyncWriteExt::write_all(&mut f, b"new").await?;
        tokio::io::AsyncWriteExt::shutdown(&mut f).await?;

        let plain = sftp.rename(&tmp, format!("{dir}/target")).await.err().map(|e| e.to_string());
        let mut payload = Vec::new();
        for s in [tmp.as_str(), &format!("{dir}/target")] {
            payload.extend_from_slice(&(s.len() as u32).to_be_bytes());
            payload.extend_from_slice(s.as_bytes());
        }
        match raw.extended("posix-rename@openssh.com", payload).await? {
            Packet::Status(s) if s.status_code == StatusCode::Ok => {}
            other => bail!("posix-rename reply {other:?}"),
        }
        let content = sftp.read(format!("{dir}/target")).await?;
        let denied = sftp.create("/opt/protected/x").await.err().map(|e| e.to_string());
        if content != b"new" || denied.is_none() {
            bail!("content {:?}, denied {denied:?}", String::from_utf8_lossy(&content));
        }
        Ok(format!("extension advertised {posix:?}; plain rename over existing -> {plain:?}; posix-rename ok; protected dir -> {denied:?}"))
    }.await);

    println!("{}", if ok { "ALL PASS" } else { "SOME CHECKS FAILED" });
    Ok(())
}
