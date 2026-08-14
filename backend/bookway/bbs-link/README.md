# BBS Link 内容服务

## 职责

`bbs-link` 是公开内容的唯一事实源，持有帖子元数据、正文、媒体引用、主题、作者归属、版本和审核状态。

## 接口

- `POST /v1/posts`：创建内容，支持 `Idempotency-Key`。
- `GET/PATCH /v1/posts/{id}`：读取和更新内容。
- `POST /v1/posts/{id}/publish`：发布内容。
- 内部 gRPC：`list`、`get`、`get_public`、`create`、`update`、`publish`。

## 环境变量

`BBS_LINK_ADDR` 和 `BBS_LINK_GRPC_ADDR`，默认分别监听 `127.0.0.1:8084`、`127.0.0.1:18004`。审核依赖使用 `CONTENT_AUDIT_GRPC_URL`。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Repository、事务幂等键、乐观版本冲突和内容审核调用；媒体字节由独立 `media` 服务负责，OpenSearch 由独立索引进程同步。下一阶段补齐内容变更 Outbox/CDC、审核申诉、批量媒体归属校验、索引回放和热点内容缓存。
