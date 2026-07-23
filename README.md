# Paper Codex

Paper Codex 是一个单用户、本地优先的论文研究工作区。Rust/Axum 负责论文获取、PDF 抽取、SQLite/FTS、项目树和任务队列；React 提供浏览器界面；Codex App Server 通过 stdio 完成中文总结、证据化问答和论文关系发现。

应用是一个完整的本地 HTTP 服务：默认只监听 `127.0.0.1:3000`，同时提供 API 和网页，不会再开放其他应用端口。运行数据默认保存在仓库内、已被 Git 忽略的 `paper-workspace/`。

## 功能

- 接收论文名称、DOI、arXiv、网页链接或本地 PDF，生成中文结构化阅读页；
- 用可嵌套、可拖动的项目树组织论文，同一篇论文可加入多个项目；
- 在网页内渐进加载 PDF，不需要打开新标签页；
- 提供可切换、可归档的 Codex 对话，并支持论文、项目和全局上下文；
- 点击 Codex 引用可跳转并高亮 PDF 原文；页边解释卡支持拖动、缩小、隐藏和固定；
- 固定批注和坐标会持久化，论文版本变化后仍保留历史；
- 用论文、概念、方法、数据集和研究发现构建证据感知知识图谱；
- 文件树、知识图谱和 Codex 面板均可收起，桌面端宽度可拖动并保存。

## 环境要求

- Rust stable
- Node.js 20.19 或更高版本，推荐 Node.js 22 LTS
- SQLite 3
- 已安装并登录的 Codex CLI
- OpenSSL，用于生成随机会话密钥
- 支持 bcrypt 的 `htpasswd`，通常由 `apache2-utils` 或 `httpd-tools` 提供

Codex CLI 必须由运行 Paper Codex 的同一个操作系统用户完成登录。

## 构建

在仓库根目录执行：

```bash
cd web
npm ci
npm run build
cd ..

export CARGO_TARGET_DIR="$PWD/target"
cargo build --release --locked
```

构建完成后得到：

- 后端：`target/release/paper-codex`
- 前端：`web/dist/`

应用运行时直接从 `web/dist/` 提供网页，因此不需要单独启动前端服务。

## 配置

复制公开配置模板：

```bash
cp paper-codex.env.example paper-codex.env
```

生成登录密码的 bcrypt 哈希：

```bash
htpasswd -bnBC 12 "" '换成你的登录密码' | tr -d ':\n'
```

生成 JWT 密钥：

```bash
openssl rand -hex 32
```

把两个命令的输出分别填入 `paper-codex.env` 的：

```dotenv
PAPER_CODEX_PASSWORD_HASH=...
PAPER_CODEX_JWT_SECRET=...
```

`paper-codex.env` 已被 Git 忽略，不要提交它。其余常用配置如下：

| 变量 | 默认或示例 | 说明 |
| --- | --- | --- |
| `PAPER_CODEX_BIND` | `127.0.0.1:3000` | 本地监听地址，只接受回环地址 |
| `PAPER_CODEX_WORKSPACE` | `./paper-workspace` | PDF、索引、上下文和 SQLite 数据 |
| `PAPER_CODEX_STATIC_DIR` | `./web/dist` | 编译后的网页目录 |
| `PAPER_CODEX_CODEX_BIN` | `codex` | Codex CLI 可执行文件 |
| `PAPER_CODEX_CODEX_HOME` | 未设置 | 可选的 Codex 配置目录 |
| `PAPER_CODEX_DATABASE_URL` | 工作区内自动生成 | 通常不需要手动设置 |
| `PAPER_CODEX_MAX_UPLOAD_BYTES` | `104857600` | 单个上传文件的最大字节数 |

## 启动

必须从仓库根目录启动，这样模板中的相对路径才能正确解析：

```bash
set -a
. ./paper-codex.env
set +a
./target/release/paper-codex
```

看到监听日志后，访问：

```text
http://127.0.0.1:3000
```

另一个终端可以检查服务：

```bash
curl http://127.0.0.1:3000/api/health
```

预期返回类似：

```json
{"codex":true,"status":"ok","version":"0.1.0"}
```

进程在前台运行，按 `Ctrl+C` 可安全停止。

## 测试与开发

```bash
cd web
npm test -- --run
npm run typecheck
cd ..

export CARGO_TARGET_DIR="$PWD/target"
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

也可以执行一次完整发布检查：

```bash
scripts/build-release.sh
```

## 数据、备份与恢复

默认情况下，所有运行数据都位于 `paper-workspace/`。备份前先按 `Ctrl+C` 停止 Paper Codex，然后执行：

```bash
tar -czf paper-workspace-backup.tar.gz paper-workspace
```

恢复时确保 Paper Codex 已停止，把归档解压到仓库根目录：

```bash
tar -xzf paper-workspace-backup.tar.gz
```

重新启动后，数据库迁移会自动执行。备份文件可能包含论文原文、笔记和研究内容，应当按私人数据保存。

## 升级

1. 停止当前进程；
2. 备份 `paper-workspace/`；
3. 获取新版本代码；
4. 重新执行前端构建和 `cargo build --release --locked`；
5. 使用原来的 `paper-codex.env` 和 `paper-workspace/` 启动。

## 可选的独立运行目录

如果不想一直从源码目录运行，可以整理成以下结构：

```text
paper-codex-local/
├── paper-codex
├── paper-codex.env
├── paper-workspace/
└── web/
    └── dist/
```

复制文件后进入该目录，加载 `paper-codex.env`，再运行 `./paper-codex`。环境模板中的相对路径无需修改。

## 远程访问

开源版本只负责本地 HTTP 服务，不规定公网部署方式。若需要从其他设备访问，请在仓库之外自行选择 SSH 隧道、VPN 或反向代理，并自行负责传输加密和访问控制。

## 许可证

[MIT](LICENSE)
