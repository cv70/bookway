# Recommend Main 推荐主服务

## 职责

在线推荐引擎，负责候选编排、补全、过滤、打分、多样性重排和曝光事件，不持有内容或社交事实数据。

## 流水线

```text
Query Hydration
-> 多路内容召回
-> BBS 社交图谱补全
-> 点赞/收藏状态补全
-> 安全与已看过滤
-> 质量/意图/多样性打分
-> Selector
-> 异步曝光副作用
```

## 依赖与环境变量

- 依赖：`bbs-link`、`bbs`、`commonlikestatus`、`feature-main`、`rank-main`。
- `RECOMMEND_MAIN_ADDR`：默认 `127.0.0.1:8083`。
- `BBS_LINK_URL`、`BBS_URL`、`LIKE_STATUS_URL`：上游服务地址。
- `FEATURE_MAIN_URL`、`RANK_MAIN_URL`：在线特征和排序服务地址。

`STORAGE_MODE=postgres` 时曝光及曝光条目持久化到 PostgreSQL。远程特征或模型不可用时保留流水线启发式得分，并返回 `meta.degraded=true`。

## 生产化待办

增加独立召回索引、训练/Feature Registry、在线实验配置、负反馈、事件回放、热点保护、模型漂移监控和一键回滚。Feed 产品编排已经由 `bbs-feed` 独立承接，推荐主服务保持在线决策边界。
