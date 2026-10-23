use std::path::PathBuf;

use mdm_core::ValidationReport;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("无法访问 {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("数据目录 {0} 已被另一个 MyDeployMate 进程占用")]
    Locked(PathBuf),
    #[error("配置已被保存为修订 {found}，当前草稿基于修订 {expected}，请重新加载")]
    RevisionConflict { expected: u64, found: u64 },
    #[error("workspace.json 在本程序之外被修改过，请重新加载后再保存")]
    ExternalChange,
    #[error("{path} 不是有效的配置文件: {message}")]
    Corrupt { path: PathBuf, message: String },
    #[error("配置版本 {found} 高于本程序支持的版本 {supported}，请升级 MyDeployMate")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("配置从版本 {from} 迁移失败: {message}")]
    Migration { from: u32, message: String },
    #[error("运行记录数据库出错：{0}")]
    Database(String),
    #[error("运行记录数据库版本 {found} 高于本程序支持的版本 {supported}，请升级 MyDeployMate")]
    HistoryVersion { found: u32, supported: u32 },
    #[error("另一个运行尚未结束，同一时间只能有一个运行")]
    RunActive,
    #[error("{0} 的服务器身份已有记录，只能通过信任管理显式更新")]
    TrustConflict(String),
    #[error("Windows 凭据管理器出错：{0}")]
    Vault(String),
    #[error("配置校验未通过（{} 个问题）", .0.issues.len())]
    Invalid(ValidationReport),
}

pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> StoreError {
    let path = path.into();
    move |source| StoreError::Io { path, source }
}
