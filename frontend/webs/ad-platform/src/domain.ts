export type View = "overview" | "campaigns" | "ads" | "audiences" | "reports";
export type Campaign = {
  id: string;
  name: string;
  goal: string;
  budget: string;
  spent: string;
  impressions: string;
  state: "running" | "paused" | "draft" | "pending";
  color: string;
  binding: ActionNodeBinding;
  creativeName: string;
  frequencyCap: number;
  bid: number;
  predictions: {
    pctr: number;
    pcvr: number;
    pwegu: number;
  };
};
export type ActionNodeBinding = {
  id: string;
  routeId: string;
  route: string;
  node: string;
  equipment: string;
};
export type Creative = {
  name: string;
  format: string;
  binding: ActionNodeBinding;
  contextNote: string;
  status: "已通过" | "审核中" | "需修改";
  updated: string;
  assetData?: string;
};
export type Scene = {
  id: string;
  name: string;
  node: string;
  equipment: string;
  audience: string;
  reach: string;
  enabled: boolean;
};
export type DeliveryGuardrails = {
  userDailyCap: number;
  campaignDailyCap: number;
  cooldownMinutes: number;
  explorationShare: number;
  sameRouteLimit: number;
};
export const guardrailSeed: DeliveryGuardrails = {
  userDailyCap: 8,
  campaignDailyCap: 3,
  cooldownMinutes: 20,
  explorationShare: 30,
  sameRouteLimit: 2,
};

export const actionNodes: ActionNodeBinding[] = [
  {
    id: "scene-hike-gear",
    routeId: "route-weekend-hike",
    route: "周末轻徒步入门",
    node: "装备准备",
    equipment: "轻量背包与防磨徒步袜",
  },
  {
    id: "scene-bike-check",
    routeId: "route-city-bike",
    route: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
  },
  {
    id: "scene-camp-light",
    routeId: "route-summer-camp",
    route: "夏日露营清单",
    node: "夜间照明",
    equipment: "营地灯与备用电源",
  },
];

export const formatBinding = (binding: ActionNodeBinding) =>
  `${binding.route} · ${binding.node} · ${binding.equipment}`;

export const seed: Campaign[] = [
  {
    id: "campaign-hike",
    name: "秋日徒步装备推广",
    goal: "商品成交",
    budget: "¥1,200",
    spent: "¥4,918.30",
    impressions: "72,481",
    state: "running",
    color: "navy",
    binding: actionNodes[0],
    creativeName: "秋日徒步背包主视觉",
    frequencyCap: 3,
    bid: 2.6,
    predictions: { pctr: 0.0416, pcvr: 0.1274, pwegu: 0.0718 },
  },
  {
    id: "campaign-bike",
    name: "城市骑行系列",
    goal: "落地页访问",
    budget: "¥800",
    spent: "¥2,764.15",
    impressions: "39,044",
    state: "running",
    color: "cyan",
    binding: actionNodes[1],
    creativeName: "骑行装备清单",
    frequencyCap: 3,
    bid: 2.2,
    predictions: { pctr: 0.0351, pcvr: 0.0938, pwegu: 0.0542 },
  },
  {
    id: "campaign-camp",
    name: "露营新手季",
    goal: "商品成交",
    budget: "¥600",
    spent: "¥744.30",
    impressions: "17,115",
    state: "paused",
    color: "violet",
    binding: actionNodes[2],
    creativeName: "露营灯场景图",
    frequencyCap: 2,
    bid: 1.8,
    predictions: { pctr: 0.0278, pcvr: 0.1046, pwegu: 0.0693 },
  },
  {
    id: "campaign-cup",
    name: "保温杯秋季新品",
    goal: "商品成交",
    budget: "¥500",
    spent: "¥0.00",
    impressions: "0",
    state: "draft",
    color: "gray",
    binding: actionNodes[0],
    creativeName: "秋日徒步背包主视觉",
    frequencyCap: 3,
    bid: 1.5,
    predictions: { pctr: 0.031, pcvr: 0.081, pwegu: 0.048 },
  },
];

export const creativeSeed: Creative[] = [
  {
    name: "秋日徒步背包主视觉",
    format: "路线节点卡片",
    binding: actionNodes[0],
    contextNote: "在装备准备节点说明背负调节与防磨袜的搭配方式。",
    status: "已通过",
    updated: "今天 09:24",
  },
  {
    name: "骑行装备清单",
    format: "节点信息流",
    binding: actionNodes[1],
    contextNote: "配合出发前检查清单说明头盔与补给的使用要点。",
    status: "已通过",
    updated: "昨天 18:40",
  },
  {
    name: "露营灯场景图",
    format: "路线装备推荐",
    binding: actionNodes[2],
    contextNote: "在夜间照明节点展示营地灯的安全布置方式。",
    status: "需修改",
    updated: "8 月 16 日",
  },
];
export const sceneSeed: Scene[] = [
  {
    id: "scene-hike-gear",
    name: "周末轻徒步入门",
    node: "装备准备",
    equipment: "轻量背包与防磨徒步袜",
    audience: "主动查看节点",
    reach: "12,480 人",
    enabled: true,
  },
  {
    id: "scene-bike-check",
    name: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
    audience: "完成上一步打卡",
    reach: "8,920 人",
    enabled: true,
  },
  {
    id: "scene-camp-light",
    name: "夏日露营清单",
    node: "夜间照明",
    equipment: "营地灯与备用电源",
    audience: "收藏路线用户",
    reach: "5,160 人",
    enabled: false,
  },
];
export const reportRows = [
  [
    "秋日徒步装备推广",
    "商品成交",
    "¥4,918.30",
    "72,481",
    "234",
    "¥21.02",
    "4.16%",
    "12.74%",
    "7.18%",
    "42.6%",
  ],
  [
    "城市骑行系列",
    "落地页访问",
    "¥2,764.15",
    "39,044",
    "96",
    "¥28.79",
    "3.51%",
    "9.38%",
    "5.42%",
    "35.1%",
  ],
  [
    "露营新手季",
    "商品成交",
    "¥744.30",
    "17,115",
    "46",
    "¥16.18",
    "2.78%",
    "10.46%",
    "6.93%",
    "38.9%",
  ],
];
export const nav: [View, string, string][] = [
  ["overview", "▦", "概览"],
  ["campaigns", "◉", "广告活动"],
  ["ads", "▧", "广告素材"],
  ["audiences", "◌", "定向与场景"],
  ["reports", "▥", "报告中心"],
];
export const titles: Record<View, string> = {
  overview: "概览",
  campaigns: "广告活动",
  ads: "广告素材",
  audiences: "定向与场景",
  reports: "报告中心",
};
