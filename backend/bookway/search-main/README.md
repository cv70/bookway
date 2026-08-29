# Search Main 搜索主服务

## 职责

搜索产品策略的主入口。它把用户查询编排为可演进的流水线：

`规范化 -> 精确召回 + 受控同义扩展 -> 混合/去重 -> 相关性 + pCTR/pCVR/pWEGU/route_completion_rate 重排 -> 稳定分页 -> 首页场景广告 eCPM 混排`

`bbs-search` 继续拥有底层关键词检索、OpenSearch/PIT 与事实源降级、内容可见性过滤，以及每一路召回的私有游标；其 OpenSearch 命中会先由 `bbs-link` 的紧凑公开摘要批量回读，OpenSearch 因此只提供候选与排序而不成为内容事实源。`knowledge-catalog` 作为公共资源目录候选源接入同一 pipeline，Search Main 只返回已发布资源的 provider、URL、license、version、citation 和 topics，不复制正文或绕过目录授权。Gateway 先根据社交关系注入可信查看者和屏蔽作者集合，Search Main 会原样传递给每一个 `bbs-search` 请求，不能由客户端覆盖。

## 查询与排序策略

路线搜索结果保留来自权威公开摘要的 `route_actions`，包括稳定行动节点 ID 和声明装备；客户端可以把行动/装备命中继续传递给路线执行或场景商业化，而不会依赖索引中的旧节点数据。
搜索请求可选携带 `route_id`、`action_node_id` 和 `scene_equipment` 结构化上下文；这组字段会绑定分页游标，并在 BBS Search 的索引候选与公开事实回读阶段共同过滤，避免只因标题或正文命中就返回错误路线节点。

- 将空白和常见中英文分隔符规范为 canonical query，限制为 1--100 个字符。
- 每次搜索都有精确 lexical recall；仅命中受控词典时增加一条扩展 recall，例如 `跑步 -> 慢跑/晨跑/夜跑`、`阅读 -> 读书/书单/主题阅读`。`@用户` 和 `#话题` 等身份/主题查询不扩展，最多附加 6 个词并始终保留原始精确召回。扩展故障只标记 `degraded`，精确召回故障则向上游报错。
- `all` 搜索会同时召回内容/路线/用户/话题和公共资源；`resources` 搜索只请求 `knowledge-catalog`，不会把资源查询打到 BBS。资源结果与内容结果共用稳定分页、去重、曝光归因和轻量重排。
- 相同 `(result_type, id)` 的候选会合并，未展示的候选保存在服务端并按标题完整命中、标题词覆盖、`#` 话题/`@` 用户/路线/资源意图和领域匹配做确定性轻量重排。
- 对已登录用户，Search Main 每轮召回合并后批量读取 Feature Main 的候选特征，在 35ms 有界预算内加入 pCTR、pCVR、pWEGU 和 `route_completion_rate`；pWEGU（行动完成）与路线完成度占行为项最大权重。特征服务不可用时保留词法结果并返回 `degraded=true`，不会阻断精确搜索。
- 当请求携带完整路线、行动节点和场景装备上下文时，首页按 `pkg/commercial-mix` 的密度调度（负载 ~15%、至少三个有机结果）向 `ad-main` 请求相应槽数的决策，并按广告 eCPM 竞价序复核上下文后混入；被挤出的尾部有机结果按原序回到服务端 pending 缓冲，分页不受广告影响。广告决策单独注册频控/曝光租约，广告不写入有机搜索曝光归因或分页候选。`ad-main` 在 25ms 内不可用时仅标记 `degraded`。
- 该层仍是明确的模型接入点，而不是宣称已经存在 ML 排序模型；后续可在这里加入向量召回、实验和离线评测。

## 游标与会话

公开游标格式为 `sm1-{fingerprint}-{uuid}`，只包含查询、搜索类型、可信查看者和排序后的排除作者集合的指纹；游标不能跨查询、用户、搜索类型或可见性集合复用。

会话保存各 recall 的 `bbs-search` 私有游标、候选 pending 队列、已见结果和退化状态，TTL 为 5 分钟。它不会把 OpenSearch PIT 或底层 `bbs-search` token 返回给客户端。每次从 pending 队列返回帖子或路线前，Search Main 会批量回读 `bbs-link` 的权威公开摘要；已限制、删除或不再公开的 ID 会被丢弃，展示字段会以当前摘要重建，异常或不可信的摘要批次会失败关闭。用户和话题候选不经过内容回读。`0028_search_main_sessions.sql` 提供 PostgreSQL 存储；`STORAGE_MODE=memory` 用于本地演示。只接受当前 `sm1-...` 会话游标，其他版本均以 `INVALID_ARGUMENT` 拒绝。

会话过期返回 `FAILED_PRECONDITION`，无效或不匹配游标返回 `INVALID_ARGUMENT`，存储和底层可用性故障返回 `UNAVAILABLE`。

## 查询改写版本

`0040_search_query_rewrite_versions.sql` 将词典从服务内硬编码规则升级为版本化配置。每个版本包含受限 trigger 和扩展词集合；`search_query_rewrite_active` 是一个原子单例指针，运行实例最多每 60 秒刷新一次。新搜索会使用当前活动版本，已创建的分页会话会保留自身的召回词和版本，不会因为切换而重复、漏项或改变后续页。

上线新版本时先以 `draft` 写入规则并做离线召回质量检查，将版本改为 `ready` 后调用：

```sql
SELECT activate_search_query_rewrite('lifestyle-v2');
```

该函数拒绝非 `ready` 或无规则版本；回滚只需把活动指针切回上一已验证版本。运行时还会重新规范化、去重、限制 trigger/扩展词长度和总扩展预算；配置读取失败或无效时保留上次有效词典（首次启动回退内置 `builtin-v1`）并标记响应 `degraded=true`。搜索曝光会记录 `query_rewrite_version`，但不会保存查询明文，供之后以可信交互做版本级质量评估。

`job/search-evaluator` 在 PostgreSQL 中运行，固定评估窗口后输出并写入匿名快照：

```bash
SEARCH_EVAL_START_AT=2026-08-01T00:00:00Z \
SEARCH_EVAL_CUTOFF_AT=2026-08-08T00:00:00Z \
cargo run -p bookway-search-evaluator
```

任务只读取未降级页，并只计入 Search Main 已验证、且服务端 `received_at` 落在每个搜索页标签窗口（默认 168 小时）内的事件。它按改写版本和结果类型计算渲染、点击、查看、高意图、负反馈、净效用与可见 NDCG@5；没有查询文本、用户 ID 或结果 ID 会进入快照。`SEARCH_EVAL_MIN_RENDERED_ITEMS` 默认 500，低于阈值时状态为 `insufficient_data`，不得用于升级或切换词典。为可重复比较，必须固定 `SEARCH_EVAL_START_AT`、`SEARCH_EVAL_CUTOFF_AT` 和 `SEARCH_EVAL_LABEL_WINDOW_HOURS`。它是观察性指标，不提供反事实结论，也不会自动激活版本。

## 搜索归因

每个返回给客户端的结果页都有独立 UUID `request_id`。登录请求在返回前将可信查看者、客户端会话、查询 hash、结果 ID 和原始零基位置写入 `search_exposures` / `search_exposure_items`；匿名请求只返回 request ID，不写入共享的合成身份，也不能产生可验证归因。User Event 通过内部 `ValidateAttributions` 批量 RPC 校验搜索点击、点赞、收藏、隐藏、知识收集和路线加入。归因记录在 30 天后失效，写入路径每次至多清理 1,000 个过期页。伪造或过期归因会被拒绝；搜索服务暂时不可用时，User Event 仅保留无归因反馈，避免污染训练和质量指标。

## 可观测性与边界

完成日志只记录稳定查询 hash、召回变体数、调用数、候选数、耗时和降级状态，不记录搜索明文。Search Main 的 gRPC 入口对搜索和联想统一执行 140ms 预算，为 150ms P99 留出传输余量；任一等待中的下游请求会随之取消。精确召回或内容回读超时以 `DEADLINE_EXCEEDED` 失败，扩展召回超时只标记 `degraded` 并保留精确结果。每个响应最多向底层取 8 个 recall page，单页最多返回 50 项，避免去重风暴或异常翻页放大下游压力。

## 接口与环境变量

- 内部 gRPC：`search`、`suggestions`、`validate_attributions`。
- `SEARCH_MAIN_ADDR`：默认 `127.0.0.1:8090`。
- `BBS_SEARCH_GRPC_URL`：默认 `http://127.0.0.1:8085`。
- `BBS_LINK_GRPC_URL`：默认 `http://127.0.0.1:18004`，用于 pending 内容的权威公开回读。
- `KNOWLEDGE_CATALOG_GRPC_URL`：默认 `http://127.0.0.1:8105`，用于统一搜索中的公共资源召回。
- `FEATURE_MAIN_GRPC_URL`：默认 `http://127.0.0.1:8093`，用于已登录搜索结果的 pCTR/pCVR/pWEGU 特征。
- `AD_MAIN_GRPC_URL`：默认 `http://127.0.0.1:8100`，用于路线行动节点搜索首页的场景广告 eCPM 决策。
- `STORAGE_MODE`：`memory` 或生产使用的 `postgres`；后者要求先运行数据库迁移。
