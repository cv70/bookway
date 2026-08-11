# BBS Search 搜索服务

## 职责

负责查询理解、检索和搜索索引访问，但不成为内容事实源。

## 接口

- `GET /internal/v1/search?q=...&search_type=all|posts|journeys|users|topics&cursor=...`
- `GET /internal/v1/suggestions?q=...`

配置 `OPENSEARCH_URL` 后，服务通过版本化 OpenSearch 索引执行多字段检索，并在连接、HTTP 或响应解析失败时回退到 `bbs-link` 的公开内容，同时返回 `degraded=true`。未配置 OpenSearch 时可直接使用 `bbs-link` 作为无依赖开发路径。结果包含稳定游标、相关性分数和高亮。

## 依赖与环境变量

- 依赖：OpenSearch（生产主路径）、`bbs-link`（降级路径）。
- `BBS_SEARCH_ADDR`：默认 `127.0.0.1:8085`。
- `BBS_LINK_URL`：内容服务地址。
- `OPENSEARCH_URL`：OpenSearch 地址；生产环境必须配置。
- `OPENSEARCH_INDEX`：版本化内容索引名，默认 `bookway-content-v1`。

## 生产化待办

将当前增量索引轮询升级为 Outbox/CDC 驱动，继续补齐词典管理、查询改写、拼写纠错、权限过滤、删除传播、零停机索引别名切换和搜索分析。
