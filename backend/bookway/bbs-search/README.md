# BBS Search 搜索服务

## 职责

负责查询理解、检索和搜索索引访问，但不成为内容事实源。

## 接口

- 内部 gRPC：`search`、`suggestions`。

配置 `OPENSEARCH_URL` 后，服务通过版本化 OpenSearch 索引执行多字段检索。Gateway 从可信身份派生拉黑/静音作者集合；OpenSearch 以 `author_id` 条件下推排除，任何降级源也会在生成帖子、路线、用户、话题和内容派生联想词之前过滤，因此不会把不可见作者计入用户结果、话题数或联想词。结果续页使用 5 分钟 PIT、`_score` / 内容 ID 排序和 `search_after`，而公共游标仅包含版本、查询/类型/查看者可见性指纹和短期会话 ID；PIT token、未消费的混合结果及跨页去重键保存在服务端。可见性策略变化会使旧游标失效，避免后续页混入不再可见的内容。PostgreSQL 模式的会话可跨实例续页，过期时服务返回可识别的前置条件错误，客户端应重新发起搜索。首次索引请求不可用时回退到 `bbs-link` 的公开内容并返回 `degraded=true`；继续中的 PIT 请求不会混用降级排序。未配置 OpenSearch 时可直接使用 `bbs-link` 作为无依赖开发路径。结果包含稳定游标、相关性分数和高亮。

## 依赖与环境变量

- 依赖：OpenSearch（生产主路径）、`bbs-link`（降级路径）。
- `BBS_SEARCH_ADDR`：默认 `127.0.0.1:8085`。
- `BBS_LINK_GRPC_URL`：内容服务地址；生产环境在 `SERVICE_AUTH_REQUIRED=true` 下自动携带 `x-service-token`。
- `OPENSEARCH_URL`：OpenSearch 地址；生产环境必须配置。
- `OPENSEARCH_INDEX`：版本化内容索引名，默认 `bookway-content-v1`。

`0022_search_sessions.sql` 必须在生产发布前执行；会话默认 5 分钟过期，创建会话时会利用 `idx_search_sessions_expiry` 清理已过期状态。

## 生产化待办

将当前增量索引轮询升级为 Outbox/CDC 驱动，继续补齐词典管理、查询改写、拼写纠错、细粒度权限策略、删除传播、零停机索引别名切换和搜索分析。
