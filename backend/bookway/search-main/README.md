# Search Main 搜索主服务

## 职责

搜索产品与排序流水线的主入口。当前负责查询规范化、搜索类型策略、分页上限、空查询校验以及 `bbs-search` 调用；后续在这里编排查询改写、多路召回、粗排、精排、重排、实验和降级，不把这些策略堆入 Gateway。

`bbs-search` 保持底层检索和索引访问职责，`bbs-link` 保持内容事实源职责。

## 接口

- `GET /internal/v1/search`
- `GET /internal/v1/suggestions`
- `GET /health`

## 环境变量

- `SEARCH_MAIN_ADDR`：默认 `127.0.0.1:8090`。
- `BBS_SEARCH_URL`：默认 `http://127.0.0.1:8085`。

## 生产化待办

底层 `bbs-search` 已接入 OpenSearch 关键词检索和 PostgreSQL 事实源降级。下一阶段在本服务按 `query rewrite -> recall -> pre-rank -> rank -> rerank` 拆分可观测阶段，增加向量检索、Redis 会话、搜索特征/模型、实验分桶和查询事件流；为每个阶段配置独立超时预算、熔断、并行召回与离线回放评测。
