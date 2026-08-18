import { Campaign, DeliveryGuardrails } from "../domain";

export type AuctionCandidate = Campaign & {
  ecpm: number;
  actionScore: number;
};
export type DeliveryHistory = {
  evaluatedAt: number;
  totalImpressionsToday: number;
  campaignImpressionsToday: Record<string, number>;
  campaignLastShownAt: Record<string, number>;
  recentCampaignNames: string[];
  recentRoutes: string[];
};
export type GuardrailResult = {
  candidates: AuctionCandidate[];
  blocked: string[];
};

export function calculateEcpm(campaign: Campaign) {
  return campaign.bid * campaign.predictions.pctr * 1000;
}

export function calculateActionScore(campaign: Campaign) {
  const { pctr, pcvr, pwegu } = campaign.predictions;
  return pctr * 0.2 + pcvr * 0.3 + pwegu * 0.5;
}

export function rankAuctionCandidates(
  campaigns: Campaign[],
  actionNodeId: string,
) {
  return campaigns
    .filter(
      (campaign) =>
        campaign.state === "running" && campaign.binding.id === actionNodeId,
    )
    .map((campaign) => ({
      ...campaign,
      ecpm: calculateEcpm(campaign),
      actionScore: calculateActionScore(campaign),
    }))
    .sort(
      (left, right) =>
        right.ecpm * right.actionScore - left.ecpm * left.actionScore,
    );
}

export function reRankAuctionCandidates(
  campaigns: Campaign[],
  actionNodeId: string,
  guardrails: DeliveryGuardrails,
  history: DeliveryHistory,
): GuardrailResult {
  if (history.totalImpressionsToday >= guardrails.userDailyCap) {
    return { candidates: [], blocked: ["已达到用户全局日曝光上限"] };
  }
  const base = rankAuctionCandidates(campaigns, actionNodeId);
  const lastCampaign = history.recentCampaignNames.at(-1);
  const lastRoute = history.recentRoutes.at(-1);
  const consecutiveSameRoute =
    history.recentRoutes.length >= guardrails.sameRouteLimit &&
    history.recentRoutes
      .slice(-guardrails.sameRouteLimit)
      .every((route) => route === lastRoute);
  const blocked: string[] = [];
  const eligible = base.filter((candidate) => {
    if (
      (history.campaignImpressionsToday[candidate.name] || 0) >=
      candidate.frequencyCap
    ) {
      blocked.push(`${candidate.name} 已达到活动日频控`);
      return false;
    }
    const lastShownAt = history.campaignLastShownAt[candidate.name];
    if (
      candidate.name === lastCampaign &&
      lastShownAt !== undefined &&
      history.evaluatedAt - lastShownAt < guardrails.cooldownMinutes * 60_000
    ) {
      blocked.push(`${candidate.name} 处于活动冷却期`);
      return false;
    }
    if (consecutiveSameRoute && lastRoute === candidate.binding.route) {
      blocked.push(`${candidate.name} 触发连续同路线限制`);
      return false;
    }
    return true;
  });
  const recentRoutes = new Set(history.recentRoutes);
  return {
    candidates: eligible.sort((left, right) => {
      const leftScore =
        left.ecpm *
        left.actionScore *
        (recentRoutes.has(left.binding.route)
          ? 1
          : 1 + guardrails.explorationShare / 100);
      const rightScore =
        right.ecpm *
        right.actionScore *
        (recentRoutes.has(right.binding.route)
          ? 1
          : 1 + guardrails.explorationShare / 100);
      return rightScore - leftScore;
    }),
    blocked,
  };
}
