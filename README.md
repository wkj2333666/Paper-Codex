# Paper Codex

Paper Codex 是一个面向个人研究者的开源论文阅读工作区。它使用 Rust/Axum 完成论文获取、PDF 文本抽取、SQLite/FTS 检索、项目树和任务持久化；React 提供浏览器界面；Codex App Server 通过标准输入输出生成中文总结、论文比较、问答与关系发现。

运行数据默认放在 Git 忽略的 `paper-workspace/`。同一篇论文只保存一份规范化记录，但可以加入多个项目。

## 功能

- 从论文名称、DOI、arXiv 地址、网页链接或本地 PDF 创建中文结构化阅读页；
- 用可嵌套的项目树组织论文，同一论文可属于多个项目；
- 通过“收件箱 → 项目 → 回收站”管理生命周期，永久删除前展示引用影响；
- 构建包含论文、概念、方法、数据集和研究发现的证据感知知识图谱；
- 在全局、项目或单篇论文范围内继续向 Codex 提问；
- 左侧项目树、知识图谱和右侧 Codex 抽屉均可独立收起。

## 环境要求

- Linux；
- Rust stable；
- Node.js 20 或更高版本；
- 已安装并登录的 Codex CLI；
- 可选：带 `dns.providers.cloudflare` 模块的 Caddy，用于公网 HTTPS。

## 本地开发

```bash
cd web
npm ci
npm test -- --run
npm run typecheck
npm run build
cd ..
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

复制 `deploy/paper-codex.env.example` 并设置密码哈希、JWT 密钥及本机路径，然后运行：

```bash
cargo run --release
```

服务只允许绑定回环地址，示例默认监听 `127.0.0.1:3000`。

## 部署示例

仓库内的部署文件采用以下示例值，请按实际环境替换：

- 公网入口：`https://paper.example.com:54321`；
- 程序与静态文件：`/opt/paper-codex`；
- 工作区：`/var/lib/paper-codex/workspace`；
- 备份目录：`/var/backups/paper-codex`；
- systemd 服务用户：`paper-codex`。

建议部署步骤：

1. 运行 `scripts/build-release.sh`，完成前后端测试、Rust 检查与生产构建。
2. 将二进制安装为 `/usr/local/bin/paper-codex`，并运行 `sudo scripts/sync-static.sh "$PWD/web/dist" /opt/paper-codex/web`。
3. 参考 `deploy/paper-codex.env.example` 创建 `/etc/paper-codex/paper-codex.env`，权限设为 `0600`。
4. 创建 `paper-codex` 系统用户，并安装 `deploy/paper-codex.service`。
5. 将 `deploy/Caddyfile.paper-codex` 合并到 Caddy 配置；使用 Cloudflare DNS-01 时，再安装 `deploy/caddy.service.d.conf` 并创建 `/etc/caddy/cloudflare.env`。

Cloudflare API Token 只需目标 Zone 的 `Zone:Read` 与 `DNS:Edit`。示例使用高端口 `54321`；如果启用 Cloudflare 代理，请先确认该端口是否受支持，否则将 DNS 记录设为 **DNS only**。DNS-01 签发证书不要求服务器开放 80/443。

Caddy 负责 TLS、安全响应头和回环反向代理，公网只显示应用登录页。不要把密码、JWT 密钥或 Cloudflare Token 提交到 Git；示例中的 `.env` 文件只包含占位符。

Codex 使用 `paper-codex` 服务用户自己的登录状态，App Server 仅通过子进程标准输入输出通信，不监听额外网络端口。

## 运维

```bash
systemctl status paper-codex caddy
journalctl -u paper-codex -u caddy --since today
curl http://127.0.0.1:3000/api/health
sudo scripts/backup.sh /var/backups/paper-codex
```

备份脚本会短暂停止应用，排除可重建的缓存、索引和 staging，再生成带 SHA-256 校验的 `tar.zst`。可通过 `PAPER_CODEX_WORKSPACE` 覆盖源工作区。

## License

[MIT](LICENSE)
