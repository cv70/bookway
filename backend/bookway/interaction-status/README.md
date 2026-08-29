# Interaction Status 互动状态服务

## 职责

持有用户对内容的点赞和收藏状态、幂等开关、计数以及批量互动上下文。

## 接口

- `PUT /v1/posts/{post_id}/reactions`
- 内部 gRPC：`context`、`set_reaction`。

帖子存在性由 Gateway 在公开写入前校验。推荐服务直接消费批量互动上下文，不再通过 BBS 间接获取。`Context.user_id` 缺失或为空时返回空状态，不会映射到共享的匿名用户；只有带身份的请求才读取个人点赞、收藏和隐藏。无论是否登录，`post_ids` 都先执行长度和 500 项批次上限校验。

## 环境变量

`INTERACTION_STATUS_ADDR` 和 `INTERACTION_STATUS_GRPC_ADDR`，默认分别监听 `127.0.0.1:8087`、`127.0.0.1:18007`。

配置 `REDIS_URL` 后，批量 `context` 会使用按用户和内容集合分片的 protobuf 热缓存；缓存带 30 秒 TTL、用户版本校验、跨实例刷新租约和进程内 singleflight。写入仍以 PostgreSQL 为事实源，并在成功后递增用户版本。Redis 不可用时回退 PostgreSQL；其他实例正在刷新且缓存尚未回填时返回 `UNAVAILABLE`，避免把未知状态当成允许。

## 生产化待办

`STORAGE_MODE=postgres` 已使用 SQLx/PostgreSQL 联合唯一键保证点赞和收藏幂等。Redis/PostgreSQL 真实依赖下的容量、P99、故障切换和租约演练仍需在发布前补齐；热点计数、异步对账、反刷限流、Outbox 事件投递和数据保留/隐私策略仍是后续工作。
