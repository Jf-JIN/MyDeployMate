# MyDeployMate

个人服务器部署桌面工具（Tauri 2 + Rust + Vue 3 / TypeScript），只面向 Windows。目前处于阶段 1：工程骨架、领域模型、字段校验和配置存储已就绪，界面只有导航骨架。

## 目录

| 路径 | 内容 |
|---|---|
| `crates/core`（`mdm-core`） | 配置模型、ID、运行状态词汇、字段校验；纯逻辑，不做 IO |
| `crates/store`（`mdm-store`） | 数据目录 `%APPDATA%\MyDeployMate`、`workspace.json` 原子保存与备份、文件锁、版本迁移 |
| `crates/exec`（`mdm-exec`） | 执行器与调度器，阶段 4 起实现 |
| `src-tauri/` | Tauri 外壳：命令、单实例、窗口与安装包配置 |
| `src/` | Vue 前端；`src/ipc/bindings/` 是从 Rust 类型生成的 TS 类型，不要手改 |
| `src/styles/tokens.css` | 设计变量（《UI 设计规范》第 2–4 节） |
| `docs/` | 需求、交互、架构、UI 规范、实施计划；`docs/original/` 为 v0.1 PDF |
| `test-env/` | Docker SSH 测试目标（集成测试用） |
| `spikes/` | 阶段 0 一次性验证程序，不在 CI 范围 |
| `demo/server_manager_demo.py` | 原交互原型（Python 标准库，全部模拟），`python demo/server_manager_demo.py` |

## 开发

前置：Rust（MSVC 工具链）、Node.js 22.22 及以上（或 24 LTS）、WebView2（Windows 11 自带）。

```powershell
cd MyDeployMate-Tauri
npm install
npm run tauri dev      # 启动桌面应用（前端热更新）
npm run tauri build    # 生成 NSIS 安装包
```

提交前的检查（与 CI 相同）：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace      # 同时重新生成 src/ipc/bindings，有变化时一并提交
npm run lint
npm run typecheck
npm test
npm run build
```

修改 `crates/core` 或 `src-tauri` 中带 `#[ts(export)]` 的类型后，运行 `cargo test` 更新 TS 绑定。新增 Tauri 命令时，同时在 `src/ipc/invoke.ts` 的 `Commands` 中登记。
