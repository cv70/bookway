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
};
