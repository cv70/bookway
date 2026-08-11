# BBS 社交图谱服务

## 职责

持有关注、拉黑和静音关系，并为推荐系统批量提供社交上下文。它不持有帖子正文、评论、点赞或收藏。

## 接口

- `GET /internal/v1/social-context?user_id=...`
- `PUT /v1/users/{user_id}/follow`，请求体使用 `edge=follow|block|mute` 和 `active`。

拉黑会移除双方已有关注关系；处于拉黑关系的用户不能重新关注；服务拒绝用户对自己建立社交关系。

## 环境变量

`BBS_ADDR`，默认监听 `127.0.0.1:8082`。

## 生产化待办

`STORAGE_MODE=postgres` 已使用 SQLx/PostgreSQL 持久化关系，并在拉黑事务中清理冲突关注。下一阶段增加 Redis 图谱缓存、关系 Outbox/审计事件、隐私权限、反滥用限流，以及超大关系账号的读写扩散策略。
