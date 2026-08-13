# content-audit 内容审核服务

内容审核服务负责文本规则、外部审核提供商结果归一化、风险分数、审核版本和人工复审状态。当前实现提供可运行的规则引擎，并将每次决策持久化到 `content_audits`；后续云审核或自研模型只需作为新的 provider 接入，不改变 `bbs-link` 状态机。

内部 gRPC `audit` 返回 `approved`、`reviewing` 或 `restricted`。高风险禁止词进入受限，中风险健康/广告内容进入复审，其余允许发布。默认监听 `8092`。
