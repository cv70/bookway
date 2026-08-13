# Recommend Rank

在线排序服务接收已过滤和打分的候选，按模型版本和稳定实验桶进行序列重排；当前默认实现是无外部模型依赖的可回滚近似，模型推理可在同一协议下替换。

默认监听 `127.0.0.1:8096`（`RECOMMEND_RANK_ADDR`），模型版本由 `RECOMMEND_RANK_MODEL_VERSION` 配置。
