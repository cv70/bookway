export type View = "overview" | "campaigns" | "audiences" | "reports";
export type Campaign = {
  id: string;
  name: string;
  goal: string;
  budget: string;
  spent: string;
  impressions: string;
  // Mirrors ad-center CampaignStatus: draft/active/paused. Review is the
  // platform's server-side process; the advertiser console only drafts,
  // activates, and pauses.
  state: "running" | "paused" | "draft";
  color: string;
  binding: ActionNodeBinding;
  // Serving creative synced to ad-center (rendered on the action node).
  title: string;
  body: string;
  imageUrl: string;
  landingUrl: string;
  frequencyCap: number;
  bid: number;
  // Delivery targeting slugs; empty arrays mean unrestricted.
  geoRegions: string[];
  deviceOs: string[];
  // Server-verified clicks from ad-center; 0 until the gateway answers.
  clicks: number;
  predictions: {
    pctr: number;
    pcvr: number;
    pwegu: number;
  };
};
// The scene context every ad must declare: a public route, the action node
// on it, and the equipment that node declares. The server revalidates all
// three against bbs-link on create, update, recall, and decision.
export type ActionNodeBinding = {
  routeId: string;
  actionNodeId: string;
  equipment: string;
};
// Only the server-enforced platform guardrail. Per-campaign impression caps
// live on each campaign; decision pacing lives in ad-main configuration.
export type DeliveryGuardrails = {
  userDailyCap: number;
};

export const formatBinding = (binding: ActionNodeBinding) =>
  `${binding.routeId} · ${binding.actionNodeId} · ${binding.equipment}`;

// 广告活动与投放护栏一律来自服务端网关；控制台内存态从空开始，
// 不做本地种子，也不持久化到 localStorage（业务事实宁可显示空/未连接）。

export const nav: [View, string, string][] = [
  ["overview", "▦", "概览"],
  ["campaigns", "◉", "广告活动"],
  ["audiences", "◌", "定向与护栏"],
  ["reports", "▥", "报告中心"],
];
export const titles: Record<View, string> = {
  overview: "概览",
  campaigns: "广告活动",
  audiences: "定向与护栏",
  reports: "报告中心",
};
