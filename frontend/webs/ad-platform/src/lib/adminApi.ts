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

export type AdCampaign = {
  id: string;
  advertiser_id: string;
  name: string;
  placement: string;
  route_id: string;
  action_node_id: string;
  scene_equipment: string;
  title: string;
  body: string;
  image_url: string;
  landing_url: string;
  target_domains: string[];
  geo_regions: string[];
  device_os: string[];
  status: number;
  pricing_model: number;
  bid_micros: number;
  daily_budget_micros: number;
  spent_today_micros: number;
  frequency_cap: number;
  predicted_ctr: number;
  predicted_cvr: number;
  global_frequency_cap: number;
  impressions: number;
  clicks: number;
};

export type CreateAdCampaign = Omit<AdCampaign, "id" | "advertiser_id" | "status" | "spent_today_micros" | "impressions" | "clicks">;
export type UpdateAdCampaign = Partial<
  Pick<
    AdCampaign,
    | "name"
    | "title"
    | "body"
    | "image_url"
    | "landing_url"
    | "target_domains"
    | "geo_regions"
    | "device_os"
    | "status"
    | "bid_micros"
    | "daily_budget_micros"
    | "frequency_cap"
    | "predicted_ctr"
    | "predicted_cvr"
    | "global_frequency_cap"
  >
>;

// Platform guardrails that actually exist server-side; writes are limited to
// platform admins by the gateway.
export type AdGuardrails = { user_daily_total_cap: number };

// Daily ledger rows from ad_campaign_daily_stats.
export type AdDeliveryReportRow = {
  campaign_id: string;
  stat_date: string;
  impressions: number;
  clicks: number;
  spent_micros: number;
};
export type AdDeliveryReport = { rows: AdDeliveryReportRow[] };

const gatewayUrl = import.meta.env.VITE_GATEWAY_URL?.replace(/\/$/, "");

export const isAdAdminApiConfigured = () => Boolean(gatewayUrl);

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (!gatewayUrl) {
    throw new AdminApiError("未配置广告管理网关地址。", 0, "not_configured");
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
    throw new AdminApiError("无法连接广告管理服务。", 0, "network_error");
  }
  const payload = (await response.json().catch(() => null)) as
    | ApiEnvelope<T>
    | ErrorEnvelope
    | null;
  if (!response.ok || !payload || !("data" in payload)) {
    const error = payload && "error" in payload ? payload.error : undefined;
    throw new AdminApiError(
      error?.message || "广告管理服务请求失败。",
      response.status,
      error?.code || "request_failed",
    );
  }
  return payload.data;
}

export const adAdminApi = {
  listCampaigns: () =>
    request<{ items: AdCampaign[] }>("/v1/admin/ads/campaigns?limit=100"),
  getCampaign: (campaignId: string) =>
    request<AdCampaign>(`/v1/admin/ads/campaigns/${encodeURIComponent(campaignId)}`),
  createCampaign: (campaign: CreateAdCampaign) =>
    request<AdCampaign>("/v1/admin/ads/campaigns", {
      method: "POST",
      body: JSON.stringify(campaign),
    }),
  updateCampaign: (campaignId: string, campaign: UpdateAdCampaign) =>
    request<AdCampaign>(`/v1/admin/ads/campaigns/${encodeURIComponent(campaignId)}`, {
      method: "PATCH",
      body: JSON.stringify(campaign),
    }),
  getGuardrails: () => request<AdGuardrails>("/v1/admin/ads/guardrails"),
  setUserDailyTotalCap: (cap: number) =>
    request<AdGuardrails>("/v1/admin/ads/guardrails", {
      method: "PATCH",
      body: JSON.stringify({ user_daily_total_cap: cap }),
    }),
  deliveryReport: (fromDate: string, toDate: string) =>
    request<AdDeliveryReport>(
      `/v1/admin/ads/reports?from_date=${encodeURIComponent(fromDate)}&to_date=${encodeURIComponent(toDate)}`,
    ),
};
