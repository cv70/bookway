# 搜索后台任务

`bg` 是搜索域后台任务的容器，每个任务使用独立目录和 Cargo package。

当前任务：

- `bbs-indexer/`：消费事务性内容索引 Outbox，将 PostgreSQL 内容变更同步到 `OPENSEARCH_WRITE_INDEX` 指定的物理 OpenSearch 索引，并可在发布期间同步到 `OPENSEARCH_SHADOW_WRITE_INDEX`。

写索引必须是明确的物理索引，不能配置为读别名或数据流；Worker 会在任何文档写入前拒绝这两种配置。主、影子索引任一写入失败都会保留 Outbox 任务重试；同一内容版本以 OpenSearch `external_gte` 写入，因此重试、重建和双写不会让旧投影覆盖新投影。`OPENSEARCH_INDEX` 仅保留为旧部署兼容回退，应迁移到 `OPENSEARCH_WRITE_INDEX`。发布新索引时先配置影子写入并完整重建目标，再由 [`../job/README.md`](../job/README.md) 的别名切换任务发布。

一次性索引对账由 [`../job/search-index-reconcile`](../job/search-index-reconcile) 执行；它不写索引或 Outbox，可在完整重建后确认每个公开内容的版本和全局计数都已收敛。
