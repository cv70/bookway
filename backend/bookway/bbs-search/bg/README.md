# 搜索后台任务

`bg` 是搜索域后台任务的容器，每个任务使用独立目录和 Cargo package。

当前任务：

- `bbs-indexer/`：将 PostgreSQL 内容增量同步到 OpenSearch。

后续可以在此增加查询分析、热词刷新、索引对账等独立后台任务。
