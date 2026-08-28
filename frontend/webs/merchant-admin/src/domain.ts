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
  // Commission snapshot (basis points, 0-3000) when mounted remotely.
  commissionBps?: number;
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
// 商品、订单、路线挂载与分账台账一律来自服务端网关；控制台内存态从空开始，
// 不做本地种子，也不持久化到 localStorage（业务事实宁可显示空/未连接）。

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
