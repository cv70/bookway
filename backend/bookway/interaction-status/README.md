# Interaction Status 互动状态服务

## 职责

持有用户对内容的点赞和收藏状态、幂等开关、计数以及批量互动上下文。

## 接口

- `PUT /v1/posts/{post_id}/reactions`
- 内部 gRPC：`context`、`set_reaction`。

帖子存在性由 Gateway 在公开写入前校验。推荐服务直接消费批量互动上下文，不再通过 BBS 间接获取。

## 环境变量

`INTERACTION_STATUS_ADDR` 和 `INTERACTION_STATUS_GRPC_ADDR`，默认分别监听 `127.0.0.1:8087`、`127.0.0.1:18007`。

## 生产化待办

`STORAGE_MODE=postgres` 已使用 SQLx/PostgreSQL 联合唯一键保证点赞和收藏幂等。下一阶段增加 Redis 热点计数、异步对账、反刷限流、Outbox 事件投递和数据保留/隐私策略。
