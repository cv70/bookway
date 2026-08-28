import { Campaign, DeliveryGuardrails, View } from "../domain";
import { reRankAuctionCandidates } from "../lib/auction";

type Props = {
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails | null;
  go: (view: View) => void;
  open: () => void;
};

const impressionCount = (campaign: Campaign) =>
  Number(campaign.impressions.replace(/,/g, "")) || 0;

// 观察点击率只由服务端已验证的展示与点击计算；没有数据时显示 --，
// 不再用写死的百分比冒充统计。
const observedCtr = (campaign: Campaign) => {
  const impressions = impressionCount(campaign);
  return impressions > 0
    ? `${((campaign.clicks / impressions) * 100).toFixed(2)}%`
    : "--";
};

export function Overview({ campaigns, guardrails, go, open }: Props) {
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">场景化投放</p>
          <h1>投放概览</h1>
          <p className="muted">
            活动与消耗数据来自服务端广告账本，汇总指标请查看报告中心
          </p>
        </div>
        <div className="heading-actions">
          <button className="button primary" onClick={open}>
            ＋ 创建广告活动
          </button>
        </div>
      </div>
      <div className="guardrail">
        <span>✓</span>
        <p>
          <strong>场景化投放</strong>
          广告仅出现在用户主动进入的公开路线行动节点，频控与预算保护由服务端强制执行。
        </p>
        <button className="link-button" onClick={() => go("audiences")}>
          查看场景策略
        </button>
      </div>
      <CampaignSummary campaigns={campaigns} go={go} />
      <AuctionPreview campaigns={campaigns} guardrails={guardrails} />
      <section className="panel">
        <div className="panel-header">
          <div>
            <h2>投放指标</h2>
            <p>消耗、展示与点击汇总</p>
          </div>
          <button className="link-button" onClick={() => go("reports")}>
            前往报告中心
          </button>
        </div>
        <p className="panel-note">
          消耗、展示、点击等汇总指标来自 ad-center
          每日投放账本，请按区间在报告中心查看；转化指标未接入前不做展示。
        </p>
      </section>
    </section>
  );
}

function CampaignSummary({
  campaigns,
  go,
}: {
  campaigns: Campaign[];
  go: (view: View) => void;
}) {
  return (
    <section className="panel campaign-summary">
      <div className="panel-header">
        <div>
          <h2>广告活动</h2>
          <p>活动状态与服务端累计投放数据</p>
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
            <th>日预算</th>
            <th>消耗</th>
            <th>展示</th>
            <th>观察点击率</th>
          </tr>
        </thead>
        <tbody>
          {campaigns.length ? (
            campaigns.map((campaign) => (
              <tr key={campaign.id}>
                <td>
                  {campaign.name}
                  <small>目标：{campaign.goal}</small>
                </td>
                <td>
                  <Status state={campaign.state} />
                </td>
                <td>{campaign.budget}</td>
                <td>{campaign.spent}</td>
                <td>{campaign.impressions}</td>
                <td>{observedCtr(campaign)}</td>
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={6} className="empty-row">
                暂无广告活动。
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </section>
  );
}

function AuctionPreview({
  campaigns,
  guardrails,
}: {
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails | null;
}) {
  const nodes = Array.from(
    new Map(
      campaigns
        .filter((campaign) => campaign.state === "running")
        .map((campaign) => [
          `${campaign.binding.routeId}/${campaign.binding.actionNodeId}`,
          campaign.binding,
        ]),
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
      {!guardrails ? (
        <p className="empty-row">未读取到服务端护栏，暂无法预览节点竞价。</p>
      ) : nodes.length ? (
        <div className="auction-list">
          {nodes.map((binding) => {
            const result = reRankAuctionCandidates(
              campaigns,
              binding.actionNodeId,
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
              <div
                className="auction-row"
                key={`${binding.routeId}/${binding.actionNodeId}`}
              >
                <div>
                  <strong>
                    {binding.routeId} · {binding.actionNodeId}
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

function Status({ state }: { state: Campaign["state"] }) {
  return state === "draft" ? (
    <span className="status draft">草稿</span>
  ) : (
    <span className={`status ${state}`}>
      <i />
      {state === "running" ? "投放中" : "已暂停"}
    </span>
  );
}
