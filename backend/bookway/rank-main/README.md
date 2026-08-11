# rank-main 模型排序服务

排序服务是在线模型服务边界，把召回候选和特征转换为可解释的排序分数，并返回模型版本与实验桶。当前实现为可回滚的 `heuristic-v1`，通过 `RANK_MODEL_VERSION` 版本化；服务不可用时 `recommend-main` 保留基础启发式分并标记 Feed 降级。后续推理引擎应作为本服务内部 datasource 接入，并继续保持同一 API 和降级契约。
