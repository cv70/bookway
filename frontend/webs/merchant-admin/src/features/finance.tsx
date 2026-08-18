import { FormEvent, useState } from "react";
import {
  actionNodes,
  AffiliateRule,
  AffiliateSettlement,
  formatActionNode,
  Settlement,
} from "../domain";

export function Finance({
  settlements,
  affiliates,
  affiliateRules,
  onExport,
  onExportAffiliates,
  onCreateAffiliateRule,
  onToggleAffiliateRule,
  onRemoveAffiliateRule,
}: {
  settlements: Settlement[];
  affiliates: AffiliateSettlement[];
  affiliateRules: AffiliateRule[];
  onExport: () => void;
  onExportAffiliates: () => void;
  onCreateAffiliateRule: (rule: Omit<AffiliateRule, "id">) => string | null;
  onToggleAffiliateRule: (id: string) => string | null;
  onRemoveAffiliateRule: (id: string) => void;
}) {
  const [filter, setFilter] = useState("全部");
  const [showRuleForm, setShowRuleForm] = useState(false);
  const [ruleError, setRuleError] = useState("");
  const [removingRule, setRemovingRule] = useState<AffiliateRule | null>(null);
  const rows = settlements.filter(
    (item) => filter === "全部" || item.status === filter,
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
            订单收入、平台服务费与创作者分账按结算周期对账。
          </p>
        </div>
        <button className="button secondary" onClick={onExport}>
          ⇩ 导出明细
        </button>
      </div>
      <div className="metric-grid">
        <Metric
          label="待结算金额"
          value="¥16,578.00"
          change="本期"
          note="预计 8 月 25 日"
        />
        <Metric
          label="累计已结算"
          value="¥85,420.00"
          change="↑ 12.4%"
          note="较上季度"
        />
        <Metric
          label="创作者分账"
          value="¥4,328.00"
          change="18 笔"
          note="路线贡献佣金"
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
            结算周期：半月 · 到账账户：尾号 4821
          </span>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                {[
                  "结算单",
                  "周期",
                  "订单成交",
                  "平台与分账",
                  "应付金额",
                  "状态",
                  "到账时间",
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((item) => (
                <tr key={item.id}>
                  <td>
                    <strong>{item.id}</strong>
                  </td>
                  <td>{item.period}</td>
                  <td>{item.gross}</td>
                  <td>{item.commission}</td>
                  <td>
                    <strong>{item.payable}</strong>
                  </td>
                  <td>
                    <Status value={item.status} />
                  </td>
                  <td>{item.date}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="panel table-panel affiliate-panel">
        <div className="panel-header">
          <div>
            <h2>创作者分账明细</h2>
            <p>仅归因于已挂载的路线行动节点</p>
          </div>
          <button className="button secondary" onClick={onExportAffiliates}>
            ⇩ 导出分账
          </button>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                {[
                  "分账单",
                  "创作者",
                  "路线 / 节点",
                  "成交订单",
                  "分账比例",
                  "应付金额",
                  "状态",
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {affiliates.map((item) => (
                <tr key={item.id}>
                  <td>
                    <strong>{item.id}</strong>
                  </td>
                  <td>{item.creator}</td>
                  <td>
                    <strong>{item.route}</strong>
                    <small>
                      {item.node} · {item.equipment}
                    </small>
                  </td>
                  <td>{item.orders}</td>
                  <td>{item.rate}</td>
                  <td>
                    <strong>{item.payable}</strong>
                  </td>
                  <td>
                    <Status value={item.status} />
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
function Status({ value }: { value: string }) {
  return (
    <span className={`status ${value === "已结算" ? "success" : "warning"}`}>
      {value}
    </span>
  );
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
