# BBS Link 内容服务

## 职责

`bbs-link` 是公开内容的唯一事实源，持有帖子元数据、正文、媒体引用、主题、作者归属、版本和审核状态。

## 接口

- `POST /v1/posts`：创建内容，支持 `Idempotency-Key`。
- `GET/PATCH /v1/posts/{id}`：读取已公开内容和更新内容。
- `POST /v1/posts/{id}/publish`：发布内容。
- 内部 gRPC：`list`、`get`、`get_public`、`create`、`update`、`publish`、`restrict`、`restore`。启用 `SERVICE_AUTH_REQUIRED=true` 后，所有业务方法都必须携带 `x-service-token`，健康检查除外；客户端不能通过直连绕过 Gateway 的可信身份、审核和社交可见性策略。`get` 仅供受信内部的作者/审核流程使用；面向客户端及社区互动的读取必须使用 `get_public`，草稿、审核中和受限内容统一以未找到响应。`restrict` 是人工处置专用的幂等状态迁移；它会清除公开时间，任何重试都不会重新公开内容。`restore` 只接受此前受限的内容，避免把草稿或待审编辑绕过发布审核直接公开。

## 环境变量

`BBS_LINK_ADDR` 和 `BBS_LINK_GRPC_ADDR`，默认分别监听 `127.0.0.1:8084`、`127.0.0.1:18004`。审核依赖使用 `CONTENT_AUDIT_GRPC_URL`。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Repository、事务幂等键、乐观版本冲突和内容审核调用；媒体字节由独立 `media` 服务负责，OpenSearch 由独立索引进程同步。Gateway 可通过内部 `list(author_id=...)` 为创作中心读取作者自己的非公开状态，但该过滤不对客户端直接开放。下一阶段补齐内容变更 Outbox/CDC、批量媒体归属校验、索引回放和热点内容缓存。
