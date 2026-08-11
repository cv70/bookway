# feature-main 特征服务

特征服务统一提供用户兴趣、近期行为、内容质量和负反馈特征。在线读取优先从 Redis 获取，PostgreSQL `user_features` 作为事实与回填来源；不存在特征时返回版本化默认值，避免推荐主链路因特征平台抖动中断。Redis 初始化受 `REDIS_CONNECT_TIMEOUT_MS` 约束，连接失败时禁用缓存并回退 PostgreSQL。默认监听 `8093`。
