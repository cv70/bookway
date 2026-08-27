export type View =
  | "overview"
  | "products"
  | "orders"
  | "inventory"
  | "offers"
  | "finance";
export type Product = {
  id: string;
  name: string;
  spu: string;
  description: string;
  sku: string;
  skuId: string;
  variant: string;
  kind: "装备" | "课程";
  warehouse: "北京中心仓" | "数字内容库";
  price: string;
  stock: number;
  sales: number;
  status: "销售中" | "待发布";
  image: string;
};
export const productSpu = (product: Product) => product.spu;
export const productSkuLabel = (product: Product) =>
  `${product.sku} · ${product.variant}`;
export type Order = {
  id: string;
  date: string;
  product: string;
  recipient: string;
  city: string;
  amount: string;
  status: "待发货" | "运输中" | "已完成";
  carrier?: string;
  tracking?: string;
};
export type RouteOffer = {
  id: string;
  productId: string;
  routeId: string;
  actionNodeId: string;
  route: string;
  node: string;
  equipment: string;
  product: string;
  sku: string;
  clicks: number;
  orders: number;
  wegu: number;
  routeCompletion: number;
  enabled: boolean;
};
// Mirrors the mall-order affiliate settlement ledger: one row per order,
// shaped by the AffiliateSettlement proto (id/order/creator/amount/status).
export type AffiliateSettlement = {
  id: string;
  order_id: string;
  creator: string;
  payable: string;
  status: "待生效" | "待结算" | "已结算" | "已冲正";
  date: string;
};
export type AffiliateRule = {
  id: string;
  creator: string;
  route: string;
  node: string;
  equipment: string;
  rate: number;
  enabled: boolean;
};
export type MerchantActionNode = {
  id: string;
  routeId: string;
  route: string;
  node: string;
  equipment: string;
};
export const actionNodes: MerchantActionNode[] = [
  {
    id: "hike-gear",
    routeId: "route-weekend-hike",
    route: "周末轻徒步入门",
    node: "装备准备",
    equipment: "轻量背包与防磨徒步袜",
  },
  {
    id: "bike-check",
    routeId: "route-city-bike",
    route: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
  },
  {
    id: "camp-light",
    routeId: "route-summer-camp",
    route: "夏日露营清单",
    node: "夜间照明",
    equipment: "营地灯与备用电源",
  },
];
export const formatActionNode = (node: MerchantActionNode) =>
  `${node.route} · ${node.node} · ${node.equipment}`;
export type StockAdjustment = {
  id: string;
  sku: string;
  product: string;
  warehouse: string;
  quantity: number;
  reason: string;
  createdAt: string;
};

export const productImages = [
  "https://images.unsplash.com/photo-1553062407-98eeb64c6a62?auto=format&fit=crop&w=96&q=80",
  "https://images.unsplash.com/photo-1521334884684-d80222895322?auto=format&fit=crop&w=96&q=80",
  "https://images.unsplash.com/photo-1544967082-d9d25d867d66?auto=format&fit=crop&w=96&q=80",
  "https://images.unsplash.com/photo-1517825738774-7de9363ef735?auto=format&fit=crop&w=96&q=80",
];
export const initialProducts: Product[] = [
  {
    id: "product-tr20",
    name: "轻量徒步背包 20L",
    spu: "SPU-TR20",
    description: "适合周末轻徒步的轻量收纳背包。",
    sku: "SKU-TR20-G",
    skuId: "sku-tr20-g",
    variant: "岩灰色",
    kind: "装备",
    warehouse: "北京中心仓",
    price: "¥329.00",
    stock: 8,
    sales: 246,
    status: "销售中",
    image: productImages[0],
  },
  {
    id: "product-sk03",
    name: "快干徒步袜 3 双装",
    spu: "SPU-SK03",
    description: "适合骑行与徒步的排汗防磨袜。",
    sku: "SKU-SK03-D",
    skuId: "sku-sk03-d",
    variant: "深灰色",
    kind: "装备",
    warehouse: "北京中心仓",
    price: "¥78.00",
    stock: 12,
    sales: 418,
    status: "销售中",
    image: productImages[1],
  },
  {
    id: "product-lg01",
    name: "折叠露营灯",
    spu: "SPU-LG01",
    description: "夜间露营节点适用的暖光照明装备。",
    sku: "SKU-LG01-W",
    skuId: "sku-lg01-w",
    variant: "暖白光",
    kind: "装备",
    warehouse: "北京中心仓",
    price: "¥89.00",
    stock: 6,
    sales: 167,
    status: "待发布",
    image: productImages[2],
  },
  {
    id: "product-bt01",
    name: "山野保温杯",
    spu: "SPU-BT01",
    description: "长距离步行场景使用的保温水杯。",
    sku: "SKU-BT01-G",
    skuId: "sku-bt01-g",
    variant: "森林绿",
    kind: "装备",
    warehouse: "北京中心仓",
    price: "¥119.00",
    stock: 64,
    sales: 0,
    status: "待发布",
    image: productImages[3],
  },
  {
    id: "product-cr01",
    name: "城市骑行安全入门课程",
    spu: "SPU-CR01",
    description: "面向首次城市骑行的出发检查与风险规避课程。",
    sku: "SKU-CR01-ON",
    skuId: "sku-cr01-on",
    variant: "在线 90 分钟",
    kind: "课程",
    warehouse: "数字内容库",
    price: "¥69.00",
    stock: 999,
    sales: 93,
    status: "销售中",
    image: productImages[3],
  },
];
export const initialOrders: Order[] = [
  {
    id: "BW202608180128",
    date: "今天 10:42",
    product: "轻量徒步背包 20L × 1",
    recipient: "张晓",
    city: "北京朝阳",
    amount: "¥329.00",
    status: "待发货",
  },
  {
    id: "BW202608180125",
    date: "今天 10:18",
    product: "快干徒步袜 3 双装 × 2",
    recipient: "陈舟",
    city: "上海徐汇",
    amount: "¥156.00",
    status: "待发货",
  },
  {
    id: "BW202608170915",
    date: "昨天 17:18",
    product: "折叠露营灯 × 1",
    recipient: "吴悦",
    city: "杭州西湖",
    amount: "¥89.00",
    status: "运输中",
  },
  {
    id: "BW202608160627",
    date: "8 月 16 日",
    product: "山野保温杯 × 1",
    recipient: "陆青",
    city: "南京鼓楼",
    amount: "¥119.00",
    status: "已完成",
  },
];
export const initialOffers: RouteOffer[] = [
  {
    id: "offer-1",
    productId: "product-tr20",
    routeId: "route-weekend-hike",
    actionNodeId: "hike-gear",
    route: "周末轻徒步入门",
    node: "装备准备",
    equipment: "轻量背包与防磨徒步袜",
    product: "轻量徒步背包 20L",
    sku: "SKU-TR20-G",
    clicks: 1840,
    orders: 12,
    wegu: 7.2,
    routeCompletion: 46.8,
    enabled: true,
  },
  {
    id: "offer-2",
    productId: "product-sk03",
    routeId: "route-city-bike",
    actionNodeId: "bike-check",
    route: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
    product: "快干徒步袜 3 双装",
    sku: "SKU-SK03-D",
    clicks: 1260,
    orders: 8,
    wegu: 5.4,
    routeCompletion: 41.2,
    enabled: true,
  },
  {
    id: "offer-3",
    productId: "product-lg01",
    routeId: "route-summer-camp",
    actionNodeId: "camp-light",
    route: "夏日露营清单",
    node: "夜间照明",
    equipment: "营地灯与备用电源",
    product: "折叠露营灯",
    sku: "SKU-LG01-W",
    clicks: 730,
    orders: 5,
    wegu: 6.1,
    routeCompletion: 38.7,
    enabled: false,
  },
  {
    id: "offer-4",
    productId: "product-cr01",
    routeId: "route-city-bike",
    actionNodeId: "bike-check",
    route: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
    product: "城市骑行安全入门课程",
    sku: "SKU-CR01-ON",
    clicks: 680,
    orders: 9,
    wegu: 5.8,
    routeCompletion: 43.5,
    enabled: true,
  },
];
// Sandbox seeds for the local (gateway-unconfigured) mode only. Remote mode
// replaces these with the mall-order affiliate ledger via affiliateFromMall.
export const affiliateSettlements: AffiliateSettlement[] = [
  {
    id: "AF20260801",
    order_id: "order-8f2a",
    creator: "creator-1",
    payable: "¥438.00",
    status: "待结算",
    date: "2026-08-20T09:30:00Z",
  },
  {
    id: "AF20260802",
    order_id: "order-91cc",
    creator: "creator-2",
    payable: "¥216.00",
    status: "待结算",
    date: "2026-08-21T14:05:00Z",
  },
  {
    id: "AF20260711",
    order_id: "order-77be",
    creator: "creator-3",
    payable: "¥172.50",
    status: "已结算",
    date: "2026-08-01T10:00:00Z",
  },
];

export const initialAffiliateRules: AffiliateRule[] = [
  {
    id: "affiliate-rule-1",
    creator: "山野阿柠",
    route: "周末轻徒步入门",
    node: "装备准备",
    equipment: "轻量背包与防磨徒步袜",
    rate: 12,
    enabled: true,
  },
  {
    id: "affiliate-rule-2",
    creator: "骑行小陈",
    route: "城市骑行第一课",
    node: "出发前检查",
    equipment: "头盔、手套与骑行补给",
    rate: 10,
    enabled: true,
  },
];
export const nav: { view: View; label: string; icon: string }[] = [
  { view: "overview", label: "经营总览", icon: "▦" },
  { view: "products", label: "商品管理", icon: "◇" },
  { view: "orders", label: "订单中心", icon: "▤" },
  { view: "inventory", label: "库存管理", icon: "▤" },
  { view: "offers", label: "路线商品", icon: "↗" },
  { view: "finance", label: "财务结算", icon: "¥" },
];
export const titles: Record<View, string> = {
  overview: "经营总览",
  products: "商品管理",
  orders: "订单中心",
  inventory: "库存管理",
  offers: "路线商品",
  finance: "财务结算",
};
