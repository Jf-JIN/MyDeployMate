# AGENTS.md

给在本仓库工作的 AI agent 的说明。人也可以照着做。

## 项目

MyDeployMate：个人服务器部署桌面工具，Tauri 2 + Rust + Vue 3 / TypeScript，只面向 Windows。

- 所有代码和文档放在 `MyDeployMate-Tauri/` 下。
- 需求、交互和分阶段计划在 `MyDeployMate-Tauri/docs/`，实现前先读对应需求编号。
- 规划中的结构：Cargo workspace（`crates/core`、`crates/store`、`crates/exec`、`src-tauri`）+ Vue 前端。`core` 是纯逻辑、不做 IO，业务校验只写在 Rust 里，前端不重复实现。

## 界面文字（多语言）

界面支持简体中文和 English（NFR-05、ARC-38），**不要在代码里写死界面文字**：

- 前端：文字写进 `MyDeployMate-Tauri/src/i18n/zh-CN.ts`，组件里用 `useT()`，组件外用 `translate()`；带标记的句子用 `<i18n-t>`。新增或修改文字时同时写 `en-US.ts` 的英文：它的类型就是 zh-CN 的结构，缺少 key 时 typecheck 失败，占位符不一致时测试失败。
- 后端：面向用户的提示一律返回 `mdm_core::Message`（code + params），不拼中文句子；第三方原文放 `detail`，用户内容用 `Message::text`。新增 code 后在 `zh-CN.ts` 的 `backend.*` 下补中文，占位符与参数同名（有测试检查）。前端用 `messageText` / `errorText` 渲染。
- 不翻译：用户填写的内容、命令输出和日志正文。
- 检查：ESLint `@intlify/vue-i18n/no-raw-text` 检查模板文字；`scripts/check-ui-text.py` 检查前端和 Rust 非测试代码里的中文（注释、测试、`src/views/dev/` 除外）。

## 提交

**提交前必须读 [`MyDeployMate-Tauri/docs/提交规范.md`](MyDeployMate-Tauri/docs/提交规范.md)。** 要点：

- 格式 `type(scope): subject`，subject 用英文祈使句、首字母小写、不加句号，标题不超过 72 字符；正文可用中文。
- type：`feat` `fix` `refactor` `perf` `test` `docs` `style` `build` `ci` `chore` `revert`
- scope：`core` `store` `exec` `tauri` `ui` `design` `spike` `test-env` `demo` `deps` `repo`
- 破坏性变更加 `!` 并写 `BREAKING CHANGE:`；关联需求写 `Refs: SRV-01`。
- 提交前运行 `scripts/check-commit-msg.sh --range origin/main HEAD`，或先执行 `git config core.hooksPath .githooks` 启用钩子。

## 提交前的本地检查

CI（`.github/workflows/ci.yml`）会跑以下检查，提交前请在本地先跑通：

```bash
cd MyDeployMate-Tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

在仓库根目录运行 `python3 scripts/check-ui-text.py`，确认没有写死的中文界面文字。

前端在 `MyDeployMate-Tauri/package.json` 里提供下列脚本，CI 会依次运行已定义的那些（未定义的跳过）：

| 脚本 | 要求 |
| --- | --- |
| `lint` | ESLint（及 Prettier 检查），有问题时非零退出，不自动修复 |
| `typecheck` | `vue-tsc --noEmit` |
| `test` | 单次运行的单元测试（如 `vitest run`），不能是 watch 模式 |
| `build` | 生产构建（`vite build`） |

`spikes/` 是阶段 0 的一次性验证代码，不在 CI 检查范围内。
