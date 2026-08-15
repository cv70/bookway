# feature-main 特征服务

特征服务统一提供用户兴趣、近期行为、内容质量和负反馈特征。在线读取优先从 Redis 获取，PostgreSQL `user_features` 和 `user_events` 作为事实与回填来源；不存在特征时返回版本化默认值，避免推荐主链路因特征平台抖动中断。它会从近 90 天可关联公开内容的高意图行为派生 `domain_interest.*`，供召回阶段补充用户实际偏好的领域；候选级领域/作者亲和度仍供精排使用。`save_knowledge` 表示用户把公开内容收集进可继续整理、关联 Journey 和阅读的私有知识库，权重为 4，强于普通 `bookmark`（3）但低于实际 `join_route`/`complete`（5）。结构化隐藏反馈不会被粗暴视作同一种拒绝：`not_relevant` 只降低相应领域，`already_seen` 只抑制重复内容，`low_quality` 主要降低创作者亲和度。`user-event` 在新事件落库后会主动失效对应的 Redis 用户特征键，因此下一次排序会重新派生兴趣与负反馈信号；Redis 初始化受 `REDIS_CONNECT_TIMEOUT_MS` 约束，连接失败时禁用缓存并回退 PostgreSQL。默认监听 `8093`。
