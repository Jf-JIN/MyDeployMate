# AGENTS.md

给在本仓库工作的 AI agent 的说明。人也可以照着做。

## 项目

MyDeployMate：个人服务器部署桌面工具，Tauri 2 + Rust + Vue 3 / TypeScript，只面向 Windows。

- 所有代码和文档放在 `MyDeployMate-Tauri/` 下。
- 需求、交互和分阶段计划在 `MyDeployMate-Tauri/docs/`，实现前先读对应需求编号。
- 规划中的结构：Cargo workspace（`crates/core`、`crates/store`、`crates/exec`、`src-tauri`）+ Vue 前端。`core` 是纯逻辑、不做 IO，业务校验只写在 Rust 里，前端不重复实现。

## 提交

**提交前必须读 [`MyDeployMate-Tauri/docs/提交规范.md`](MyDeployMate-Tauri/docs/提交规范.md)。** 要点：

- 格式 `type(scope): subject`，subject 用英文祈使句、首字母小写、不加句号，标题不超过 72 字符；正文可用中文。
- type：`feat` `fix` `refactor` `perf` `test` `docs` `style` `build` `ci` `chore` `revert`
- scope：`core` `store` `exec` `tauri` `ui` `design` `spike` `test-env` `demo` `deps` `repo`
- 破坏性变更加 `!` 并写 `BREAKING CHANGE:`；关联需求写 `Refs: SRV-01`。
- 提交前运行 `scripts/check-commit-msg.sh --range origin/main HEAD`，或先执行 `git config core.hooksPath .githooks` 启用钩子。
