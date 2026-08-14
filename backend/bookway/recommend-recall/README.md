# Recommend Recall

`recommend-recall` 是候选召回服务。它并行读取优质、新鲜和用户兴趣域内容索引，合并重复候选的召回理由、排除已看内容，并返回统一候选协议；任一召回源失败时保留其余链路并标记 `degraded=true`。

默认监听 `127.0.0.1:8095`（`RECOMMEND_RECALL_ADDR`），内容上游由 `BBS_LINK_GRPC_URL` 配置。
