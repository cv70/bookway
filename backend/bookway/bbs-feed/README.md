# BBS Feed 信息流交付服务

## 职责

Feed 产品交付层。它负责把客户端的 Feed 请求转换为稳定的 Feed 产品契约，统一默认场景、分页上限和降级边界；真正的候选召回、排序和特征补全由 `recommend-main` 负责。

## 接口

- 内部 gRPC：`feed`。

Gateway 只访问 `bbs-feed`，客户端不直接访问推荐引擎。`surface=home` 交给个性化推荐流水线；`surface=following` 交给受信社交图谱约束的最新内容时间流，空关注集合不会回退到首页推荐。交付层对 `recommend-main` 使用进程级熔断器：连续传输/超时错误达到阈值后短路，冷却期只放行一个探测请求，避免下游故障占满 Gateway 的请求预算。

## 环境变量

- `BBS_FEED_ADDR`：默认 `127.0.0.1:8088`。
- `RECOMMEND_MAIN_GRPC_URL`：推荐主服务地址。

## 生产化待办

增加多产品 Feed surface、Redis 游标缓存、热点保护、内容安全二次过滤、运营配置、广告/活动插卡和独立 Feed SLO；当前产品 Feed 的召回、排序和场景广告仍由推荐主服务负责。
