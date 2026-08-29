# feature-main 特征服务

候选的 pWEGU、pCTR、pCVR 使用近 90 天总体行为作为平滑先验，再按当前用户的有效曝光和加入样本量逐步切换到个性化比率。路线使用 Gateway 验证后的 `complete` / `join_route`，并要求同一用户先加入再完成；知识内容使用 `complete` / `save_knowledge`。冷启动路线因此不会被当成零转化，非路线内容不会获得路线完成或购买信号。路线候选另外输出 `route_completion_rate`，按 distinct 完成用户 / distinct 加入用户计算，作为“整条路线是否真的走完”的独立排序目标，避免把重复完成动作或客户端自报完成误当作路线完成度。

特征服务统一提供用户兴趣、近期行为、内容质量和负反馈特征。在线读取优先从 Redis 获取，PostgreSQL `user_features` 和 `user_events` 作为事实与回填来源；不存在特征时返回版本化默认值，避免推荐主链路因特征平台抖动中断。匿名请求只返回固定冷启动先验，不访问 Redis 或 PostgreSQL，也不会使用共享的空用户键。它会从近 90 天可关联公开内容的高意图行为派生 `domain_interest.*`，供召回阶段补充用户实际偏好的领域；候选级领域/作者亲和度仍供精排使用。`save_knowledge` 表示用户把公开内容收集进可继续整理、关联 Journey 和阅读的私有知识库，权重为 4，强于普通 `bookmark`（3）但低于实际 `join_route`/`complete`（5）。结构化隐藏反馈不会被粗暴视作同一种拒绝：`not_relevant` 只降低相应领域，`already_seen` 只抑制重复内容，`low_quality` 主要降低创作者亲和度。`user-event` 在新事件落库后会主动失效对应的 Redis 用户特征键，因此下一次排序会重新派生兴趣与负反馈信号；Redis 初始化受 `REDIS_CONNECT_TIMEOUT_MS` 约束，连接失败时禁用缓存并回退 PostgreSQL。默认监听 `8093`。

`job/feature-snapshot` 是由外部调度器触发的一次性离线聚合作业。它从已验证的 `user_events` 和公开内容领域聚合最近窗口，写入 `user_feature_snapshots` 的版本、计算时间、窗口、TTL、数值特征和来源血缘；同一用户、版本和 `as_of` 可安全重跑，过期快照会清理。示例：

```bash
FEATURE_SNAPSHOT_VERSION=heuristic-v1 \
FEATURE_SNAPSHOT_WINDOW_DAYS=90 \
FEATURE_SNAPSHOT_TTL_DAYS=14 \
cargo run -p bookway-feature-snapshot
```

在线读取会优先加载同版本且未过期的快照，再用实时事件和 `user_features` 覆盖；因此实时事件查询短暂异常时仍可使用有明确血缘的旧特征，而不会把无来源的默认值误认为真实偏好。

Redis 热键失效时，服务先使用进程内按用户互斥，再尝试 `bookway:features:refresh-lock:<user_id>` 分布式刷新租约（5 秒 TTL、compare-and-delete 释放）。持有租约的实例完成 PostgreSQL 回填并写入缓存后，其他实例会重新读取缓存；Redis 故障或租约释放异常不会阻断特征读取，租约 TTL 负责清理崩溃实例遗留状态。
