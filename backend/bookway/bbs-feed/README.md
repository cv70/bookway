# BBS Feed 信息流交付服务

## 职责

Feed 产品交付层。它负责把客户端的 Feed 请求转换为稳定的 Feed 产品契约，统一默认场景、分页上限和降级边界；真正的候选召回、排序和特征补全由 `recommend-main` 负责。

## 接口

- `GET /internal/v1/feed`

Gateway 只访问 `bbs-feed`，客户端不直接访问推荐引擎。未来可在这里加入关注流、编辑精选、运营插卡、广告位、缓存和不同端的 Feed 组装策略，而不改变推荐模型服务。

## 环境变量

- `BBS_FEED_ADDR`：默认 `127.0.0.1:8088`。
- `RECOMMEND_MAIN_URL`：推荐主服务地址。

## 生产化待办

增加多产品 Feed surface、Redis 游标缓存、热点保护、内容安全二次过滤、运营配置、广告/活动插卡和独立 Feed SLO。
