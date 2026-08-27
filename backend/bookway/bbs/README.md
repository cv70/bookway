# BBS 社交图谱服务

## 职责

持有关注、拉黑、静音关系和公共路线参与事实，并为推荐、搜索批量提供社交与同行上下文。它不持有帖子正文、评论、点赞或收藏。

## 接口

- 内部 gRPC：`context`、`visibility_context`、`set_edge`、`list_followers`、`get_social_stats`、`list_route_peers`、`list_route_participations`、`route_context`、`set_route_participation`。
- 外部 HTTP：`PUT /v1/users/{user_id}/follow`，请求体使用 `edge=follow|block|mute` 和 `active`。

拉黑会移除双方已有关注关系；处于拉黑关系的用户不能重新关注；服务拒绝空 ID 或用户对自己建立社交关系。每次关系写入都会按无向用户对获取事务 advisory lock，因此并发的关注和拉黑会串行化，拉黑完成后不会再落入新的关注关系。启用 `SERVICE_AUTH_REQUIRED=true` 后，全部业务 gRPC 都必须携带 `x-service-token`，健康检查除外；这使客户端只能经 Gateway 使用可信身份和可见性策略。`visibility_context` 合并当前用户的拉黑/静音对象与拉黑当前用户的作者；推荐、搜索和 Gateway 的直接内容读取/互动据此双向隐藏内容，但不会经客户端社交上下文暴露入站拉黑关系。路线参与命令携带 Growth 意图版本，BBS 在事务内拒绝旧版本，避免延迟加入覆盖较新的退出。

## 粉丝列表与社交计数

`list_followers` 以 keyset 游标返回某用户的入站关注（按 `(followed_at, follower_id)` 降序），游标形如 `{unix_millis}.{follower_id}`；相比偏移量分页，翻页期间新增的关注不会造成重复或漏行。单页默认 50 条、上限 200，非法或负时间戳游标返回校验错误。查询由迁移 `0080_bbs_follower_pages.sql` 的部分覆盖索引 `(target_user_id, created_at DESC, source_user_id DESC)` 支持，每页一次有序索引扫描。`get_social_stats` 返回双向关注计数，与上下文读取共用版本化缓存：任何关系写入都会同时失效两端的计数缓存，未关注变化不会伪装成已刷新。

Gateway 暴露 `GET /v1/users/{user_id}/followers?cursor&limit` 与 `GET /v1/users/{user_id}/social-stats`，供创作者主页展示粉丝与关注数量。

## 同行者列表

`list_route_peers(route_id, viewer_id, cursor)` 从公共参与事实读取一条路线的活跃参与者（不含查看者本人），供路线详情的「同行」模块使用。可见性过滤复用既有安全链路：领域层先经 `visibility_context` 取得查看者的排除集合（双向拉黑加上对外静音），再把集合下推到事实查询；该读失败时整个请求返回错误而不是把未知关系当作可见——fail-closed。参与者的私人旅程 ID 不在响应中暴露。Gateway 路由为 `GET /v1/routes/{route_id}/peers?cursor&limit`，游标语义与粉丝页一致。

## 热门路线计数

参与事实仍以 `route_participations` 为准。写入只按“用户 + 路线”获取事务 advisory lock，不再把同一热门路线的所有用户串行化；`0016_route_participation_sharded_counts.sql` 将每条路线的活跃人数按用户稳定哈希拆为 64 个固定分片，PostgreSQL 大版本升级不会改变已有用户的分片。数据库触发器只在 `inactive -> active` 或 `active -> inactive` 时增减分片，事实与计数位于同一事务，重复命令不会重复计数。`route_context` 和写响应最多聚合 64 行，不再扫描整条路线的参与事实。

迁移会在安装触发器后回填存量事实，并兼容尚未滚动完成的旧 BBS 实例。发布后可用下列只读查询检查事实与分片是否一致；返回结果必须为空：

```sql
WITH facts AS (
    SELECT route_id, COUNT(*)::BIGINT AS active_count
    FROM route_participations
    WHERE left_at IS NULL
    GROUP BY route_id
), counters AS (
    SELECT route_id, SUM(active_count)::BIGINT AS active_count
    FROM route_participation_count_shards
    GROUP BY route_id
)
SELECT
    COALESCE(facts.route_id, counters.route_id) AS route_id,
    COALESCE(facts.active_count, 0) AS fact_count,
    COALESCE(counters.active_count, 0) AS shard_count
FROM facts
FULL OUTER JOIN counters USING (route_id)
WHERE COALESCE(facts.active_count, 0) <> COALESCE(counters.active_count, 0);
```

## 环境变量

`BBS_ADDR` 和 `BBS_GRPC_ADDR`，默认分别监听 `127.0.0.1:8082`、`127.0.0.1:18002`。

## 生产化边界

`STORAGE_MODE=postgres` 使用 SQLx/PostgreSQL 持久化关系，并在拉黑事务中清理冲突关注。关系和可见性上下文通过 Redis protobuf 缓存加速，按用户使用 30 秒 TTL；缓存 miss 由进程内互斥和 Redis 刷新租约（2 秒 TTL）合并，写关系时递增版本并删除对应上下文，旧读回填会因版本不一致而被丢弃。版本键保留 120 秒，长于缓存但不会按历史用户永久增长。Redis 故障只禁用缓存并回退事实库；跨实例刷新仍未回填时，安全可见性调用返回 `UNAVAILABLE` 而不是把未知关系当作允许。公共路线已消除单路线全局写锁并使用分片精确计数。

关系 Outbox/审计事件、反滥用限流、超大关系账号的读写扩散、隐私权限和多地域图谱复制仍属于独立上线门禁，不由本服务的缓存伪装完成。
