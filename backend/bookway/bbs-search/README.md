# BBS Search 搜索服务

## 职责

负责查询理解、检索和搜索索引访问，但不成为内容事实源。

## 接口

- 内部 gRPC：`search`、`suggestions`。

配置 `OPENSEARCH_URL` 后，服务通过 OpenSearch 读别名执行多字段检索。OpenSearch 只负责候选召回与稳定排序：每个索引页都会按内容 ID 批量调用 `bbs-link.get_public_summaries`，再以当前公开摘要重建可显示字段；缺失项会丢弃并标记 `degraded=true`，验证失败时整个索引页 fail-closed，绝不返回索引中的旧标题、正文、媒体或已受限内容。Gateway 从可信身份派生拉黑/静音作者集合；OpenSearch 以 `author_id` 条件下推排除，任何降级源也会在生成帖子、路线、用户、话题和内容派生联想词之前过滤，因此不会把不可见作者计入用户结果、话题数或联想词。结果续页使用 5 分钟 PIT、`_score` / 内容 ID 排序和 `search_after`，而公共游标仅包含版本、查询/类型/查看者可见性指纹和短期会话 ID；PIT token、未消费的混合结果及跨页去重键保存在服务端。可见性策略变化会使旧游标失效，避免后续页混入不再可见的内容。PostgreSQL 模式的会话可跨实例续页，过期时服务返回可识别的前置条件错误，客户端应重新发起搜索。首次索引请求不可用时回退到 `bbs-link` 的公开内容并返回 `degraded=true`；继续中的 PIT 请求不会混用降级排序。未配置 OpenSearch 时可直接使用 `bbs-link` 作为无依赖开发路径。结果包含稳定游标、相关性分数和高亮。

`bbs-indexer` 会把 Protobuf 枚举投影为稳定的关键字 `status`、`content_type` 和 `domain`，避免生成代码的整数表示污染 OpenSearch 过滤。`milestone` 以普通内容参与关键词召回、重排和公开摘要回读，且在结果摘要中保留 `is_milestone=true`；它不会被误分类为可加入的路线。此映射需要新物理索引，已有索引应遵循下方的影子写入、重建、对账和别名切换流程升级，不能原地改变字段类型。

## 依赖与环境变量

- 依赖：OpenSearch（生产主路径）、`bbs-link`（降级路径）。
- `BBS_SEARCH_ADDR`：默认 `127.0.0.1:8085`。
- `BBS_LINK_GRPC_URL`：内容服务地址；生产环境在 `SERVICE_AUTH_REQUIRED=true` 下自动携带 `x-service-token`。
- `OPENSEARCH_URL`：OpenSearch 地址；生产环境必须配置。
- `OPENSEARCH_READ_ALIAS`：读取端使用的逻辑别名，例如 `bookway-content`。配置 `OPENSEARCH_URL` 时必须显式配置。
- `OPENSEARCH_WRITE_INDEX`：`bbs-indexer` 唯一可写的物理索引，例如 `bookway-content-v2`。Worker 在启动时通过 `_resolve/index` 验证它是精确的物理索引；别名和数据流会 fail-closed。
- `OPENSEARCH_SHADOW_WRITE_INDEX`：仅在全量重建期间启用的第二个物理写索引。每个 Outbox 变更必须同时写入主、影子索引才会被确认，避免构建期间漏掉并发变更。
- `OPENSEARCH_REBUILD_INDEX`：`search-index-rebuild` 的显式重建目标，通常与影子写索引相同。
- `OPENSEARCH_RECONCILE_INDEX`：`search-index-reconcile` 的显式只读对账目标，必须是物理索引；别名和数据流会被拒绝。
- `SEARCH_INDEX_RECONCILE_AFTER_ID`：只读对账的可选 keyset 续跑游标。设置后只审计后缀范围，不会产生全量健康结论。
- `SEARCH_INDEX_RECONCILE_SAMPLE_LIMIT`：可选、最多 `100` 个的内部内容 ID 问题样本；默认不输出内容 ID。
- `SEARCH_INDEX_RECONCILE_RUN_ID`：对账运行 ID。失败或中断后以同一 ID 续跑，任务从受限数据库内的持久检查点继续，不把游标内容 ID 输出到普通日志。
- `SEARCH_INDEX_RECONCILE_LEASE_SECONDS`：新对账运行的租约，默认 `600` 秒；续跑沿用已持久化的原值，运行中的同一 ID 不可被并发接续。
- `SEARCH_INDEX_RECOVERY_ACTION`：索引 Outbox 恢复任务的动作，默认只读 `report`；`requeue_dead` 必须同时设置具名操作者和恢复原因，才会把满足最小死信年龄的任务重新排队。

`0022_search_sessions.sql` 必须在生产发布前执行；会话默认 5 分钟过期，创建会话时会利用 `idx_search_sessions_expiry` 清理已过期状态。

## 零停机索引发布

先将现有物理索引挂到读别名，再把 `bbs-search` 配置为 `OPENSEARCH_READ_ALIAS`。发布新版本时，保持旧主写索引不变，将新物理索引设为 `OPENSEARCH_SHADOW_WRITE_INDEX`，再运行 `search-index-rebuild` 填充历史内容。索引器的主、影子写入均使用内容版本的 `external_gte`，因此重建重放和并发 Outbox 写入不会倒退文档。待 Outbox 清空后，`search-index-reconcile` 从头比对每个内容的应有可见性和版本、源/目标总数，并再次确认没有未投递 Outbox；只有完整扫描返回 `healthy=true` 才可作为投放前完整性证据。完成 mapping、查询和权限验证后，由 `job/search-index-alias-switch` 原子替换该别名的全部旧成员；它不会删除旧索引。已有 PIT 保持旧快照直到过期，新建 PIT 在切换后读取新索引，因此续页不会混入两套排序。详细前置检查、切换和回滚命令见 [`job/README.md`](job/README.md)。

`bbs-indexer` 通过内容服务事务性写入的 `content_index_outbox` 消费内容投影：同一内容连续变更会合并为最新版本，索引 worker 使用租约防止旧 worker 覆盖新版本，并在 OpenSearch 不可用时指数退避重试。`search-index-reconcile` 将聚合进度、检查点和最终结论保存在 `content_index_reconciliation_runs`，可用运行 ID 安全续跑；死信由 `search-index-outbox-recovery` 默认聚合报告，只有显式具名的受控运行才能重排；每次报告和重排都会留下数据库审计记录。查询词典和受控改写由 `search-main` 的版本化配置承接。

登录用户的成功搜索会写入 `search_query_history`，只保留查询文本、类型、计数和最近时间，默认按 90 天窗口提供个人建议词；匿名请求不落个人历史，个人历史只通过同一已验证用户返回。全局建议词只有至少两次请求的聚合词才可见，个人词优先于全局词，数据库不可用时自动退回内容派生和内置词典建议。该设计把搜索意图回流到体验，同时避免将单个用户的一次敏感查询直接作为公共热词。
