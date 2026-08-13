# 万卷行 Backend

万卷行后端是独立的 Rust workspace，统一使用 Rust 2024、Axum、Tokio、Tower、Serde 和 Reqwest。

当前版本是**生产基础设施接入版本**：十五个服务可以独立编译和启动。核心事实数据可通过 `STORAGE_MODE=postgres` 切换到 SQLx/PostgreSQL；用户事件使用同事务 Outbox 并由 Kafka Relay 发布；Redis 提供限流和特征缓存；OpenSearch、对象存储/CDN、内容审核、特征与模型排序、JWT/服务令牌、Prometheus 指标和 SLO 已形成可运行边界。默认 `memory` 模式仍保留用于无依赖开发。

## 微服务拓扑

| 服务 | 默认端口 | 所有权 | 主要职责 |
| --- | ---: | --- | --- |
| `gateway` | `8080` | platform | App 唯一入口、聚合和错误转换 |
| `growth` | `8081` | growth | 私人路线、今日行动和完成状态 |
| `bbs` | `8082` | bbs | 关注、拉黑、静音和社交图谱 |
| `recommend-main` | `8083` | recommendation | 多路召回、补全、过滤、排序和曝光 |
| `bbs-link` | `8084` | bbs-content | 内容事实、版本和发布状态 |
| `bbs-search` | `8085` | bbs-search | 内容、路线、用户、主题检索与联想 |
| `comment` | `8086` | comment | 评论正文、父子关系和审核状态 |
| `commonlikestatus` | `8087` | interaction | 点赞、收藏、计数和批量互动状态 |
| `bbs-feed` | `8088` | bbs-feed | Feed 产品策略、请求规范化和交付 |
| `user-event` | `8089` | data-platform | 曝光、点击等用户行为批量接收 |
| `search-main` | `8090` | search | 查询规范化、搜索编排和降级边界 |
| `media` | `8091` | platform | 对象 key、上传会话、媒体元数据和 CDN |
| `content-audit` | `8092` | trust-safety | 文本审核、风险决策和人工复审入口 |
| `feature-main` | `8093` | recommendation | 实时/离线特征统一读取与缓存 |
| `recommend-recall` | `8095` | recommendation | 候选召回、去重和已看过滤 |
| `recommend-rank` | `8096` | recommendation | 模型排序、版本、实验桶与启发式降级 |

```text
mobile ──HTTPS──> gateway ─┬──> growth
                           ├──> bbs-link
                           ├──> bbs
                           ├──> comment
                           ├──> commonlikestatus
                           ├──> user-event
                           ├──> media ────────────> S3/MinIO + CDN
                           ├──> search-main ───────> bbs-search ──> bbs-link
                           └──> bbs-feed ─────────> recommend-main ─┬──> bbs-link
                                                                  ├──> bbs
                                                                  ├──> commonlikestatus
                                                                  └──> feature-main ──> recommend-rank
```

数据所有权严格分离：`bbs-link` 持有内容事实，`bbs` 持有关系，`comment` 持有评论，`commonlikestatus` 持有互动状态，`user-event` 持有行为接收幂等状态，`recommend-main` 只负责在线推荐决策，`bbs-feed` 负责 Feed 交付，`search-main` 负责搜索产品编排，`bbs-search` 只负责检索和索引访问。

## 目录约定

```text
backend/
├── bookway/
│   ├── gateway/
│   ├── growth/
│   ├── bbs/
│   ├── recommend-main/
│   ├── bbs-link/
│   ├── bbs-search/
│   ├── comment/
│   ├── commonlikestatus/
│   ├── bbs-feed/
│   ├── user-event/
│   ├── search-main/
│   ├── media/
│   ├── content-audit/
│   ├── feature-main/
│   ├── recommend-recall/
│   └── recommend-rank/
│       ├── Cargo.toml
│       ├── README.md
│       ├── service.yaml
│       ├── src/main.rs
│       ├── src/api/          # HTTP 或 gRPC 传输适配
│       ├── src/conf/
│       ├── src/datasource/
│       └── src/domain/       # Domain 持有配置、依赖和业务编排
├── deploy/docker-compose.yml
├── migrations/
├── cmd/
│   ├── db-migrate/
│   ├── outbox-relay/
│   └── search-indexer/
├── infra/data/
├── infra/event/
├── infra/runtime/
├── pkg/api/                  # HTTP DTOs and shared application types
├── Cargo.toml
├── Cargo.lock
└── rust-toolchain.toml
```

每一个微服务的 `README.md` 与该服务的 `Cargo.toml` 同级。服务内部统一由 `Domain` 持有配置、Repository、客户端和业务对象；`api/http.rs` 只承载外部 HTTP，`api/grpc.rs` 只承载内部 gRPC，服务启动入口由 `api` 暴露给 `main`：

```text
main -> Domain -> api/http.rs or api/grpc.rs
          |
     datasource / conf
```

## 推荐主链路

`recommend-main/src/domain/pipeline` 对应 x-algorithm 的多阶段 Candidate Pipeline：

```text
Query Hydrator
-> 质量/新鲜度内容召回（并行）
-> BBS 关系补全
-> commonlikestatus 互动状态补全
-> 去重、已看、安全过滤
-> 质量、意图、作者多样性打分
-> 多样性 Selector
-> 异步曝光副作用
```

候选源失败或补全失败时保留可用结果，并将 `FeedMeta.degraded` 置为 `true`。响应包含 `request_id`、`pipeline_id`、游标和阶段统计。在线链路已调用 `feature-main` 和 `recommend-rank`；当前模型实现是可解释、可版本化的启发式 Ranker，远程排序不可用时保留流水线基础分并显式标记降级。训练平台、外部推理引擎、负反馈建模和可回放评测仍属于后续容量阶段。

## Feed 与推荐链路

Gateway 只请求 `bbs-feed`。`bbs-feed` 负责客户端 surface、分页上限和产品降级边界，再调用 `recommend-main` 完成候选流水线。避免把产品插卡、运营策略或关注流逻辑写入模型决策服务。

## 搜索链路

Gateway 只请求 `search-main`，由它规范化参数和编排底层 `bbs-search`：

```text
内部搜索由 `search-main` 的 gRPC `search` 和 `suggestions` 方法提供。
```

设置 `OPENSEARCH_URL` 后，`bbs-search` 使用 OpenSearch 多字段召回并在索引不可用时降级到 `bbs-link`；`bookway-search-indexer` 创建版本化 CJK 索引并同步公开内容。`search-main` 仍承担 `query rewrite -> recall -> pre-rank -> rank -> rerank` 的产品编排边界。

## 内容、社区与互动

- `bbs-link`：草稿、审核中、已发布、受限、删除状态；作者归属、版本号和 `Idempotency-Key` 请求指纹。
- `bbs`：关注、拉黑、静音；拉黑清理关注并阻止冲突关注。
- `comment`：空评论/超长评论/跨帖父评论校验，评论数据不再混入 BBS。
- `commonlikestatus`：点赞与收藏幂等集合、计数和批量互动上下文。
- Gateway：互动写入前调用内容服务校验内容存在且公开。
- `user-event`：最多 100 条批量事件、UUID 与时间校验、`event_id` 幂等；事件和 Outbox 同事务写入，`bookway-outbox-relay` 负责 Kafka 重试、指数退避与死信。

配置 `CONTENT_AUDIT_GRPC_URL` 后，发布会先进入 `content-audit`，再转换为公开、复审或受限状态；每个内容版本保留风险分数、原因和 provider。人工复审、申诉和多媒体审核仍需继续扩展。

媒体上传通过 Gateway 获取 S3/MinIO 预签名 PUT URL，客户端直接上传到对象存储，再调用完成接口。`media` 会使用 HEAD 校验对象的实际大小和 MIME，只有校验一致才将元数据从 `pending` 转为 `ready`；待上传资产仅所有者可读，`ready` 资产通过 CDN URL 访问。

## 数据库与本地依赖

迁移按服务所有权拆分：

| 文件 | 服务 | 内容 |
| --- | --- | --- |
| `0001_content.sql` | bbs-link | 内容、媒体、主题、幂等键 |
| `0002_bbs.sql` | bbs | 社交关系 |
| `0003_search.sql` | bbs-search | 搜索文档和查询统计 |
| `0004_feed.sql` | recommend-main | 曝光和推荐事件 |
| `0005_commonlikestatus.sql` | commonlikestatus | 点赞/收藏状态 |
| `0006_comment.sql` | comment | 评论和审核状态 |
| `0007_user_event.sql` | user-event | 用户行为事件、时间和请求关联索引 |
| `0008_growth.sql` | growth | 私人路线与今日行动 |
| `0009_outbox.sql` | data-platform | Transactional Outbox 与审计日志 |
| `0010_media_audit_features.sql` | platform | 媒体、审核、特征和模型版本 |

```bash
docker compose -f deploy/docker-compose.yml up -d
```

该命令启动 PostgreSQL 16、Redis 7、Redpanda、OpenSearch 2.17、MinIO 和 Prometheus，并在 Redpanda 健康后幂等创建 12 分区的 `bookway.domain-events.v1` Topic。复制 `.env.example` 中需要的变量并显式设置 `STORAGE_MODE=postgres`；业务服务不会自动执行迁移。

```bash
DATABASE_URL=postgres://bookway:bookway-local-only@127.0.0.1:5432/bookway \
  cargo run -p bookway-db-migrate
cargo run -p bookway-outbox-relay
cargo run -p bookway-search-indexer
```

## 本地运行

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

依次启动：

```bash
cargo run -p bookway-bbs
cargo run -p bookway-bbs-link
cargo run -p bookway-bbs-search
cargo run -p bookway-comment
cargo run -p bookway-commonlikestatus
cargo run -p bookway-recommend-main
cargo run -p bookway-bbs-feed
cargo run -p bookway-user-event
cargo run -p bookway-search-main
cargo run -p bookway-media
cargo run -p bookway-content-audit
cargo run -p bookway-feature-main
cargo run -p bookway-recommend-recall
cargo run -p bookway-recommend-rank
cargo run -p bookway-growth
cargo run -p bookway-gateway
```

主要环境变量：

| 服务 | 监听变量 | 上游变量 |
| --- | --- | --- |
| gateway | `GATEWAY_ADDR` | `GROWTH_GRPC_URL`、`BBS_FEED_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、`USER_EVENT_GRPC_URL`、`BBS_LINK_GRPC_URL`、`BBS_GRPC_URL`、`COMMENT_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`MEDIA_GRPC_URL` |
| growth | `GROWTH_ADDR` | 无 |
| bbs | `BBS_ADDR` | 无 |
| recommend-main | `RECOMMEND_MAIN_ADDR` | `BBS_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`RECOMMEND_RECALL_GRPC_URL`、`RECOMMEND_RANK_GRPC_URL` |
| bbs-link | `BBS_LINK_ADDR`、`BBS_LINK_GRPC_ADDR` | `CONTENT_AUDIT_GRPC_URL` |
| bbs-search | `BBS_SEARCH_ADDR` | `BBS_LINK_GRPC_URL` |
| comment | `COMMENT_ADDR` | 无（由 Gateway 先校验内容） |
| commonlikestatus | `LIKE_STATUS_ADDR` | 无（由 Gateway 先校验内容） |
| bbs-feed | `BBS_FEED_ADDR` | `RECOMMEND_MAIN_GRPC_URL` |
| user-event | `USER_EVENT_ADDR` | PostgreSQL + Transactional Outbox，由 Relay 发布 Kafka |
| search-main | `SEARCH_MAIN_ADDR` | `BBS_SEARCH_GRPC_URL` |
| media | `MEDIA_ADDR` | `S3_ENDPOINT`、`S3_BUCKET`、`CDN_BASE_URL` |
| content-audit | `CONTENT_AUDIT_ADDR` | 审核规则与 PostgreSQL |
| feature-main | `FEATURE_MAIN_ADDR` | `REDIS_URL`、PostgreSQL |
| recommend-recall | `RECOMMEND_RECALL_ADDR` | `BBS_LINK_GRPC_URL` |
| recommend-rank | `RECOMMEND_RANK_ADDR` | `RECOMMEND_RANK_MODEL_VERSION` |

全服务共享 `STORAGE_MODE`、`DATABASE_URL`、`SERVICE_AUTH_TOKEN`、`SERVICE_AUTH_REQUIRED`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`HTTP_CONNECT_TIMEOUT_MS` 和 `HTTP_REQUEST_TIMEOUT_MS`。Redis 连接和命令预算分别由 `REDIS_CONNECT_TIMEOUT_MS`（默认 1000ms）与 `REDIS_COMMAND_TIMEOUT_MS`（默认 100ms）控制，缓存或限流 Redis 故障时会告警并 fail-open。生产环境必须启用两种鉴权：App 的 Bearer JWT 只在 Gateway 解析；非 Gateway 服务的业务端点只接受带服务令牌的内部请求，并信任 Gateway 注入的 `x-user-id`。健康、就绪和指标端点不要求服务令牌。

如果默认端口被占用，可整组使用 `18080-18090` 并显式设置全部 URL；客户端将 `EXPO_PUBLIC_API_URL` 指向新的 Gateway 地址。

## 已实现的生产能力

- SQLx 连接池和按服务所有权拆分的 PostgreSQL Repository，写路径使用事务与幂等键。
- Redis 全局限流和特征缓存；Redis 故障 fail-open 并记录告警。
- Transactional Outbox、Kafka/Redpanda Relay、`SKIP LOCKED`、退避和死信状态。
- OpenSearch CJK 索引、版本化索引名、检索降级和独立重建命令。
- Gateway 媒体 API、MinIO/S3 预签名直传、对象大小/MIME 完成校验、私有 pending 元数据和 CDN 地址。
- `feature-main -> recommend-rank` 在线调用，推荐模型失败时保留启发式得分并标记降级。
- Gateway HS256 JWT、下游服务 token、全服务请求 ID、调用超时、Prometheus 指标、按服务依赖 readiness 与 SLO 文档。

## 下一阶段阻断项

1. PostgreSQL 分片、读写隔离、跨地域复制、自动故障切换和备份恢复演练。
2. Kafka 幂等消费者、Schema 管理、事件回放工具与离线数仓/湖仓落地。
3. 图片处理、视频转码、病毒扫描、媒体审核和 CDN 回源保护。
4. OpenTelemetry OTLP trace、日志关联、持续压测和完整容量模型。
5. 人工审核台、申诉、举报、反作弊、未成年人和区域合规策略。
6. 模型训练、特征管理、A/B 平台、影子流量、漂移监控和一键回滚。
7. 多区域容灾、Kubernetes/Service Mesh、CI/CD、密钥托管和供应链安全。

应按压测、延迟、成本和故障域推进后续拆分，而不是仅以服务数量判断是否生产化。
