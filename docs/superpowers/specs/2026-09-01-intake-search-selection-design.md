# Paper Codex 搜索—选择—导入设计

日期：2026-09-01

## 1. 背景与根因

首页输入框承诺接受“论文名称、链接、DOI 或 arXiv”，但当前 `POST /api/intake` 会把所有输入直接创建为导入任务。`Acquirer` 对自由文本只向 Crossref 请求一条结果，不校验标题相似度，然后尝试该结果的 Crossref PDF 或按 DOI 查询 OpenAlex。这个实现把“搜索”和“导入”错误地合并成了一步。

由此产生两个用户可见问题：

- 模糊查询（例如 `jepa`）不会展示 I-JEPA、V-JEPA 等候选，而会静默采用 Crossref 第一条结果；
- 完整参考文献也可能被 Crossref 错配。错误结果没有 PDF 时，系统却显示“metadata resolved, but no downloadable PDF was found”，让用户误以为正确论文已被定位。

OpenReview 还会对 Paper Codex 服务器的无头 HTTP 请求返回 `403 Challenge verification required`。浏览器可通过 JavaScript 和验证 Cookie，而 Rust `reqwest` 客户端不能。这个错误与论文权限、Caddy 或 Paper Codex 登录无关，且不应被折叠为通用“没有 PDF”。

## 2. 已确认的产品决策

### 2.1 自由文本始终先展示候选

以下输入一律进入论文搜索，不自动导入：

- 简称、主题词和方法名，例如 `jepa`；
- 完整或部分论文标题；
- 从参考文献复制的自然语言书目信息。

即使搜索只返回一个高相似度结果，也必须先展示候选并由用户确认。系统不得以“高置信度”为由静默正式导入。

### 2.2 结构化标识符和 URL 直接导入

以下输入继续直接创建 intake 任务：

- DOI 或 `doi:` 标识；
- arXiv ID、`arxiv:` 标识或 arXiv URL；
-普通网页 URL、OpenReview URL 或 PDF URL。

URL 直接导入失败时保留完整、可读的来源链错误。系统不把 URL 自动改写为自由文本搜索，避免用户明确指定的来源被替换。

### 2.3 候选确认后才创建导入任务

搜索本身不创建 intake 任务，也不在“最近失败”区域产生红色任务卡。用户点击候选的“导入并分析”后，系统才创建正式导入任务。

如果候选属于已有正式论文，系统不重复下载，而是直接打开论文或将其关联到用户所选项目。

## 3. 目标与非目标

### 3.1 目标

- 让首页输入框兑现“按论文名称搜索”的界面承诺；
- 复用已有 Crossref、arXiv、OpenAlex provider、去重缓存和任务队列；
- 防止低相似度 Crossref 结果被当作已解析论文；
- 让用户在导入前看到标题、作者、年份、来源和全文可用性；
- 候选选中后使用已经确认的元数据，不再按原始标题重新解析；
- 尝试多个已知下载来源，并保留每一次失败的来源、HTTP 状态和原因；
- 保持项目选择、收件箱、正式分析和知识图谱流程不变。

### 3.2 非目标

- 不抓取 Google、Brave 等搜索结果 HTML 作为无凭据的长期后端接口；
- 不绕过 OpenReview 的浏览器挑战或验证码；
- 不保证每篇论文都有开放 PDF；
- 不自动导入自由文本搜索结果；
- 不在首版增加必须付费或必须配置 API key 的新论文服务；
- 不把网页或摘要伪装成正式 PDF 全文。

## 4. 总体架构

```text
首页输入
  ├─ DOI / arXiv / URL
  │    └─ POST /api/intake
  │         └─ 现有 TaskEngine 导入与分析
  └─ 自由文本
       └─ POST /api/intake/search
            └─ ResearchService::discover
                 ├─ Crossref
                 ├─ arXiv
                 ├─ OpenAlex
                 ├─ 规范化、去重、排序
                 └─ 候选列表 + provider 状态
                      └─ 用户选择
                           └─ POST /api/intake/candidates/{work_id}/import
                                └─ 候选元数据快照 + 下载来源链
                                     └─ TaskEngine 导入与分析
```

`ResearchService` 继续作为所有学术检索 provider 的唯一聚合边界。新增一个不要求项目、对话或消息 ID 的发现方法；现有项目对话检索在它之上继续记录 `literature_search_runs` 和项目候选，避免维护两套 provider 调用与去重逻辑。

## 5. 输入分类与 API

### 5.1 服务端分类是唯一事实来源

前端可以为了即时提示预判输入类型，但最终分类必须由服务端现有 `classify_input` 完成。避免 TypeScript 和 Rust 对 DOI、arXiv URL 或特殊 URL 的判断逐渐分叉。

### 5.2 `POST /api/intake`

保留现有请求：

```json
{
  "source": "arxiv:2301.08243",
  "project_id": null
}
```

如果 `source` 被分类为 DOI、arXiv 或 URL，返回原有入队结果：

```json
{
  "kind": "enqueued",
  "task_id": "task-id"
}
```

如果 `source` 是自由文本，不再入队，返回 `422 Unprocessable Entity` 和稳定错误码 `intake_search_required`。前端正常工作流会直接调用搜索端点；这个保护用于阻止旧客户端继续把自由文本误建为导入任务。

### 5.3 `POST /api/intake/search`

请求：

```json
{
  "query": "jepa",
  "limit": 12
}
```

`limit` 默认 12，允许范围 1–25。响应：

```json
{
  "query": "jepa",
  "state": "partial",
  "providers": {
    "arxiv": {"state": "completed", "hits": 12, "error": null},
    "crossref": {"state": "completed", "hits": 12, "error": null},
    "openalex": {"state": "failed", "hits": 0, "error": "HTTP 429: rate limit exceeded"}
  },
  "results": [
    {
      "work": {"id": "work-id", "title": "..."},
      "providers": ["arxiv", "crossref"],
      "best_rank": 1,
      "match": {"score": 0.93, "title_exact": false},
      "fulltext": {"state": "available", "source_count": 2}
    }
  ]
}
```

只要至少一个 provider 成功，响应就是 `completed` 或 `partial`；全部 provider 失败时返回 `503`，body 仍包含每个 provider 的错误。没有命中不是异常，返回空 `results`。

### 5.4 `POST /api/intake/candidates/{work_id}/import`

请求：

```json
{
  "project_id": null
}
```

服务端必须从 `discovered_works` 读取候选，不能接受客户端上传标题、DOI 或 PDF URL。响应为以下 tagged union：

```json
{"state": "enqueued", "task_id": "task-id"}
```

```json
{"state": "existing", "paper_id": "arxiv:2301.08243"}
```

候选没有 DOI、arXiv ID、来源 URL和任何 PDF 来源时返回 `409 candidate_not_importable`。

## 6. 发现、去重与排序

### 6.1 抽取无项目依赖的发现核心

新增 `ResearchService::discover(query, cancel)`：

- 并行调用已配置 provider；
- 记录每个 provider 的完成、失败或取消状态；
- 通过 `ResearchStore::upsert_work` 规范化并持久化命中；
- 合并相同 DOI、arXiv ID 或 OpenAlex ID；
- 返回去重后的 work、provider 列表、各 provider 最佳名次和原始元数据；
- 不创建项目、对话、检索轮次或候选关系。

现有 `search_with_cancel` 调用 `discover`，再把结果写入 `literature_search_runs`。这样首页与项目研究使用完全相同的 provider 行为。

### 6.2 排序只影响展示，不授权自动导入

候选排序依次考虑：

1. 规范化标题是否与查询中的标题完全相等；
2. 查询词在标题中的覆盖率；
3. 多 provider 是否共同命中；
4. provider 内最佳名次；
5. 作者与年份是否与参考文献文本一致。

排序分数只用于候选顺序。低分结果可以放在列表后部或标记“匹配较弱”，但系统不得因此自动导入，也不得把任意第一条结果描述成“元数据已解析”。

### 6.3 参考文献文本

首版不使用模型解析参考文献。排序器从原始文本中保留数字年份和词项，并以“候选标题是否是输入规范化文本的连续子串”识别完整参考文献中的标题。这样可处理包含作者、版本、期刊和页码的输入，同时保持结果确定、可测试。

## 7. 候选导入与下载来源链

### 7.1 导入任务携带已确认元数据

扩展 `IngestInput`，增加可选的服务端候选快照。旧任务反序列化时该字段默认为空。快照包含：

- `work_id` 和 canonical identity；
- 标题、作者、年份；
- DOI、arXiv ID；
- landing/source URL；
- 按优先级排列的下载来源。

存在候选快照时，`TaskEngine` 不得再次以原始自由文本调用 Crossref。正式 Paper 记录使用快照元数据，实际下载成功的 URL 作为 revision source。

### 7.2 下载优先级

候选导入的下载来源顺序为：

1. provider 已返回并标记为 PDF 的 URL；
2. arXiv canonical PDF；
3. DOI 解析得到的 Crossref PDF；
4. OpenAlex 已知开放获取 PDF。

相同规范化 URL 只尝试一次。每个来源继续使用现有单 URL 的瞬时错误重试；某个来源最终失败后再进入下一个来源，而不是对第一条来源无限重试。

### 7.3 错误链

所有来源失败时，任务错误必须包含结构化尝试列表：

```json
{
  "code": "all_pdf_sources_failed",
  "message": "已定位论文，但所有 PDF 来源均失败",
  "attempts": [
    {
      "provider": "openreview",
      "url": "https://openreview.net/pdf?id=...",
      "status": 403,
      "reason": "来源要求浏览器完成 Challenge verification"
    }
  ]
}
```

数据库 `tasks.error` 保留适合旧客户端显示的中文摘要；任务事件 payload 同时保留结构化字段供新前端展示。错误不得包含认证 header、Cookie、API key 或完整响应 HTML。

### 7.4 OpenReview 403

OpenReview 的 `403` 且响应包含 `Challenge verification required` 时映射为稳定原因 `browser_challenge_required`。系统继续尝试候选中其他开放 PDF 地址；没有其他地址时明确提示：

> 已找到 OpenReview 记录，但该来源要求浏览器验证，服务器无法自动下载。可在浏览器中打开来源，或提供可直接下载的 PDF。

系统不尝试自动执行验证码，也不把 HTML 挑战页保存为 PDF。

## 8. 前端交互

### 8.1 首页输入框

提交时显示“正在搜索论文来源…”。服务端确认是直接标识符/URL时维持当前入队体验；自由文本则打开输入框下方的候选区域。

候选区域包含：

- 原始查询；
- provider 状态摘要；
- 标题、作者、年份；
- 命中 provider；
- `可获取 PDF`、`可能可获取` 或 `当前未发现开放全文`；
- “打开来源”和“导入并分析”操作。

输入新查询会替换旧候选；关闭候选不会创建或删除任务。

### 8.2 部分失败

如果 arXiv/Crossref 成功而 OpenAlex 429，候选仍正常显示，并出现紧凑提示“部分来源暂不可用：OpenAlex 限流”。用户可展开查看 provider 错误，不用面对整页失败。

### 8.3 导入状态

点击候选后，该候选按钮进入“正在加入”状态。任务创建成功后复用现有任务卡和事件流。任务失败卡新增可展开的来源尝试详情，但继续支持单条关闭和“清除失败记录”。

### 8.4 无结果

空结果显示：

> 没有找到匹配论文。可以修改查询，或粘贴 DOI、arXiv、论文网页/PDF URL。

这只是搜索空结果，不创建失败任务。

## 9. 数据兼容与安全

- 不新增必须的数据表；复用 `discovered_works`，下载候选列表保存在 work 的来源元数据和 task 输入快照中；
- 新增字段均使用 `serde(default)`，现有 queued/failed task 可继续读取；
- API 只允许按服务端 `work_id` 导入，客户端不能注入任意本地路径；
- URL 下载继续受大小限制、重定向上限和 `%PDF-` 魔数校验；
- provider 响应和下载错误不记录密钥、Cookie 或 Authorization header；
- 自由文本搜索不产生正式论文、项目关联或分析任务。

## 10. 测试与验收

所有测试、类型检查、构建和发布仅由 GitHub CI 执行；本地不运行测试或构建。

### 10.1 Rust

- `jepa` 被分类为自由文本，`POST /api/intake` 不创建任务；
- DOI、arXiv 和 URL 仍直接入队；
- 首页发现复用三个 provider，单个 provider 429 时返回 partial 和其他结果；
- 同一 DOI/arXiv 的多来源结果只展示一次，并保留 provider 列表；
- 完整参考文献中的正确标题排在无关 Crossref 第一条之前；
- 候选导入读取服务端 work，携带元数据快照，且不再次执行标题解析；
- 第一 PDF 来源 403、第二来源成功时任务完成并记录实际来源；
- OpenReview challenge 映射为 `browser_challenge_required`；
- 所有来源失败时保留结构化尝试列表，且错误中不出现敏感 header。

### 10.2 前端

- 自由文本提交后展示候选而不是活动/失败任务卡；
- `jepa` 可同时显示多篇候选；
- direct DOI/arXiv/URL 继续进入任务状态；
- partial provider 状态不隐藏成功结果；
- 点击候选只提交 `work_id` 和 `project_id`；
- 无结果、全部 provider 失败、候选导入失败分别显示不同状态；
- 候选区域支持关闭、重新搜索和打开来源；
- 现有任务关闭、清空失败、上传 PDF 和项目选择回归测试保持通过。

### 10.3 端到端验收

1. 输入 `jepa`，看到多个去重候选，不生成失败 task；
2. 选择一个具有 arXiv PDF 的候选，任务进入现有分析流程；
3. 输入完整 LeCun 参考文献，看到标题相关候选或明确空结果，不再静默匹配 `NTS 2022/62`；
4. 对 OpenReview-only PDF，看到浏览器挑战的具体原因；若存在备用 PDF，自动继续并成功导入；
5. 输入 `arxiv:2301.08243`，保持一键直接导入。

## 11. 发布与迁移

- 本次不自动重跑历史失败 task；历史记录可由用户关闭；
- 部署后用 `jepa`、完整 LeCun 参考文献和一个 arXiv ID 做浏览器端验收；
- GitHub CI 完成 Rust 测试、前端测试、类型检查、构建和发布；
- 仅下载 GitHub Release 产物部署，不在本机构建。
