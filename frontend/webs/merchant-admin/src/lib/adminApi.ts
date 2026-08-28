type ApiEnvelope<T> = { data: T };
type ErrorEnvelope = { error?: { code?: string; message?: string } };

export class AdminApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code = "admin_api_error",
  ) {
    super(message);
  }

  get requiresAuthentication() {
    return this.status === 401 || this.status === 403;
  }
}

export type MallSku = {
  id: string;
  product_id: string;
  title: string;
  price_cents: number;
  currency: string;
  attributes: Record<string, string>;
  saleable: boolean;
};

export type MallProduct = {
  id: string;
  title: string;
  description: string;
  image_url: string;
  status: number;
  skus: MallSku[];
  created_at: string;
  updated_at: string;
};

export type CreateMallProduct = {
  title: string;
  description: string;
  image_url: string;
  status: number;
  skus: Array<{
    title: string;
    price_cents: number;
    currency: string;
    attributes: Record<string, string>;
    saleable: boolean;
  }>;
};

export type UpdateMallProduct = {
  title?: string;
  description?: string;
  image_url?: string;
  status?: number;
  sku_updates?: Array<{
    sku_id: string;
    title?: string;
    price_cents?: number;
    currency?: string;
    attributes?: { values: Record<string, string> };
    saleable?: boolean;
  }>;
};

export type CreateNodeOffer = {
  product_id: string;
  sku_id: string;
  creator_id: string;
  commission_bps: number;
  // Must be one of the scene equipments the action node itself declares; the
  // mall service revalidates this against bbs-link on attach.
  scene_equipment: string;
};

// Public route content as exposed by GET /v1/posts/{id}: only the fields the
// offer mounting flow needs.
export type PublicRouteAction = {
  id: string;
  title: string;
  scene_equipment: string[];
};
export type PublicRouteContent = {
  id: string;
  author_id: string;
  content_type: number;
  status: number;
  route_template?: { actions: PublicRouteAction[] } | null;
};

export type MerchantOrder = {
  id: string;
  status: number;
  total_cents: number;
  items: Array<{ title: string; quantity: number }>;
  created_at: string;
  fulfillment_status: number;
  tracking_number: string;
};

export type AffiliateSettlement = {
  id: string;
  order_id: string;
  creator_id: string;
  amount_cents: number;
  status: number;
  eligible_at: string;
  settled_at?: string;
};

export type InventoryItem = {
  sku_id: string;
  available: number;
  reserved: number;
  updated_at: string;
};

const gatewayUrl = import.meta.env.VITE_GATEWAY_URL?.replace(/\/$/, "");

export const isMerchantAdminApiConfigured = () => Boolean(gatewayUrl);

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (!gatewayUrl) {
    throw new AdminApiError("未配置商家管理网关地址。", 0, "not_configured");
  }
  let response: Response;
  try {
    const headers = new Headers(init?.headers);
    headers.set("content-type", "application/json");
    response = await fetch(`${gatewayUrl}${path}`, {
      credentials: "include",
      ...init,
      headers,
    });
  } catch {
    throw new AdminApiError("无法连接商家管理服务。", 0, "network_error");
  }
  const payload = (await response.json().catch(() => null)) as
    | ApiEnvelope<T>
    | ErrorEnvelope
    | null;
  if (!response.ok || !payload || !("data" in payload)) {
    const error = payload && "error" in payload ? payload.error : undefined;
    throw new AdminApiError(
      error?.message || "商家管理服务请求失败。",
      response.status,
      error?.code || "request_failed",
    );
  }
  return payload.data;
}

export const merchantAdminApi = {
  getPublicContent: (contentId: string) =>
    request<PublicRouteContent>(`/v1/posts/${encodeURIComponent(contentId)}`),
  listProducts: () =>
    request<{ items: MallProduct[] }>("/v1/admin/mall/products?limit=100"),
  createProduct: (product: CreateMallProduct) =>
    request<MallProduct>("/v1/admin/mall/products", {
      method: "POST",
      body: JSON.stringify(product),
    }),
  updateProduct: (productId: string, product: UpdateMallProduct) =>
    request<MallProduct>(`/v1/admin/mall/products/${encodeURIComponent(productId)}`, {
      method: "PATCH",
      body: JSON.stringify(product),
    }),
  attachNodeOffer: (
    routeId: string,
    actionNodeId: string,
    offer: CreateNodeOffer,
    idempotencyKey: string,
  ) =>
    request<{ id: string }>(
      `/v1/admin/routes/${encodeURIComponent(routeId)}/nodes/${encodeURIComponent(actionNodeId)}/offers`,
      {
        method: "POST",
        headers: { "idempotency-key": idempotencyKey },
        body: JSON.stringify(offer),
      },
    ),
  listOrders: (params = "") =>
    request<{ items: MerchantOrder[]; next_cursor?: string }>(
      `/v1/admin/mall/orders?limit=100${params}`,
    ),
  updateFulfillment: (orderId: string, status: number, trackingNumber = "") =>
    request<MerchantOrder>(
      `/v1/admin/mall/orders/${encodeURIComponent(orderId)}/fulfillment`,
      {
        method: "POST",
        body: JSON.stringify({ status, tracking_number: trackingNumber }),
      },
    ),
  listAffiliateSettlements: () =>
    request<{ items: AffiliateSettlement[] }>(
      "/v1/admin/mall/affiliate-settlements?limit=100",
    ),
  // Settles an eligible affiliate share; replays are idempotent on the
  // backend and a non-eligible row comes back as FailedPrecondition.
  settleAffiliate: (settlementId: string) =>
    request<AffiliateSettlement>(
      `/v1/admin/mall/affiliate-settlements/${encodeURIComponent(settlementId)}/settle`,
      { method: "POST" },
    ),
  // Absolute saleable count; the backend rejects reductions below the
  // reserved quantity and returns the authoritative inventory row.
  setSkuStock: (skuId: string, available: number) =>
    request<InventoryItem>(
      `/v1/admin/mall/skus/${encodeURIComponent(skuId)}/stock`,
      {
        method: "POST",
        body: JSON.stringify({ available }),
      },
    ),
};
