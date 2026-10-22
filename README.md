# MyDeployMate

MyDeployMate 是一个个人服务器部署桌面工具：在 Windows 上把项目的构建、上传和远程操作配置成流程，一键部署到 Linux 服务器。

新增服务器、项目、构建命令、上传路径或远程操作时，只改配置，不改程序源码。工具不绑定任何语言或框架，Git 拉取、Docker 操作、重启服务、健康检查等都是普通命令步骤。

## 主要功能

- **服务器与环境**：按 development / staging / production 等环境分组管理 Linux 服务器；生产等环境可设为受保护，发布前需键入环境名确认。
- **SSH 认证**：密码、私钥（OpenSSH 格式与 PuTTY `.ppk` v2/v3）、Windows OpenSSH Agent、Pageant；首次连接确认主机指纹，密钥变化时阻断。
- **项目与流程**：每个项目可有多条流程，流程由有序步骤组成。步骤有四类：本地命令（PowerShell、cmd、WSL）、SFTP 上传、SSH 远程命令、人工确认。
- **执行顺序与失败策略**：流程可选“步骤优先”或“服务器优先”；步骤失败时可停止、继续，或只跳过该服务器。
- **变量**：全局、项目、环境多级变量，支持 `${project.root}`、`${server.host}` 等内建变量；敏感值保存在 Windows 凭据管理器，界面和日志中遮蔽。
- **sudo**：按服务器或步骤设置提权方式，运行时密码经 stdin 输入，不出现在远端进程列表中。
- **预览与执行**：执行前预检并逐台展示解析后的命令；运行时实时查看日志，可取消、可人工确认。
- **记录与配置**：执行历史可筛选和再次执行；配置可导出与导入（导入前展示差异并自动备份）。

范围说明：只做本地命令、SFTP 和 SSH 三种执行能力，不是完整的 CI/CD 平台；当前版本面向单人、单机使用，同一时间只有一个运行。

## 仓库结构

本仓库包含同一产品的两套实现，共用同一套需求。

| 目录 | 实现 | 状态 |
| --- | --- | --- |
| [`MyDeployMate-Tauri/`](MyDeployMate-Tauri/) | Tauri 2 + Rust 后端 + Vue 3 / TypeScript 前端 | 开发中 |
| 待定 | Slint 实现，由合作者开发 | 尚未上传 |

其他文件：

| 路径 | 内容 |
| --- | --- |
| `AGENTS.md` / `CLAUDE.md` | 给 AI 助手（及人）的仓库工作说明 |
| `scripts/check-commit-msg.sh` | 提交信息校验脚本 |
| `.githooks/` | Git 钩子（`git config core.hooksPath .githooks` 启用） |
| `.github/workflows/` | CI：Rust 格式、clippy、测试，前端 lint、类型检查、测试、构建，PR 标题校验 |

### Tauri 实现

位于 [`MyDeployMate-Tauri/`](MyDeployMate-Tauri/)，详见其 [README](MyDeployMate-Tauri/README.md)。

- `crates/core`：领域模型、字段校验、变量解析与脱敏、任务展开与预检、运行状态机，纯逻辑、不做 IO
- `crates/store`：配置存储（`%APPDATA%\MyDeployMate\workspace.json`），原子保存、备份、文件锁、版本迁移
- `crates/exec`：执行器与调度器（本地命令、SSH、SFTP），基于 russh / russh-sftp
- `src-tauri/`：Tauri 外壳；`src/`：Vue 前端
- `docs/`：需求、交互、架构、UI 规范和实施计划
- `demo/`：最早的交互原型（Python，全部模拟）

当前进度（共 9 个阶段，详见[实施计划](MyDeployMate-Tauri/docs/可行性分析与实施计划.md)第 5 节）：

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| 0 | 评审定稿与技术验证 | 基本完成（部分 Windows 验证待补） |
| 1 | 工程脚手架与领域核心 | 完成 |
| 2 | 变量解析、预检与任务展开 | 完成 |
| 3 | 配置管理界面 | 进行中（基础组件与应用外壳已完成） |
| 4 | 执行引擎与本地命令 | 未开始 |
| 5 | SSH 连接、主机信任与远程命令 | 未开始 |
| 6 | SFTP 上传与 sudo | 未开始 |
| 7 | 历史、模板、导入导出 | 未开始 |
| 8 | 非功能、打包与整体验收 | 未开始 |

### Slint 实现

由合作者开发，代码暂未上传。上传后会放在仓库根目录下的独立目录中，与 `MyDeployMate-Tauri/` 并列，并在此补充说明。

## 文档

需求与设计文档目前放在 `MyDeployMate-Tauri/docs/`，两套实现共同遵循：

- [软件需求规格说明书](MyDeployMate-Tauri/docs/软件需求规格说明书.md)：做什么、业务规则、验收用例
- [交互设计说明书](MyDeployMate-Tauri/docs/交互设计说明书.md)：页面、弹窗和操作反馈
- [架构设计文档](MyDeployMate-Tauri/docs/架构设计文档.md)
- [UI 设计规范](MyDeployMate-Tauri/docs/UI设计规范.md)
- [可行性分析与实施计划](MyDeployMate-Tauri/docs/可行性分析与实施计划.md)：技术选型、已确认决策、分阶段任务
- [提交规范](MyDeployMate-Tauri/docs/提交规范.md)

## 快速开始（Tauri 实现）

前置：Windows、Rust（MSVC 工具链）、Node.js 22.22 及以上、WebView2（Windows 11 自带）。

```powershell
cd MyDeployMate-Tauri
npm install
npm run tauri dev      # 启动桌面应用
npm run tauri build    # 生成 NSIS 安装包
```

## 参与开发

- 提交信息使用 Conventional Commits：`type(scope): subject`，subject 用英文，正文可用中文。完整规则见[提交规范](MyDeployMate-Tauri/docs/提交规范.md)。
- 提交前本地跑通 CI 中的检查，命令见 [`AGENTS.md`](AGENTS.md)。
- 通过分支和 Pull Request 合入 `main`。
