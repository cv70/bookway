# Recommend Recall

`recommend-recall` 是候选召回服务。首页并行读取优质、新鲜、用户兴趣域内容索引和语义召回，合并重复候选的召回理由、排除已看内容，并返回统一候选协议。通用首页在截断前先保留每个来源的有序候选批次：可用的新鲜源、语义源和每个兴趣源各获得一个探索名额，随后按 `quality:4 / fresh:2 / semantic:2 / interest:1` 的固定轮次补齐，最后才按全局召回分数填满剩余容量。这样优质内容仍占主导，同时高分质量源不能完全挤掉新鲜、语义或兴趣探索；重复内容只占一个位置，且会保留全部来源理由。每个成功来源都会输出抓取量、可用量和耗尽状态的结构化日志，便于观察来源健康度。

语义召回是真实的向量检索：它把 Recommend Main 传入的已水合兴趣域拼成查询文本（与 Recommend Rank 的用户上下文同一套标签），经 knowledge-catalog 的 `EmbedTexts` 嵌入，再调用 BBS Search 的 `SearchSemantic` 取最近邻窗口，最后用 BBS Link 的公开摘要批量为命中重建权威候选（kNN 相似度顺序即相关性顺序，召回强度按窗口内名次归一）。该链路每个 feed 只产出一个窗口（`SearchSemantic` 无续页令牌），游标随即记录耗尽。任一上游失败只会让该次请求标记 `degraded=true` 并在下一页重试；嵌入服务未向量化、用户无兴趣信号都只是空窗口而非故障。

每个召回源在 `v2` 版本化游标中维护独立的位置与完成状态，避免不同排序或领域索引共用偏移量。任一召回源失败时保留其余链路并标记 `degraded=true`，并在后续页保留失败源的位置以供重试；其他游标版本会从当前契约重新开始。

关注时间流由 Recommend Main 传入受信社交图谱派生的 `following_author_ids`，只调用 BBS Link 的批量作者 `fresh` 查询并保留其最新优先顺序；它不扩展兴趣域、不混入全局优质候选。空关注集合是正常的空时间流，绝不降级成“为你推荐”；无效或超过 5,000 位作者的受信批次会 fail-closed 并标记 `degraded=true`。关注游标还绑定规范化作者集合的 SHA-256 指纹；用户的关注关系在翻页期间变化时，旧偏移会被丢弃并从当前集合的第一页重新开始，绝不将旧窗口混入新集合。

默认监听 `127.0.0.1:8095`（`RECOMMEND_RECALL_ADDR`），内容上游由 `BBS_LINK_GRPC_URL` 配置。`RECALL_SOURCE_BLEND=balanced-v1` 是默认的来源混合策略；紧急回滚可显式设为 `score-v1`，恢复旧的全局召回分数截断。语义召回源只在 `RECALL_SEMANTIC_BBS_SEARCH_URL` 与 `RECALL_SEMANTIC_KNOWLEDGE_CATALOG_URL` 同时配置时注册（未配置即不存在该源，其余来源不受影响）；两个端点分别指向 BBS Search 与 knowledge-catalog，后者通过 `RAG_EMBEDDING_ENDPOINT` 接入真实嵌入服务。Recall 会将实际版本返回给 Recommend Main，后者把它写入曝光和响应的 `pipeline_id`，因此不能出现未记录的策略切换。启用服务鉴权时请求会携带 `x-service-token`。
