# AGENTS.md

面向 AI 编码代理的项目规范。

## 项目概述

Prompt Tool：本地优先的跨平台桌面提示词管理工具。前端 Vue 3 + TypeScript + Vite，桌面壳 Tauri 2（Rust），数据保存在本机 `data.json`。

## 常用命令

```bash
pnpm install                 # 安装前端依赖（包管理器为 pnpm，勿用 npm/yarn）
pnpm dev                     # Vite 开发服务器
pnpm tauri dev               # Tauri 桌面开发模式
pnpm build                   # vue-tsc 类型检查 + vite build（提交前必须通过）
pnpm test                    # Vitest 单元/组件测试
pnpm test:rust               # Rust 测试（cargo test，manifest 在 src-tauri/）
pnpm test:e2e                # Playwright 浏览器端 E2E（shim 模式）
pnpm test:e2e:fullstack      # 构建调试版后的真实 Tauri 冒烟测试
```

## 目录结构

- `src/`：Vue 前端（`views/` 页面、`components/` 组件、`lib/` 逻辑、`styles/` 样式）
- `src-tauri/`：Rust 侧（`src/` 命令实现、`capabilities/` 权限、`tauri.conf.json` 配置）
- `e2e/`：Playwright 用例；`e2e-webdriver/`：真实应用冒烟测试
- `docs/`、`tools/`、`.zcode/`：本地工作区文件，不入库（见 .gitignore）

## 约定

- 版本号三处同步：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 必须一致，改动版本时三处一起改
- 提交信息遵循 Conventional Commits（`feat/fix/docs/refactor/perf/test/build/ci/chore`），中文描述即可
- 发版：打 `v*` 标签推送后由 CI（`.github/workflows/build.yml`）自动测试、构建安装包并附加到 Release；发版前更新 CHANGELOG.md（Keep a Changelog 格式）
- 换行符由 `.gitattributes` 统一为 LF，勿提交带 CRLF 的文件
- 用户数据（`data.json`、剪贴板、图片）只存本机，任何改动不得把用户数据或凭据上传到第三方服务器
