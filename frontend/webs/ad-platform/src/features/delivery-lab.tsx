import { useMemo, useState } from "react";
import {
  ActionNodeBinding,
  Campaign,
  DeliveryGuardrails,
  Scene,
} from "../domain";
import { DeliveryHistory, reRankAuctionCandidates } from "../lib/auction";

type Props = {
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails;
  scenes: Scene[];
};

const emptyHistory: DeliveryHistory = {
  evaluatedAt: Date.now(),
  totalImpressionsToday: 0,
  campaignImpressionsToday: {},
  campaignLastShownAt: {},
  recentCampaignNames: [],
  recentRoutes: [],
};

export function DeliveryLab({ campaigns, guardrails, scenes }: Props) {
  const bindings = scenes
    .filter((scene) => scene.enabled)
    .map<ActionNodeBinding>((scene) => ({
      id: scene.id,
      routeId: scene.id,
      route: scene.name,
      node: scene.node,
      equipment: scene.equipment,
    }));
  const [selectedId, setSelectedId] = useState(bindings[0]?.id || "");
  const [history, setHistory] = useState<DeliveryHistory>(emptyHistory);
  const binding =
    bindings.find((item) => item.id === selectedId) || bindings[0];
  const result = useMemo(
    () =>
      binding
        ? reRankAuctionCandidates(campaigns, binding.id, guardrails, history)
        : { candidates: [], blocked: ["没有启用的路线行动节点"] },
    [binding, campaigns, guardrails, history],
  );
  const simulate = () => {
    const winner = result.candidates[0];
    if (!winner) return;
    const now = Date.now();
    setHistory((current) => ({
      evaluatedAt: now,
      totalImpressionsToday: current.totalImpressionsToday + 1,
      campaignImpressionsToday: {
        ...current.campaignImpressionsToday,
        [winner.name]: (current.campaignImpressionsToday[winner.name] || 0) + 1,
      },
      campaignLastShownAt: {
        ...current.campaignLastShownAt,
        [winner.name]: now,
      },
      recentCampaignNames: [...current.recentCampaignNames, winner.name].slice(
        -8,
      ),
      recentRoutes: [...current.recentRoutes, winner.binding.route].slice(-8),
    }));
  };
  return (
    <section className="panel delivery-lab">
      <div className="panel-header">
        <div>
          <h2>投放护栏演练</h2>
          <p>以单个模拟用户的当天曝光历史进行节点预检</p>
        </div>
        <button
          className="button secondary"
          onClick={() => setHistory(emptyHistory)}
        >
          重置
        </button>
      </div>
      <div className="lab-controls">
        <label>
          路线行动节点
          <select
            value={binding?.id || ""}
            onChange={(event) => {
              setSelectedId(event.target.value);
              setHistory(emptyHistory);
            }}
          >
            {bindings.map((item) => (
              <option value={item.id} key={item.id}>
                {item.route} · {item.node} · {item.equipment}
              </option>
            ))}
          </select>
        </label>
        <button
          className="button primary"
          disabled={!result.candidates.length}
          onClick={simulate}
        >
          模拟一次曝光
        </button>
      </div>
      <div className="lab-results">
        <span>
          <small>今日曝光</small>
          <strong>
            {history.totalImpressionsToday} / {guardrails.userDailyCap}
          </strong>
        </span>
        <span>
          <small>下一候选</small>
          <strong>{result.candidates[0]?.name || "已被护栏拦截"}</strong>
        </span>
        <span>
          <small>节点 eCPM</small>
          <strong>
            {result.candidates[0]
              ? `¥${result.candidates[0].ecpm.toFixed(2)}`
              : "--"}
          </strong>
        </span>
      </div>
      {result.blocked.length > 0 && (
        <ul className="policy-list blocked-list">
          {result.blocked.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      )}
    </section>
  );
}
