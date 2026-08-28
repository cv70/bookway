import { useState } from "react";
import { AffiliateSettlement } from "../domain";

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
  onPayAffiliate,
  onExportAffiliates,
}: {
  affiliates: AffiliateSettlement[];
  onPayAffiliate: (settlementId: string) => void;
  onExportAffiliates: () => void;
}) {
  const [filter, setFilter] = useState("全部");
  // Statuses other than the tabs below (待生效 / 已冲正) surface under 全部.
  const rows = affiliates.filter(
    (item) =>
      filter === "全部" ||
      (filter === "待结算"
        ? item.status === "待生效" || item.status === "待结算"
        : item.status === "已结算"),
  );
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
            <h2>分账比例说明</h2>
            <p>
              佣金比例在商品挂载到路线行动节点（Offer）时按 0–30% 确定并冻结进每笔订单；
              结算金额以订单中的佣金快照为准，由平台分账台账执行，不在此单独配置。
            </p>
          </div>
        </div>
      </div>
    </section>
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
