//! Phase 0 spike, Windows-only checks: Job Object process-tree kill, local output decoding,
//! Windows Credential Manager via `keyring`, and SSH agent reachability (OpenSSH pipe, Pageant).
//! Building it also proves russh (aws-lc-rs) compiles with the local toolchain.
//!
//! Run on Windows: `cargo run`

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use encoding_rs::GBK;
use process_wrap::std::{CommandWrap, JobObject};

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

fn skip(name: &str, why: &str) {
    println!("SKIP  {name}: {why}");
}

/// Same rule the product will use: UTF-8 when it decodes cleanly, otherwise the ANSI code page.
fn decode_auto(bytes: &[u8]) -> (String, &'static str) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_owned(), "utf-8"),
        Err(_) => (GBK.decode(bytes).0.into_owned(), "gbk"),
    }
}

fn count_processes(image: &str) -> usize {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/NH", "/FO", "CSV"])
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    String::from_utf8_lossy(&out).lines().filter(|l| l.contains(image)).count()
}

fn which(exe: &str) -> bool {
    Command::new("where").arg(exe).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn run_bytes(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new(program).args(args).output()?;
    let mut all = out.stdout;
    all.extend_from_slice(&out.stderr);
    Ok(all)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut ok = true;
    println!("ANSI code page: {}", String::from_utf8_lossy(&run_bytes("cmd", &["/c", "chcp"])?).trim());

    // 1. Job Object: cmd starts a detached grandchild; killing the job must take both down.
    ok &= report("job object kills process tree", (|| {
        let before = count_processes("ping.exe");
        let mut cmd = CommandWrap::with_new("cmd", |c| {
            c.args(["/c", "start /b ping -n 120 127.0.0.1 >nul & ping -n 120 127.0.0.1 >nul"]);
        });
        cmd.wrap(JobObject);
        let mut child = cmd.spawn()?;
        std::thread::sleep(Duration::from_secs(2));
        let running = count_processes("ping.exe") - before;
        let t = Instant::now();
        child.start_kill()?;
        child.wait()?;
        std::thread::sleep(Duration::from_millis(500));
        let left = count_processes("ping.exe").saturating_sub(before);
        if running < 2 || left != 0 {
            bail!("ping processes: {running} started, {left} left after kill");
        }
        Ok(format!("{running} ping.exe started, 0 left, kill took {:?}", t.elapsed()))
    })());

    // 2. Output decoding for cmd and both PowerShells, with a Chinese literal.
    let sample = "中文测试";
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("cmd", vec!["/c", "echo 中文测试"]),
        ("cmd", vec!["/c", "dir", "/?"]),
        ("powershell", vec!["-NoProfile", "-Command", "Write-Output '中文测试'"]),
        ("pwsh", vec!["-NoProfile", "-Command", "Write-Output '中文测试'"]),
    ];
    for (program, args) in cases {
        let name = format!("decode {program} {}", args.last().unwrap());
        if !which(program) {
            skip(&name, "not installed");
            continue;
        }
        ok &= report(&name, (|| {
            let bytes = run_bytes(program, &args)?;
            let (text, used) = decode_auto(&bytes);
            let first = text.lines().next().unwrap_or("").trim().to_owned();
            if args.last().unwrap().contains(sample) && !text.contains(sample) {
                bail!("decoded as {used} but text is {first:?}");
            }
            if text.contains('\u{FFFD}') {
                bail!("replacement characters after {used} decode: {first:?}");
            }
            Ok(format!("decoded as {used}: {first:?}"))
        })());
    }

    // 3. WSL: wsl.exe's own messages are UTF-16LE; output from inside a distro is UTF-8.
    if which("wsl") {
        ok &= report("decode wsl", (|| {
            let list = Command::new("wsl").args(["-l", "-q"]).output()?.stdout;
            let utf16: Vec<u16> = list.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let distros = String::from_utf16_lossy(&utf16).replace('\0', "");
            let distro = distros.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_owned);
            let Some(distro) = distro else { return Ok("wsl.exe present, no distro installed".into()) };
            let bytes = run_bytes("wsl", &["-d", &distro, "--cd", "/tmp", "--", "sh", "-c", "echo 中文测试"])?;
            let text = String::from_utf8(bytes)?;
            if !text.contains(sample) {
                bail!("{text:?}");
            }
            Ok(format!("distros (utf-16): {:?}; {distro} output utf-8 ok", distros.lines().collect::<Vec<_>>()))
        })());
    } else {
        skip("decode wsl", "wsl.exe not found");
    }

    // 4. Windows Credential Manager round trip, then delete.
    ok &= report("credential manager", (|| {
        let entry = keyring::Entry::new("MyDeployMate-spike", "server-1/deploy/sudo:root")?;
        entry.set_password("p@ss 中文")?;
        let back = entry.get_password()?;
        entry.delete_credential()?;
        let gone = entry.get_password().is_err();
        if back != "p@ss 中文" || !gone {
            bail!("read back {back:?}, deleted {gone}");
        }
        Ok("set / get (unicode) / delete ok".into())
    })());

    // 5. SSH agents: report reachability and identity counts (no server needed).
    match russh::keys::agent::client::AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        Ok(mut a) => ok &= report("openssh agent", async {
            Ok(format!("{} identities", a.request_identities().await?.len()))
        }.await),
        Err(e) => skip("openssh agent", &format!("not reachable ({e}); start the 'OpenSSH Authentication Agent' service to test")),
    }
    match russh::keys::agent::client::AgentClient::connect_pageant().await {
        Ok(mut a) => match a.request_identities().await {
            Ok(ids) => ok &= report("pageant", Ok(format!("{} identities", ids.len()))),
            Err(e) => skip("pageant", &format!("not running ({e})")),
        },
        Err(e) => skip("pageant", &format!("not running ({e})")),
    }

    println!("{}", if ok { "ALL PASS (see SKIP lines)" } else { "SOME CHECKS FAILED" });
    Ok(())
}
