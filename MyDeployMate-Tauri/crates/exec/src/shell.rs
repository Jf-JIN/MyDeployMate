//! Shell adapters for local steps (SRS STEP-02, architecture ARC-17).
//!
//! Each shell has its own launch arguments; there is no shared quoting rule. The user's
//! command text is passed as one argument and never rewritten (no `chcp`, no profile).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use mdm_core::Shell;

/// Executable file name for each shell.
pub fn program_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Powershell => "powershell.exe",
        Shell::Pwsh => "pwsh.exe",
        Shell::Cmd => "cmd.exe",
        Shell::Wsl => "wsl.exe",
    }
}

/// Look `name` up in `path` (a `PATH` value) the way the preview shows it. The current
/// directory is never searched, unlike `CreateProcess`.
pub fn find_program(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Full path of the shell's executable on this computer, from the process `PATH`.
pub fn resolve_shell(shell: Shell) -> Option<PathBuf> {
    find_program(program_name(shell), &std::env::var_os("PATH")?)
}

/// What to start for one local task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Appended verbatim after `args` on Windows, bypassing argument quoting (cmd only).
    pub raw_args: Option<String>,
    /// Working directory for the Windows process. WSL passes its Linux directory in `args`.
    pub cwd: Option<PathBuf>,
}

impl Invocation {
    /// Build the launch for `script`. `cwd` is a Windows path, or a Linux path for WSL.
    pub fn new(
        shell: Shell,
        program: PathBuf,
        script: &str,
        cwd: Option<&str>,
        wsl_distro: Option<&str>,
    ) -> Self {
        let mut inv = Invocation {
            program,
            args: Vec::new(),
            raw_args: None,
            cwd: None,
        };
        match shell {
            Shell::Powershell | Shell::Pwsh => {
                inv.args = [
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    script,
                ]
                .map(str::to_owned)
                .to_vec();
                inv.cwd = cwd.map(PathBuf::from);
            }
            Shell::Cmd => {
                // `/s` makes cmd strip exactly the outer quotes and run the rest as typed.
                inv.raw_args = Some(format!("/d /s /c \"{script}\""));
                inv.cwd = cwd.map(PathBuf::from);
            }
            Shell::Wsl => {
                inv.args
                    .extend(["-d".into(), wsl_distro.unwrap_or_default().into()]);
                if let Some(dir) = cwd {
                    inv.args.extend(["--cd".into(), dir.into()]);
                }
                // `--exec` hands the arguments to bash directly; `--` would first pass the
                // whole line through the distribution's default shell, expanding `$` twice.
                inv.args
                    .extend(["--exec".into(), "bash".into(), "-c".into(), script.into()]);
            }
        }
        inv
    }

    /// Command line as shown in the preview. Arguments with spaces or quotes are quoted the
    /// way the Windows C runtime reads them.
    pub fn command_line(&self) -> String {
        let mut line = quote(&self.program.to_string_lossy());
        for arg in &self.args {
            line.push(' ');
            line.push_str(&quote(arg));
        }
        if let Some(raw) = &self.raw_args {
            line.push(' ');
            line.push_str(raw);
        }
        line
    }

    pub(crate) fn os_args(&self) -> Vec<OsString> {
        self.args.iter().map(OsString::from).collect()
    }

    pub(crate) fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }
}

/// Windows C runtime quoting: wrap in quotes when needed, escape quotes and the backslashes
/// in front of them.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn program(shell: Shell) -> PathBuf {
        PathBuf::from(format!(r"C:\Windows\System32\{}", program_name(shell)))
    }

    #[test]
    fn powershell_passes_script_as_one_argument() {
        let inv = Invocation::new(
            Shell::Powershell,
            program(Shell::Powershell),
            r#"Write-Output "a b"; exit 3"#,
            Some(r"D:\app"),
            None,
        );
        assert_eq!(
            inv.args,
            vec![
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                r#"Write-Output "a b"; exit 3"#
            ]
        );
        assert_eq!(inv.cwd, Some(PathBuf::from(r"D:\app")));
        assert_eq!(
            inv.command_line(),
            r#"C:\Windows\System32\powershell.exe -NoLogo -NoProfile -NonInteractive -Command "Write-Output \"a b\"; exit 3""#
        );
    }

    #[test]
    fn cmd_uses_raw_arguments() {
        let inv = Invocation::new(
            Shell::Cmd,
            program(Shell::Cmd),
            r#"cargo build --release && echo "done""#,
            Some(r"D:\app"),
            None,
        );
        assert!(inv.args.is_empty());
        assert_eq!(
            inv.raw_args.as_deref(),
            Some(r#"/d /s /c "cargo build --release && echo "done"""#)
        );
        assert_eq!(
            inv.command_line(),
            r#"C:\Windows\System32\cmd.exe /d /s /c "cargo build --release && echo "done"""#
        );
    }

    #[test]
    fn wsl_runs_bash_in_linux_directory() {
        let inv = Invocation::new(
            Shell::Wsl,
            program(Shell::Wsl),
            "echo $HOME",
            Some("/home/me/app"),
            Some("Ubuntu-22.04"),
        );
        assert_eq!(
            inv.args,
            vec![
                "-d",
                "Ubuntu-22.04",
                "--cd",
                "/home/me/app",
                "--exec",
                "bash",
                "-c",
                "echo $HOME"
            ]
        );
        assert_eq!(inv.cwd, None);
    }

    #[test]
    fn quoting_follows_c_runtime_rules() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote(r"C:\Program Files\x"), r#""C:\Program Files\x""#);
        assert_eq!(quote(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(quote(r"dir\ "), r#""dir\ ""#);
        assert_eq!(quote(r"end\ x\"), r#""end\ x\\""#);
    }

    #[test]
    fn find_program_skips_relative_and_missing_dirs() {
        let dir = std::env::temp_dir().join(format!("mdm-exec-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.exe"), b"").unwrap();
        let path =
            std::env::join_paths([PathBuf::from("relative"), dir.join("missing"), dir.clone()])
                .unwrap();
        assert_eq!(find_program("tool.exe", &path), Some(dir.join("tool.exe")));
        assert_eq!(find_program("other.exe", &path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
