# Paper Codex

[English](README.md)

Paper Codex 是一个本地优先的论文研究工作区，用于阅读论文、组织研究项目，并在不断增长的论文库中建立证据之间的联系。它把集成式 PDF 阅读器、结构化论文笔记、知识图谱和 Codex 对话整合到一个本地 HTTP 服务中。

> Paper Codex 面向单个研究者运行一个私有工作区，不是托管式多用户服务。

## 主要功能

- 通过论文标题、DOI、arXiv 编号、网页链接或本地 PDF 添加论文。
- 用可嵌套的研究项目组织相关论文；同一篇论文可以加入多个项目。
- 在工作区内阅读 PDF，并同步查看 Codex 的解释、引用、高亮和 overlay。
- 针对当前论文、项目或整个论文库向 Codex 提问。
- 在论文、概念、方法、数据集和研究发现之间探索知识图谱联系。
- 使用 SQLite/FTS 搜索抽取后的论文文本，不需要单独维护搜索服务。
- 自由收起和调整文件树、阅读器、知识图谱与 Codex 面板，适配笔记本和桌面屏幕。

## 工作原理

Paper Codex 是一个本地进程：

```text
浏览器 ──HTTP──> Rust/Axum 服务 ──> SQLite/FTS + 论文工作区
                         └────────> Codex CLI（stdio）
```

- **后端：** Rust、Axum、SQLite、PDF 抽取、索引、任务处理和 API。
- **前端：** React、Vite、集成式 PDF 渲染、Codex 对话界面和知识图谱可视化。
- **Codex 集成：** 服务通过本机 Codex CLI 启动 Codex app server，并把对话上下文限制在当前选中的研究材料内。
- **默认网络边界：** `127.0.0.1:3000`；开源服务不包含反向代理或公网域名配置。

## 环境要求

- Rust stable
- Node.js 20.19 或更高版本（推荐 Node.js 22 LTS）
- SQLite 3
- 可用的 Codex CLI，并使用运行 Paper Codex 的同一个操作系统用户完成认证
- `openssl` 和 `htpasswd`，用于生成本地密钥

## 从源码快速开始

克隆仓库并安装前端依赖：

```bash
git clone https://github.com/wkj2333666/Paper-Codex.git
cd Paper-Codex

cd web
npm ci
npm run build
cd ..
```

创建本地配置并生成两个密钥：

```bash
cp paper-codex.env.example paper-codex.env

# bcrypt 密码哈希
htpasswd -bnBC 12 "" 'replace-with-your-password' | tr -d ':\n'

# JWT 签名密钥
openssl rand -hex 32
```

把两个命令的输出分别填入 `paper-codex.env` 的 `PAPER_CODEX_PASSWORD_HASH` 和 `PAPER_CODEX_JWT_SECRET`。然后从仓库根目录构建并启动服务：

```bash
cargo build --release --locked

set -a
. ./paper-codex.env
set +a
./target/release/paper-codex
```

打开 <http://127.0.0.1:3000>。首次运行会创建 Git 忽略的 `paper-workspace/`，用于保存 PDF、抽取文本、索引、笔记和 SQLite 数据库。

## Codex 配置

请确保 Codex CLI 已安装，并使用启动 Paper Codex 的同一个操作系统用户完成认证。如果你的安装支持，通常可以使用：

```bash
codex login
```

当连接的 Codex 运行时支持时，Codex 面板可以选择对话作用域、模型、推理强度和服务速度。Paper Codex 不会把 Codex 凭据保存到仓库中。

## 数据与隐私

- 运行数据保存在 Git 忽略的 `paper-workspace/` 中。
- 上传的 PDF、抽取后的 Markdown、SQLite 索引、笔记和对话状态可能包含私密研究资料。
- `paper-codex.env` 包含密钥并且已被 Git 忽略，不要提交它。
- 最小服务只监听本机回环地址。如果需要从其他设备访问，请自行提供加密隧道、VPN 或反向代理，并制定访问控制策略。

## 本地开发

开发过程中可以运行以下检查：

```bash
cd web
npm ci
npm test -- --run
npm run typecheck
npm run build
cd ..

cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

完整的本地 release 检查可以使用：

```bash
scripts/build-release.sh
```

GitHub Actions 会在 push 和 pull request 时重复这些检查。推送 `v*` 标签后，Actions 会使用 [`cross`](https://github.com/cross-rs/cross) 构建以下 Linux 目标的 release 压缩包：`x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`、`armv7-unknown-linux-gnueabihf`、`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`、`riscv64gc-unknown-linux-gnu`、`powerpc64le-unknown-linux-gnu` 和 `s390x-unknown-linux-gnu`，用户不需要在本地安装每种目标的 linker。

## Release

Release 压缩包包含对应目标的后端二进制、编译后的网页资源和通用的 `systemd --user` 模板。每个 Release 都提供上述八种 Linux 目标的压缩包。树莓派 64 位系统使用 `aarch64-unknown-linux-gnu`，32 位系统使用 `armv7-unknown-linux-gnueabihf`。压缩包与具体部署环境解耦，不包含域名、Caddy 配置、机器路径或私有环境文件。

本地部署时，把 release 解压到项目目录，将 `PAPER_CODEX_STATIC_DIR` 指向其中的 `web/`，并把 `paper-workspace/` 与环境文件保留在 release 内容之外。`deploy/` 中的公开模板和 release workflow 是参考实现，请根据自己的操作系统和网络边界调整。

## 参与贡献

欢迎提交 issue、文档改进、bug 修复和功能 pull request。提交前请：

1. 保持改动聚焦，并说明对用户的影响；
2. 运行上面的前端和 Rust 检查；
3. 不要提交 `paper-workspace/`、`.runtime/`、`node_modules/`、构建产物、密钥或本地部署文件。

## 许可证

Paper Codex 使用 [MIT License](LICENSE) 发布。
