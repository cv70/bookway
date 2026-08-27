import { FormEvent, useState } from "react";
import {
  actionNodes,
  AffiliateRule,
  AffiliateSettlement,
  formatActionNode,
} from "../domain";

const payableCents = (payable: string) => {
  const amount = Number(payable.replace(/[¥,]/g, ""));
  return Number.isFinite(amount) ? Math.round(amount * 100) : 0;
};

const sumPayable = (items: AffiliateSettlement[]) =>
  `¥${(
    items.reduce((sum, item) => sum + payableCents(item.payable), 0) / 100
  ).toFixed(2)}`;

export function Finance({
  affiliates,
  affiliateRules,
  onPayAffiliate,
  onExportAffiliates,
  onCreateAffiliateRule,
  onToggleAffiliateRule,
  onRemoveAffiliateRule,
}: {
  affiliates: AffiliateSettlement[];
  affiliateRules: AffiliateRule[];
  onPayAffiliate: (settlementId: string) => void;
  onExportAffiliates: () => void;
  onCreateAffiliateRule: (rule: Omit<AffiliateRule, "id">) => string | null;
  onToggleAffiliateRule: (id: string) => string | null;
  onRemoveAffiliateRule: (id: string) => void;
}) {
  const [filter, setFilter] = useState("全部");
  const [showRuleForm, setShowRuleForm] = useState(false);
  const [ruleError, setRuleError] = useState("");
  const [removingRule, setRemovingRule] = useState<AffiliateRule | null>(null);
  // Statuses other than the tabs below (待生效 / 已冲正) surface under 全部.
  const rows = affiliates.filter(
    (item) =>
      filter === "全部" ||
      (filter === "待结算"
        ? item.status === "待生效" || item.status === "待结算"
        : item.status === "已结算"),
  );
  const submitRule = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const binding = actionNodes.find(
      (node) => node.id === data.get("actionNodeId"),
    );
    const creator = String(data.get("creator") || "").trim();
    const rate = Number(data.get("rate"));
    if (
      !binding ||
      !creator ||
      !Number.isInteger(rate) ||
      rate < 1 ||
      rate > 50
    ) {
      setRuleError("请选择行动节点，并填写 1 到 50 的整数分账比例。");
      return;
    }
    const error = onCreateAffiliateRule({
      ...binding,
      creator,
      rate,
      enabled: true,
    });
    if (error) {
      setRuleError(error);
      return;
    }
    setRuleError("");
    setShowRuleForm(false);
  };
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">账务与对账</p>
          <h1>财务结算</h1>
          <p className="muted">
            创作者分账来自订单支付成功后的分账台账，可对待结算分账单执行打款。
          </p>
        </div>
        <button className="button secondary" onClick={onExportAffiliates}>
          ⇩ 导出分账
        </button>
      </div>
      <div className="metric-grid">
        <Metric
          label="待打款分账"
          value={sumPayable(
            affiliates.filter((item) => item.status === "待结算"),
          )}
          change={`${affiliates.filter((item) => item.status === "待结算").length} 笔`}
          note="已到期，等待商家打款"
        />
        <Metric
          label="累计已结算"
          value={sumPayable(
            affiliates.filter((item) => item.status === "已结算"),
          )}
          change={`${affiliates.filter((item) => item.status === "已结算").length} 笔`}
          note="已完成打款"
        />
        <Metric
          label="已冲正分账"
          value={sumPayable(
            affiliates.filter((item) => item.status === "已冲正"),
          )}
          change={`${affiliates.filter((item) => item.status === "已冲正").length} 笔`}
          note="关联订单退款后作废"
          tone="neutral"
        />
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <div className="segmented">
            {["全部", "待结算", "已结算"].map((tab) => (
              <button
                className={filter === tab ? "selected" : ""}
                onClick={() => setFilter(tab)}
                key={tab}
              >
                {tab}
              </button>
            ))}
          </div>
          <span className="report-note">
            每笔订单一条分账 · 打款后状态变为已结算
          </span>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                {[
                  "分账单",
                  "订单",
                  "创作者",
                  "应付金额",
                  "状态",
                  "时间",
                  "",
                ].map((label) => (
                  <th key={label || "actions"}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((item) => (
                <tr key={item.id}>
                  <td>
                    <strong>{item.id}</strong>
                  </td>
                  <td>{item.order_id}</td>
                  <td>{item.creator}</td>
                  <td>
                    <strong>{item.payable}</strong>
                  </td>
                  <td>
                    <Status value={item.status} />
                  </td>
                  <td>{item.date}</td>
                  <td>
                    {item.status === "待结算" && (
                      <button
                        className="table-action"
                        onClick={() => onPayAffiliate(item.id)}
                      >
                        打款
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="panel table-panel affiliate-panel">
        <div className="panel-header">
          <div>
            <h2>创作者分账规则</h2>
            <p>按行动节点归因；同一节点的启用创作者分账合计不超过 50%</p>
          </div>
          <button
            className="button primary"
            onClick={() => setShowRuleForm(true)}
          >
            ＋ 新增规则
          </button>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>创作者</th>
                <th>路线 / 节点 / 场景装备</th>
                <th>分账比例</th>
                <th>状态</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {affiliateRules.map((rule) => (
                <tr key={rule.id}>
                  <td>
                    <strong>{rule.creator}</strong>
                  </td>
                  <td>
                    <strong>{rule.route}</strong>
                    <small>
                      {rule.node} · {rule.equipment}
                    </small>
                  </td>
                  <td>{rule.rate}%</td>
                  <td>
                    <span
                      className={`status ${rule.enabled ? "success" : "neutral"}`}
                    >
                      {rule.enabled ? "已启用" : "已停用"}
                    </span>
                  </td>
                  <td>
                    <div className="table-actions">
                      <button
                        className="table-action"
                        onClick={() => {
                          const error = onToggleAffiliateRule(rule.id);
                          setRuleError(error || "");
                        }}
                      >
                        {rule.enabled ? "停用" : "启用"}
                      </button>
                      <button
                        className="table-action danger-action"
                        onClick={() => setRemovingRule(rule)}
                      >
                        移除
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      {showRuleForm && (
        <AffiliateRuleDialog
          error={ruleError}
          close={() => {
            setRuleError("");
            setShowRuleForm(false);
          }}
          submit={submitRule}
        />
      )}
      {removingRule && (
        <RemoveAffiliateRuleDialog
          rule={removingRule}
          close={() => setRemovingRule(null)}
          remove={() => {
            onRemoveAffiliateRule(removingRule.id);
            setRemovingRule(null);
          }}
        />
      )}
    </section>
  );
}

function AffiliateRuleDialog({
  error,
  close,
  submit,
}: {
  error: string;
  close: () => void;
  submit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={submit}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">Affiliate 分账</p>
            <h2>新增创作者规则</h2>
          </div>
          <button type="button" className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <label>
          创作者
          <input name="creator" required placeholder="例如：山野阿柠" />
        </label>
        <label>
          路线行动节点与场景装备
          <select name="actionNodeId" required>
            {actionNodes.map((node) => (
              <option value={node.id} key={node.id}>
                {formatActionNode(node)}
              </option>
            ))}
          </select>
        </label>
        <label>
          分账比例（%）
          <input
            name="rate"
            type="number"
            min="1"
            max="50"
            required
            defaultValue="10"
          />
        </label>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">保存规则</button>
        </div>
      </form>
    </div>
  );
}
function Metric({
  label,
  value,
  change,
  note,
  tone = "up",
}: {
  label: string;
  value: string;
  change: string;
  note: string;
  tone?: string;
}) {
  return (
    <article className="metric-card">
      <p>{label}</p>
      <strong>{value}</strong>
      <span className={tone}>{change}</span>
      <small>{note}</small>
    </article>
  );
}
function Status({ value }: { value: AffiliateSettlement["status"] }) {
  const tone =
    value === "已结算"
      ? "success"
      : value === "待结算"
        ? "warning"
        : value === "已冲正"
          ? "danger-status"
          : "neutral";
  return <span className={`status ${tone}`}>{value}</span>;
}

function RemoveAffiliateRuleDialog({
  rule,
  close,
  remove,
}: {
  rule: AffiliateRule;
  close: () => void;
  remove: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">Affiliate 分账</p>
            <h2>移除分账规则</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <p className="modal-copy">
          将停止“{rule.creator}”在“{rule.route} · {rule.node}
          ”的后续订单分账，已生成的结算明细不会修改。
        </p>
        <div className="dialog-actions">
          <button className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button danger-button" onClick={remove}>
            确认移除
          </button>
        </div>
      </div>
    </div>
  );
}
