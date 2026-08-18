import { useState } from "react";
import { Campaign, DeliveryGuardrails, View } from "../domain";
import { reRankAuctionCandidates } from "../lib/auction";

type Props = {
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails;
  go: (view: View) => void;
  open: () => void;
  onApplyBudget: (name: string) => void;
  notify: (message: string) => void;
};

export function Overview({
  campaigns,
  guardrails,
  go,
  open,
  onApplyBudget,
}: Props) {
  const [range, setRange] = useState("过去 7 天");
  const [applied, setApplied] = useState(false);
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">2026 年 8 月 12 日 - 8 月 18 日</p>
          <h1>投放概览</h1>
          <p className="muted">所有数据按已验证展示和点击实时归因</p>
        </div>
        <div className="heading-actions">
          <button
            className="button secondary"
            onClick={() =>
              setRange(range === "过去 7 天" ? "过去 30 天" : "过去 7 天")
            }
          >
            ▣ {range}
          </button>
          <button className="button primary" onClick={open}>
            ＋ 创建广告活动
          </button>
        </div>
      </div>
      <div className="guardrail">
        <span>✓</span>
        <p>
          <strong>投放状态健康</strong>
          所有展示均来自用户主动请求的路线场景，频控和预算保护已生效。
        </p>
        <button className="link-button" onClick={() => go("audiences")}>
          查看场景策略
        </button>
      </div>
      <Metrics />
      <section className="dashboard-grid">
        <Performance />
        <Budget />
      </section>
      <section className="panel campaign-summary">
        <div className="panel-header">
          <div>
            <h2>广告活动</h2>
            <p>最近 7 天表现</p>
          </div>
          <button className="link-button" onClick={() => go("campaigns")}>
            查看全部
          </button>
        </div>
        <table>
          <thead>
            <tr>
              <th>广告活动</th>
              <th>状态</th>
              <th>消耗</th>
              <th>展示</th>
              <th>点击率</th>
              <th>转化</th>
            </tr>
          </thead>
          <tbody>
            {campaigns.slice(0, 3).map((campaign, index) => (
              <tr key={campaign.name}>
                <td>
                  {campaign.name}
                  <small>目标：{campaign.goal}</small>
                </td>
                <td>
                  <Status state={campaign.state} />
                </td>
                <td>{campaign.spent}</td>
                <td>{campaign.impressions}</td>
                <td>{["4.16%", "3.51%", "2.78%"][index]}</td>
                <td>{[234, 96, 46][index]}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <AuctionPreview campaigns={campaigns} guardrails={guardrails} />
      <section className="lower-grid">
        <article className="panel">
          <div className="panel-header">
            <div>
              <h2>高效场景</h2>
              <p>基于已验证转化的路线节点</p>
            </div>
          </div>
          {["周末轻徒步入门", "城市骑行第一课", "夏日露营清单"].map(
            (name, index) => (
              <div className="scene-row" key={name}>
                <span className="scene-icon green">⌁</span>
                <div>
                  <strong>{name}</strong>
                  <small>装备准备 · 路线节点</small>
                </div>
                <b>{["6.8%", "5.4%", "4.1%"][index]}</b>
                <span>转化率</span>
              </div>
            ),
          )}
        </article>
        <article className="panel">
          <div className="panel-header">
            <div>
              <h2>投放建议</h2>
              <p>基于近 7 日表现</p>
            </div>
          </div>
          <div className="recommendation">
            <span>↗</span>
            <div>
              <strong>增加「秋日徒步装备推广」日预算</strong>
              <p>转化成本低于账户平均 23%，剩余预算充足。</p>
              <button
                className="link-button"
                onClick={() => {
                  setApplied(true);
                  onApplyBudget("秋日徒步装备推广");
                }}
              >
                {applied ? "已应用" : "应用建议"}
              </button>
            </div>
          </div>
        </article>
      </section>
    </section>
  );
}

function AuctionPreview({
  campaigns,
  guardrails,
}: {
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails;
}) {
  const nodes = Array.from(
    new Map(
      campaigns
        .filter((campaign) => campaign.state === "running")
        .map((campaign) => [campaign.binding.id, campaign.binding]),
    ).values(),
  );
  return (
    <section className="panel auction-panel">
      <div className="panel-header">
        <div>
          <h2>节点竞价预览</h2>
          <p>
            eCPM = 最高点击出价 × pCTR × 1,000；同节点内再以行动目标评分排序
          </p>
        </div>
      </div>
      {nodes.length ? (
        <div className="auction-list">
          {nodes.map((binding) => {
            const result = reRankAuctionCandidates(
              campaigns,
              binding.id,
              guardrails,
              {
                evaluatedAt: Date.now(),
                totalImpressionsToday: 0,
                campaignImpressionsToday: {},
                campaignLastShownAt: {},
                recentCampaignNames: [],
                recentRoutes: [],
              },
            );
            return (
              <div className="auction-row" key={binding.id}>
                <div>
                  <strong>
                    {binding.route} · {binding.node}
                  </strong>
                  <small>{binding.equipment}</small>
                </div>
                <ol>
                  {result.candidates.map((campaign, index) => (
                    <li key={campaign.name}>
                      <b>{index + 1}</b>
                      <span>{campaign.name}</span>
                      <strong>¥{campaign.ecpm.toFixed(2)} eCPM</strong>
                      <small>行动分 {campaign.actionScore.toFixed(3)}</small>
                    </li>
                  ))}
                </ol>
              </div>
            );
          })}
        </div>
      ) : (
        <p className="empty-row">暂无符合场景与审核护栏的投放候选。</p>
      )}
    </section>
  );
}

function Metrics() {
  return (
    <section className="metric-grid">
      {[
        ["消耗", "¥8,426.75", "↑ 14.2%", "up"],
        ["展示", "128,640", "↑ 8.5%", "up"],
        ["点击", "4,918", "↑ 10.1%", "up"],
        ["点击率", "3.82%", "↓ 0.1%", "down"],
        ["转化", "376", "↑ 17.8%", "up"],
      ].map(([label, value, delta, tone]) => (
        <article className="metric-card" key={label}>
          <p>{label}</p>
          <strong>{value}</strong>
          <span className={tone}>{delta}</span>
          <small>环比</small>
        </article>
      ))}
    </section>
  );
}
function Performance() {
  return (
    <article className="panel performance-panel">
      <div className="panel-header">
        <div>
          <h2>投放表现</h2>
          <p>消耗与转化趋势</p>
        </div>
      </div>
      <div className="bar-chart">
        <div className="scale">
          <span>¥1.6k</span>
          <span>¥800</span>
          <span>¥0</span>
        </div>
        <div className="chart-bars">
          {[48, 63, 55, 76, 60, 88, 73].map((height, index) => (
            <div className="bar-set" key={index}>
              <i style={{ height: `${height}%` }} />
              <small>{12 + index} 日</small>
            </div>
          ))}
        </div>
      </div>
    </article>
  );
}
function Budget() {
  return (
    <article className="panel budget-panel">
      <div className="panel-header">
        <div>
          <h2>预算消耗</h2>
          <p>本月账户预算</p>
        </div>
      </div>
      <div className="budget-ring">
        <div>
          <strong>42%</strong>
          <small>已消耗</small>
        </div>
      </div>
      <div className="budget-total">
        <span>
          <small>本月预算</small>
          <strong>¥20,000.00</strong>
        </span>
        <span>
          <small>剩余可用</small>
          <strong>¥11,573.25</strong>
        </span>
      </div>
    </article>
  );
}
function Status({ state }: { state: Campaign["state"] }) {
  return state === "draft" ? (
    <span className="status draft">草稿</span>
  ) : state === "pending" ? (
    <span className="status pending">
      <i />
      审核中
    </span>
  ) : (
    <span className={`status ${state}`}>
      <i />
      {state === "running" ? "投放中" : "已暂停"}
    </span>
  );
}
