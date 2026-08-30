# 搜索一次性任务

全量重建、别名切换和数据回填任务放在这里。

## `search-index-outbox-recovery`

`bookway-search-index-outbox-recovery` 管理 `content_index_outbox` 的死信恢复。默认动作 `report` 不会改变投影任务状态，只输出并持久化按状态的数量、最大尝试次数和最老任务年龄，供告警和当班排障使用：

```bash
cargo run -p bookway-search-index-outbox-recovery
```

在 OpenSearch、mapping 或配置问题已经修复后，才可显式重排已经死亡至少 5 分钟的任务：

```bash
SEARCH_INDEX_RECOVERY_ACTION=requeue_dead \
SEARCH_INDEX_RECOVERY_ACTOR=oncall@example.com \
SEARCH_INDEX_RECOVERY_REASON='OpenSearch mapping corrected' \
cargo run -p bookway-search-index-outbox-recovery
```

`requeue_dead` 每次最多处理 `SEARCH_INDEX_RECOVERY_LIMIT`（默认 `100`）条，并受 `SEARCH_INDEX_RECOVERY_MIN_DEAD_AGE_SECONDS`（默认 `300`）约束。它以 `FOR UPDATE SKIP LOCKED` 领取死信，将版本推进到当前内容版本并重置为 `pending`；内容 ID、旧版本、尝试次数和旧错误保留在 `content_index_recovery_runs` / `content_index_recovery_items` 中。任务不会删除内容、索引或死信记录，也不会重排仍在 `pending` / `processing` 的正常投影。

## `search-index-rebuild`

`bookway-search-index-rebuild` 将 PostgreSQL `content_items` 的完整历史投影到一个已经存在的物理 OpenSearch 索引。它要求 `OPENSEARCH_URL` 与 `OPENSEARCH_REBUILD_INDEX`；目标通过 `_resolve/index` 验证为精确物理索引，别名和数据流会被拒绝。`SEARCH_INDEX_REBUILD_BATCH_SIZE` 默认 `500`，`SEARCH_INDEX_REBUILD_AFTER_ID` 可从日志中最后成功的内容 ID 继续执行。

重建投影与常驻 `bbs-indexer` 使用相同的路线节点、场景装备和规范化枚举字段。需要语义检索时同时设置 `SEMANTIC_VECTOR_DIMS`（8--4096，必须与目标索引 mapping 固定维度一致）和可选的 `KNOWLEDGE_CATALOG_GRPC_URL`；任务会经 `knowledge-catalog.EmbedTexts` 回填节点感知向量。向量服务不可用时保留可用的词法文档；修复服务后应从头重跑以补齐此前跳过的向量。

任务以内容 ID 做 keyset 分页，并在 Bulk API 中携带内容版本和 `version_type=external_gte`。已发布内容 upsert，草稿、受限和已删除内容 delete；已经被更高版本 Shadow 写入覆盖的 `version_conflict_engine_exception` 会被安全视为完成。因此从开头重放或者在某个完成页后恢复都不会倒退索引版本。成功结束时任务会刷新目标索引，确保紧随其后的计数、抽样和别名发布可见所有已提交文档。

在运行任务前，必须先配置并保持 `bbs-indexer` 双写：

```bash
OPENSEARCH_WRITE_INDEX=bookway-content-v1 \
OPENSEARCH_SHADOW_WRITE_INDEX=bookway-content-v2 \
cargo run -p bookway-search-indexer

OPENSEARCH_REBUILD_INDEX=bookway-content-v2 \
cargo run -p bookway-search-index-rebuild
```

Shadow 写入确保重建期间的并发内容变更也到达目标索引；不要把重建任务单独用于一个仍在接收写入的目标。目标验证完成前不要关闭影子写入。

## `search-index-reconcile`

`bookway-search-index-reconcile` 对比 PostgreSQL `content_items` 与一个明确的物理 OpenSearch 索引，但不会写入或删除文档，也不会改变 `content_index_outbox`。它要求 `OPENSEARCH_URL` 和 `OPENSEARCH_RECONCILE_INDEX`；任务通过 `_resolve/index` 拒绝别名和数据流目标。它在逐文档审计前刷新目标，并在结束前再次刷新后读取两侧总数。

```bash
OPENSEARCH_RECONCILE_INDEX=bookway-content-v2 \
cargo run -p bookway-search-index-reconcile
```

默认输出仅包含聚合指标：扫描数、应公开/应缺席数、缺失、版本滞后、非预期存在、源/目标总数、Outbox 的 `pending` / `processing` / `dead` 数量和 `healthy`。公开且未删除的内容必须存在，且 `_source.version` 必须等于 `content_items.version`；其他内容必须不存在。全量扫描同时满足这些逐文档判据、源/目标总数相等，并且 Outbox 没有未投递任务时，`healthy=true`，这也排除了额外文档和未收敛投影。内容 ID 默认不会出现在输出或日志中；仅在受控排障时设置 `SEARCH_INDEX_RECONCILE_SAMPLE_LIMIT`（最大 `100`）才会返回每类问题的有界样本。

任务使用内容 ID keyset 分页，`SEARCH_INDEX_RECONCILE_BATCH_SIZE` 默认 `500`。每批的聚合进度与受限数据库内的检查点会写入 `content_index_reconciliation_runs`；默认日志和 JSON 只输出安全的 `run_id`，不包含内容 ID。中断或失败后，以相同目标索引和该 ID 续跑，任务会接着既有检查点与聚合数据工作，并以创建时固定的 `SEARCH_INDEX_RECONCILE_LEASE_SECONDS`（默认 `600`）防止同一运行被并发领取；续跑配置不会缩短既有租约或改变既有批大小：

```bash
OPENSEARCH_RECONCILE_INDEX=bookway-content-v2 \
SEARCH_INDEX_RECONCILE_RUN_ID=the-run-id-from-the-prior-output \
cargo run -p bookway-search-index-reconcile
```

`SEARCH_INDEX_RECONCILE_AFTER_ID` 仅可用于创建一个新的后缀范围审计，不能与 `SEARCH_INDEX_RECONCILE_RUN_ID` 一起设置。后缀范围的结果会标记 `full_scan=false` 且不会报告全局健康；要得到可发布的 `healthy=true` 结论，必须从开头创建并完成一次全量运行。

在新索引发布前，应保持 Shadow 双写，等待 Outbox 清空后运行完整对账；最终结果也会再次验证没有 `pending`、`processing` 或 `dead` 任务。并发内容变化会使无锁对账出现瞬时不一致，因此不要把写入高峰期间的结果当作发布证明。发现局部死信时使用 `search-index-outbox-recovery` 受控重排；广泛差异则保留 Shadow 写入并重新执行 `search-index-rebuild`，再完成一次全量对账。

## `search-index-alias-switch`

`bookway-search-index-alias-switch` 是唯一负责发布 OpenSearch 读别名的一次性任务。它要求以下显式配置：

- `OPENSEARCH_URL`
- `OPENSEARCH_READ_ALIAS`，例如 `bookway-content`
- `OPENSEARCH_WRITE_INDEX`，例如 `bookway-content-v2`

任务拒绝通配符、系统名称、别名和数据流目标；依次检查目标索引存在、`_resolve/index` 精确解析为物理索引、刷新目标索引并确认 `/_count` 可读。随后它读取别名的所有当前成员，在一次 `POST /_aliases` 请求中移除这些成员并添加目标。旧索引不会被删除，失败时别名保持原状。别名控制面必须串行执行，同一别名不可并发运行两个切换任务。

推荐发布顺序：

1. 保持 `OPENSEARCH_WRITE_INDEX` 指向当前线上物理索引，并配置新物理索引为 `OPENSEARCH_SHADOW_WRITE_INDEX`；Indexer 会创建并验证两者。
2. 以同一新物理索引运行 `search-index-rebuild`，然后运行一次从开头开始的 `search-index-reconcile`，并抽样比较 mapping、查询和权限过滤结果。仅消费增量 Outbox 不能补齐一个空的新索引。
3. 确认目标索引容量、分片健康和文档计数后，保持 Shadow 写入并执行原子发布：

```bash
OPENSEARCH_URL=http://search.example:9200 \
OPENSEARCH_READ_ALIAS=bookway-content \
OPENSEARCH_WRITE_INDEX=bookway-content-v2 \
cargo run -p bookway-search-index-alias-switch
```

4. 将主写索引滚动切换到新物理索引后再移除 Shadow 写入，观察新查询、降级率和错误率；保留旧索引至少覆盖 PIT 的 5 分钟生存期及既定回滚窗口。

回滚使用相同命令，将 `OPENSEARCH_WRITE_INDEX` 改为经确认仍完整的旧物理索引。该任务的结构性检查不替代发布前的内容完整性和相关性验证。
