import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  ActionNodeBinding,
  Campaign,
  DeliveryGuardrails,
} from "../domain";
import { AdminApiError, adAdminApi, isAdAdminApiConfigured } from "../lib/adminApi";
import {
  campaignFromRemote,
  campaignsFromRemote,
  createRemoteCampaign,
  updateRemoteCampaign,
} from "../lib/adminMappers";

// Honest-guardrail wording: with no gateway there is no server truth to show.
const GATEWAY_DISCONNECTED = "未连接广告管理网关，无法读写服务端广告数据。";

// Mirrors the backend targeting normal form: trimmed, lower-cased slugs,
// blanks dropped. The server still revalidates shape and bounds.
const parseTargeting = (value: FormDataEntryValue | null) =>
  String(value ?? "")
    .split(/[,，、\s]+/)
    .map((slug) => slug.trim().toLowerCase())
    .filter(Boolean);

export function useAdPlatform(notify: (message: string) => void) {
  // 广告活动与投放护栏只来自服务端网关；内存态从空开始，
  // 不落 localStorage，也不在未连接时用本地数据冒充服务端事实。
  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [guardrails, setGuardrails] = useState<DeliveryGuardrails | null>(null);
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [dialog, setDialog] = useState(false);
  const [editing, setEditing] = useState<Campaign | null>(null);
  const [remoteStatus, setRemoteStatus] = useState<
    "local" | "loading" | "ready" | "auth" | "error"
  >(isAdAdminApiConfigured() ? "loading" : "local");
  const [remoteMessage, setRemoteMessage] = useState("");

  useEffect(() => {
    if (!isAdAdminApiConfigured()) return;
    let cancelled = false;
    adAdminApi
      .listCampaigns()
      .then(({ items }) => {
        if (cancelled) return;
        setCampaigns(campaignsFromRemote(items));
        setRemoteStatus("ready");
        setRemoteMessage("");
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const apiError = error instanceof AdminApiError ? error : undefined;
        setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
        setRemoteMessage(
          apiError?.requiresAuthentication
            ? "登录已过期，请重新登录广告后台。"
            : apiError?.message || "远端广告活动暂时不可用。",
        );
      });
    return () => {
      cancelled = true;
    };
  }, [setCampaigns]);

  useEffect(() => {
    if (!isAdAdminApiConfigured()) return;
    let cancelled = false;
    adAdminApi
      .getGuardrails()
      .then(({ user_daily_total_cap }) => {
        if (cancelled) return;
        setGuardrails({ userDailyCap: user_daily_total_cap });
      })
      .catch(() => {
        // Leave the guardrail unread; audiences surfaces the unknown state
        // instead of guessing the server value.
        setGuardrails(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Delivery-lab and dialog options come from the campaigns' own scene
  // bindings — the only bindings that actually exist server-side.
  const activeBindings = useMemo(() => {
    const unique = new Map<string, ActionNodeBinding>();
    for (const campaign of campaigns) {
      const key = `${campaign.binding.routeId}/${campaign.binding.actionNodeId}`;
      if (!unique.has(key)) unique.set(key, campaign.binding);
    }
    return [...unique.values()];
  }, [campaigns]);

  const filteredCampaigns = useMemo(
    () =>
      campaigns.filter(
        (campaign) =>
          (status === "all" || campaign.state === status) &&
          campaign.name.includes(query),
      ),
    [campaigns, query, status],
  );

  const replaceLocalCampaign = (name: string, next: Campaign) => {
    setCampaigns((current) =>
      current.map((campaign) => (campaign.name === name ? next : campaign)),
    );
  };

  // Activates (status ACTIVE) or pauses (status PAUSED) a campaign; without
  // an explicit target the action flips the current serving state. Draft
  // campaigns always activate. Server-first: local state only updates after
  // the gateway confirms, so the row never shows an unconfirmed status.
  const toggleCampaign = async (name: string, nextStatus?: 1 | 2) => {
    const campaign = campaigns.find((item) => item.name === name);
    if (!campaign) return;
    if (!isAdAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    const running = nextStatus ? nextStatus === 1 : campaign.state !== "running";
    try {
      const remote = await adAdminApi.updateCampaign(
        campaign.id,
        updateRemoteCampaign(campaign, running ? 1 : 2),
      );
      replaceLocalCampaign(name, campaignFromRemote(remote));
      setRemoteStatus("ready");
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "活动状态同步失败。");
      notify(apiError?.message || "活动状态同步失败。");
    }
  };

  const saveCampaign = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!isAdAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    const data = new FormData(event.currentTarget);
    const name = String(data.get("name"));
    const budget = Number(data.get("budget"));
    const bid = Number(data.get("bid"));
    const binding: ActionNodeBinding = {
      routeId: String(data.get("routeId") || "").trim(),
      actionNodeId: String(data.get("actionNodeId") || "").trim(),
      equipment: String(data.get("equipment") || "").trim(),
    };
    const frequencyCap = Number(data.get("frequencyCap"));
    const geoRegions = parseTargeting(data.get("geoRegions"));
    const deviceOs = parseTargeting(data.get("deviceOs"));
    const title = String(data.get("title") || "").trim();
    const body = String(data.get("body") || "").trim();
    const imageUrl = String(data.get("imageUrl") || "").trim();
    const landingUrl = String(data.get("landingUrl") || "").trim();
    const invalidLanding =
      landingUrl.length > 0 && !/^https?:\/\//i.test(landingUrl);
    // Without a gateway answer the local bound stays the form default (30);
    // the authoritative cap is enforced server-side either way.
    const capBound = guardrails?.userDailyCap ?? 30;
    if (
      !binding.routeId ||
      !binding.actionNodeId ||
      !binding.equipment ||
      !Number.isFinite(bid) ||
      bid < 0.1 ||
      !Number.isInteger(frequencyCap) ||
      frequencyCap < 1 ||
      frequencyCap > capBound ||
      geoRegions.length > 20 ||
      deviceOs.length > 20 ||
      title.length < 4 ||
      title.length > 60 ||
      body.length > 200 ||
      invalidLanding
    ) {
      notify(
        `请补全公开路线 ID、行动节点 ID、场景装备、出价与频控（1 到 ${capBound} 次${guardrails ? "" : "，服务端护栏未读取到"}）；创意标题需 4-60 字，落地页需以 http(s) 开头。`,
      );
      return;
    }
    const draft: Campaign = {
      ...(editing ?? {
        id: `campaign-${crypto.randomUUID()}`,
        spent: "¥0.00",
        impressions: "0",
        clicks: 0,
        state: "draft",
        color: "gray",
      }),
      name,
      goal: String(data.get("goal")),
      budget: `¥${budget.toLocaleString()}`,
      binding,
      title,
      body,
      imageUrl,
      landingUrl,
      frequencyCap,
      bid,
      geoRegions,
      deviceOs,
      predictions: editing?.predictions ?? { pctr: 0.03, pcvr: 0.1, pwegu: 0.05 },
    };

    try {
      const remote = editing
        ? await adAdminApi.updateCampaign(editing.id, updateRemoteCampaign(draft))
        : await adAdminApi.createCampaign(createRemoteCampaign(draft));
      if (editing) {
        replaceLocalCampaign(name, campaignFromRemote(remote));
      } else {
        // 新建成功后把服务端返回的活动插入列表顶部，无需整页刷新。
        setCampaigns((current) => [campaignFromRemote(remote), ...current]);
      }
      setRemoteStatus("ready");
      setDialog(false);
      setEditing(null);
      notify(
        editing
          ? `“${name}”已更新（服务端已保存）。`
          : `“${name}”已创建为草稿（服务端已保存），启用后参与投放。`,
      );
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "广告活动保存失败，请稍后重试。");
      notify(apiError?.message || "广告活动保存失败，请检查路线节点绑定后重试。");
    }
  };

  const saveUserDailyCap = async (cap: number) => {
    try {
      const next = await adAdminApi.setUserDailyTotalCap(cap);
      setGuardrails({ userDailyCap: next.user_daily_total_cap });
      notify(
        `单用户全局日曝光上限已更新为 ${next.user_daily_total_cap} 次（服务端立即生效）。`,
      );
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      notify(
        apiError?.message || "护栏保存失败：该上限仅平台管理员可修改。",
      );
    }
  };

  return {
    campaigns,
    guardrails,
    query,
    status,
    dialog,
    editing,
    filteredCampaigns,
    activeBindings,
    setQuery,
    setStatus,
    setDialog,
    setEditing,
    saveUserDailyCap,
    toggleCampaign,
    saveCampaign,
    remoteStatus,
    remoteMessage,
  };
}
