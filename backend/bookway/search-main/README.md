# Search Main 搜索主服务

## 职责

搜索产品策略的主入口。它把用户查询编排为可演进的流水线：

`规范化 -> 精确召回 + 受控同义扩展 -> 混合/去重 -> 轻量重排 -> 稳定分页`

`bbs-search` 继续拥有底层关键词检索、OpenSearch/PIT 与事实源降级、内容可见性过滤，以及每一路召回的私有游标；`bbs-link` 保持内容事实源职责。Gateway 先根据社交关系注入可信查看者和屏蔽作者集合，Search Main 会原样传递给每一个 `bbs-search` 请求，不能由客户端覆盖。

## 查询与排序策略

- 将空白和常见中英文分隔符规范为 canonical query，限制为 1--100 个字符。
- 每次搜索都有精确 lexical recall；仅命中受控词典时增加一条扩展 recall，例如 `跑步 -> 慢跑/晨跑/夜跑`、`阅读 -> 读书/书单/主题阅读`。扩展故障只标记 `degraded`，精确召回故障则向上游报错。
- 相同 `(result_type, id)` 的候选会合并，未展示的候选保存在服务端并按标题完整命中、标题词覆盖、`#` 话题/`@` 用户/路线意图和领域匹配做确定性轻量重排。
- 该层是明确的模型接入点，而不是宣称已经存在 ML 排序模型；后续可在这里加入向量召回、特征、实验和离线评测。

## 游标与会话

公开游标格式为 `sm1-{fingerprint}-{uuid}`，只包含查询、搜索类型、可信查看者和排序后的排除作者集合的指纹；游标不能跨查询、用户、搜索类型或可见性集合复用。

会话保存各 recall 的 `bbs-search` 私有游标、候选 pending 队列、已见结果和退化状态，TTL 为 5 分钟。它不会把 OpenSearch PIT 或底层 `bbs-search` token 返回给客户端。`0028_search_main_sessions.sql` 提供 PostgreSQL 存储；`STORAGE_MODE=memory` 用于本地演示。旧版 `v3-...` 单路游标会被安全地转为只含精确召回的新会话，下一页起返回 `sm1-...`。

会话过期返回 `FAILED_PRECONDITION`，无效或不匹配游标返回 `INVALID_ARGUMENT`，存储和底层可用性故障返回 `UNAVAILABLE`。

## 可观测性与边界

完成日志只记录稳定查询 hash、召回变体数、调用数、候选数、耗时和降级状态，不记录搜索明文。每个响应最多向底层取 8 个 recall page，单页最多返回 50 项，避免去重风暴或异常翻页放大下游压力。

## 接口与环境变量

- 内部 gRPC：`search`、`suggestions`。
- `SEARCH_MAIN_ADDR`：默认 `127.0.0.1:8090`。
- `BBS_SEARCH_GRPC_URL`：默认 `http://127.0.0.1:8085`。
- `STORAGE_MODE`：`memory` 或生产使用的 `postgres`；后者要求先运行数据库迁移。
