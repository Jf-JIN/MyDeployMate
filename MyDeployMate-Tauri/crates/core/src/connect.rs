//! Why an SSH connection failed (SRS SRV-04, architecture ARC-21, ARC-36).
//!
//! The connection layer in `mdm-exec` classifies every failure into one of these kinds, so the
//! connection test (IXD-18) and run detail can name the stage and suggest what to do, and never
//! report a refused login as "offline". Messages are safe to show: no secrets, no key bodies.

use serde::Serialize;
use ts_rs::TS;

use crate::trust::{HostEndpoint, HostKey};

/// The step a connection had reached when it failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConnectStage {
    /// Reading the key file, password variable or agent, before any network traffic.
    Prepare,
    /// Looking up the host name.
    Resolve,
    /// Opening the TCP connection.
    Connect,
    /// SSH version exchange and key exchange.
    Handshake,
    /// Checking the server key against the trust store.
    HostKey,
    Authenticate,
    /// Running a command after login (connection test probe).
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ConnectErrorKind {
    KeyFileUnreadable {
        path: String,
    },
    /// SRV-02: an unrecognised key file must say so, not "authentication failed".
    KeyFormatUnsupported {
        path: String,
    },
    PasswordEnvMissing {
        name: String,
    },
    AgentUnavailable,
    AgentNoIdentities,
    ResolveFailed,
    ConnectionRefused,
    Unreachable,
    Timeout,
    /// No common algorithm. `legacyRsaOnly` when the server only offers `ssh-rsa` and the
    /// server setting "allow legacy algorithms" is off (IXD-18 suggestion).
    AlgorithmNegotiation {
        legacy_rsa_only: bool,
    },
    /// First connection: the operator must check the fingerprint (D02).
    HostKeyUnknown {
        endpoint: HostEndpoint,
        presented: HostKey,
    },
    /// The recorded key differs: blocked until an explicit trust update (D02).
    HostKeyChanged {
        endpoint: HostEndpoint,
        known: Vec<HostKey>,
        presented: HostKey,
    },
    KeyPassphraseWrong,
    AuthenticationFailed,
    /// The operator cancelled a prompt or the whole operation.
    Cancelled,
    /// Logged in, but the side-effect-free probe command of the connection test failed.
    CommandFailed {
        exit_status: Option<u32>,
    },
    /// The server closed the connection, e.g. after its login grace time.
    Disconnected,
    /// Anything else the SSH library reported; `message` has the detail.
    Protocol,
}

impl ConnectErrorKind {
    pub fn stage(&self) -> ConnectStage {
        use ConnectErrorKind::*;
        match self {
            KeyFileUnreadable { .. }
            | KeyFormatUnsupported { .. }
            | PasswordEnvMissing { .. }
            | AgentUnavailable
            | AgentNoIdentities => ConnectStage::Prepare,
            ResolveFailed => ConnectStage::Resolve,
            ConnectionRefused | Unreachable => ConnectStage::Connect,
            AlgorithmNegotiation { .. } => ConnectStage::Handshake,
            HostKeyUnknown { .. } | HostKeyChanged { .. } => ConnectStage::HostKey,
            KeyPassphraseWrong | AuthenticationFailed => ConnectStage::Authenticate,
            CommandFailed { .. } => ConnectStage::Command,
            // Depends on when it happened; the connection layer passes the real stage.
            Timeout | Cancelled | Disconnected | Protocol => ConnectStage::Handshake,
        }
    }

    /// A short, safe explanation with a suggestion where one helps.
    pub fn describe(&self) -> String {
        use ConnectErrorKind::*;
        match self {
            KeyFileUnreadable { path } => format!("无法读取私钥文件 {path}"),
            KeyFormatUnsupported { path } => {
                format!("不支持的私钥格式：{path}。支持 OpenSSH 格式与 PuTTY .ppk v2/v3")
            }
            PasswordEnvMissing { name } => format!(
                "本机环境变量 {name} 未设置或为空。环境变量在应用启动时读取，修改后需重启应用"
            ),
            AgentUnavailable => "SSH Agent 不可用，请确认 Agent 服务或 Pageant 已启动".into(),
            AgentNoIdentities => "SSH Agent 中没有可用的密钥".into(),
            ResolveFailed => "无法解析服务器地址，请检查地址拼写和 DNS".into(),
            ConnectionRefused => "连接被拒绝，请检查端口和服务器上的 SSH 服务".into(),
            Unreachable => "网络不可达，请检查网络连接和防火墙".into(),
            Timeout => "连接超时".into(),
            AlgorithmNegotiation {
                legacy_rsa_only: true,
            } => "该服务器只支持 ssh-rsa，可在服务器设置中开启“允许旧算法”".into(),
            AlgorithmNegotiation { .. } => "与服务器没有共同支持的加密算法".into(),
            HostKeyUnknown { .. } => "首次连接该服务器，需要核对并信任服务器身份".into(),
            HostKeyChanged { .. } => "服务器身份与已记录的不一致，连接已停止".into(),
            KeyPassphraseWrong => "私钥口令不正确".into(),
            AuthenticationFailed => "认证失败，服务器未接受该账号的凭据".into(),
            CommandFailed {
                exit_status: Some(code),
            } => format!("已登录，但探测命令失败（退出码 {code}）"),
            CommandFailed { .. } => "已登录，但探测命令没有正常结束".into(),
            Cancelled => "已取消".into(),
            Disconnected => "服务器断开了连接".into(),
            Protocol => "SSH 协议错误".into(),
        }
    }
}

/// A classified connection failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectError {
    pub stage: ConnectStage,
    #[serde(flatten)]
    pub kind: ConnectErrorKind,
    /// Safe text for the operator: [`ConnectErrorKind::describe`] plus any technical detail.
    pub message: String,
}

impl ConnectError {
    pub fn new(kind: ConnectErrorKind) -> Self {
        Self::at(kind.stage(), kind)
    }

    pub fn at(stage: ConnectStage, kind: ConnectErrorKind) -> Self {
        let message = kind.describe();
        Self {
            stage,
            kind,
            message,
        }
    }

    /// Append technical detail, e.g. the algorithm lists or an IO error.
    pub fn with_detail(mut self, detail: impl AsRef<str>) -> Self {
        let detail = detail.as_ref().trim();
        if !detail.is_empty() {
            self.message = format!("{}（{detail}）", self.message);
        }
        self
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConnectError {}
