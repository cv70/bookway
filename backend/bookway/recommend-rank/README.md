# Recommend Rank

在线排序服务接收已过滤和打分的候选，按模型版本和稳定实验桶进行序列重排。默认 `recommend-rank-v2` 将本地候选分、质量、新鲜度、召回分与用户正负反馈、兴趣强度组合为可回滚的确定性排序，并同时使用 pCTR、pCVR、pWEGU 和 `route_completion_rate`；WEGU 与路线完成度权重高于点击和购买代理信号。模型推理可在同一协议下替换。

默认监听 `127.0.0.1:8096`（`RECOMMEND_RANK_ADDR`），模型版本由 `RECOMMEND_RANK_MODEL_VERSION` 配置。
