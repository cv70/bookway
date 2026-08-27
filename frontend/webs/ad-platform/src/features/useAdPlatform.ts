import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  ActionNodeBinding,
  Campaign,
  Creative,
  DeliveryGuardrails,
  guardrailSeed,
  Scene,
  sceneSeed,
  seed,
  creativeSeed,
} from "../domain";
import { readAsset, useStoredState } from "../lib/storage";
import { AdminApiError, adAdminApi, isAdAdminApiConfigured } from "../lib/adminApi";
import { campaignsFromRemote, updateRemoteCampaign } from "../lib/adminMappers";

// Mirrors the backend targeting normal form: trimmed, lower-cased slugs,
// blanks dropped. The server still revalidates shape and bounds.
const parseTargeting = (value: FormDataEntryValue | null) =>
  String(value ?? "")
    .split(/[,，、\s]+/)
    .map((slug) => slug.trim().toLowerCase())
    .filter(Boolean);

export function useAdPlatform(notify: (message: string) => void) {
  const [campaigns, setCampaigns] = useStoredState<Campaign[]>(
    "ad-campaigns-v5",
    seed,
  );
  const [creatives, setCreatives] = useStoredState<Creative[]>(
    "ad-creatives-v6",
    creativeSeed,
  );
  const [scenes, setScenes] = useStoredState<Scene[]>(
    "ad-scenes-v5",
    sceneSeed,
  );
  // Server guardrails; seeds only until the gateway answers (or when no
  // gateway is configured, where the sandbox value matches migration 0078).
  const [guardrails, setGuardrails] = useState<DeliveryGuardrails>(
    guardrailSeed,
  );
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
        // Keep the seeded default; the authoritative cap stays server-side.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const sceneBindings: ActionNodeBinding[] = scenes.map((scene) => ({
    id: scene.id,
    routeId: scene.id,
    route: scene.name,
    node: scene.node,
    equipment: scene.equipment,
  }));
  const activeBindings = sceneBindings.filter((binding) =>
    scenes.some((scene) => scene.id === binding.id && scene.enabled),
  );
  const filteredCampaigns = useMemo(
    () =>
      campaigns.filter(
        (campaign) =>
          (status === "all" || campaign.state === status) &&
          campaign.name.includes(query),
      ),
    [campaigns, query, status],
  );
  const isBindingActive = (binding: Campaign["binding"]) =>
    activeBindings.some((active) => active.id === binding.id);
  const hasApprovedCreative = (
    campaign: Pick<Campaign, "binding" | "creativeName">,
  ) =>
    creatives.some(
      (creative) =>
        creative.name === campaign.creativeName &&
        creative.status === "已通过" &&
        creative.binding.id === campaign.binding.id,
    );

  const toggleCampaign = (name: string) => {
    const campaign = campaigns.find((item) => item.name === name);
    if (!campaign) return;
    if (
      campaign?.state === "paused" &&
      (!isBindingActive(campaign.binding) || !hasApprovedCreative(campaign))
    ) {
      notify("关联场景未启用或素材未通过审核，不能恢复投放。");
      return;
    }
    const nextState = campaign.state === "running" ? "paused" : "running";
    setCampaigns((current) =>
      current.map((campaign) =>
        campaign.name === name
          ? {
              ...campaign,
              state: nextState,
            }
          : campaign,
      ),
    );
    if (isAdAdminApiConfigured() && campaign) {
      void adAdminApi
        .updateCampaign(
          campaign.id,
          updateRemoteCampaign({ ...campaign, state: nextState }),
        )
        .catch((error: unknown) => {
          const apiError = error instanceof AdminApiError ? error : undefined;
          setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
          setRemoteMessage(apiError?.message || "活动状态同步失败。");
          notify(apiError?.message || "活动状态同步失败。");
        });
    }
  };
  const submitReview = (name: string) => {
    const campaign = campaigns.find((item) => item.name === name);
    if (
      !campaign ||
      !isBindingActive(campaign.binding) ||
      !hasApprovedCreative(campaign)
    ) {
      notify("请先启用关联行动节点场景，并关联已通过的同场景素材，再提交审核。");
      return;
    }
    setCampaigns((current) =>
      current.map((campaign) =>
        campaign.name === name ? { ...campaign, state: "pending" } : campaign,
      ),
    );
    notify("广告活动已提交审核，审核通过后才会参与竞价。");
  };
  const approveCampaign = (name: string) => {
    const campaign = campaigns.find((item) => item.name === name);
    if (
      !campaign ||
      !isBindingActive(campaign.binding) ||
      !hasApprovedCreative(campaign)
    ) {
      notify("关联场景未启用或素材未通过审核，不能通过审核。");
      return;
    }
    setCampaigns((current) =>
      current.map((item) =>
        item.name === name && item.state === "pending"
          ? { ...item, state: "running" }
          : item,
      ),
    );
    notify("活动已审核通过，已在绑定行动节点参与投放。");
  };
  const returnCampaignToDraft = (name: string) => {
    setCampaigns((current) =>
      current.map((item) =>
        item.name === name && item.state === "pending"
          ? { ...item, state: "draft" }
          : item,
      ),
    );
    notify("活动已退回草稿，请完善素材和场景相关性后重新提交。");
  };
  const saveCampaign = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const name = String(data.get("name"));
    const budget = Number(data.get("budget"));
    const bid = Number(data.get("bid"));
    const binding = sceneBindings.find(
      (node) => node.id === data.get("actionNodeId"),
    );
    const creativeName = String(data.get("creativeName") || "");
    const frequencyCap = Number(data.get("frequencyCap"));
    const geoRegions = parseTargeting(data.get("geoRegions"));
    const deviceOs = parseTargeting(data.get("deviceOs"));
    const title = String(data.get("title") || "").trim();
    const body = String(data.get("body") || "").trim();
    const imageUrl = String(data.get("imageUrl") || "").trim();
    const landingUrl = String(data.get("landingUrl") || "").trim();
    const invalidLanding =
      landingUrl.length > 0 && !/^https?:\/\//i.test(landingUrl);
    if (
      !binding ||
      !Number.isFinite(bid) ||
      bid < 0.1 ||
      !Number.isInteger(frequencyCap) ||
      frequencyCap < 1 ||
      frequencyCap > guardrails.userDailyCap ||
      geoRegions.length > 20 ||
      deviceOs.length > 20 ||
      title.length < 4 ||
      title.length > 60 ||
      body.length > 200 ||
      invalidLanding
    ) {
      notify(
        `请补全有效的路线节点、出价、创意标题（4-60 字）与频控（1 到 ${guardrails.userDailyCap} 次）；落地页链接需以 http(s) 开头。`,
      );
      return;
    }
    const creative = creatives.find((item) => item.name === creativeName);
    if (
      !creative ||
      creative.status !== "已通过" ||
      creative.binding.id !== binding.id
    ) {
      notify("请选择同一行动节点且已通过审核的素材。");
      return;
    }
    if (!scenes.some((scene) => scene.id === binding.id && scene.enabled)) {
      notify("该行动节点场景未启用，不能创建或恢复投放活动。");
      return;
    }
    setCampaigns((current) =>
      editing
        ? current.map((campaign) =>
            campaign.name === editing.name
              ? {
                  ...campaign,
                  goal: String(data.get("goal")),
                  budget: `¥${budget.toLocaleString()}`,
                  binding,
                  creativeName,
                  title,
                  body,
                  imageUrl,
                  landingUrl,
                  frequencyCap,
                  bid,
                  geoRegions,
                  deviceOs,
                }
              : campaign,
          )
        : [
            {
              id: `campaign-${crypto.randomUUID()}`,
              name,
              goal: String(data.get("goal")),
              budget: `¥${budget.toLocaleString()}`,
              spent: "¥0.00",
              impressions: "0",
              state: "draft",
              color: "gray",
              binding,
              creativeName,
              title,
              body,
              imageUrl,
              landingUrl,
              frequencyCap,
              bid,
              geoRegions,
              deviceOs,
              predictions: { pctr: 0.03, pcvr: 0.1, pwegu: 0.05 },
            },
            ...current,
          ],
    );
    setDialog(false);
    setEditing(null);
    notify(editing ? `“${name}”已更新。` : `“${name}”已创建，提交审核后即可开始投放。`);
  };
  const addCreative = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const file = data.get("asset");
    if (!(file instanceof File) || !file.size) {
      notify("请选择素材文件后再提交。");
      return;
    }
    if (file.size > 2 * 1024 * 1024) {
      notify("素材文件不能超过 2MB。");
      return;
    }
    const assetData = await readAsset(file);
    const binding = sceneBindings.find(
      (node) => node.id === data.get("actionNodeId"),
    );
    const contextNote = String(data.get("contextNote") || "").trim();
    const prohibitedCopy = ["限时", "最低价", "立即购买", "全网", "抢购"];
    if (
      contextNote.length < 12 ||
      contextNote.length > 140 ||
      prohibitedCopy.some((phrase) => contextNote.includes(phrase))
    ) {
      notify("请填写 12 到 140 字的行动节点说明，且不要使用强促销文案。");
      return;
    }
    if (!binding) {
      notify("请选择有效的路线行动节点与场景装备。");
      return;
    }
    setCreatives((current) => [
      {
        name: String(data.get("name")),
        format: String(data.get("format")),
        binding,
        contextNote,
        status: "审核中",
        updated: "刚刚",
        assetData,
      },
      ...current,
    ]);
    notify(`“${String(data.get("name"))}”已提交审核。`);
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
    creatives,
    scenes,
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
    setCampaigns,
    setCreatives,
    setScenes,
    saveUserDailyCap,
    toggleCampaign,
    submitReview,
    approveCampaign,
    returnCampaignToDraft,
    saveCampaign,
    addCreative,
    remoteStatus,
    remoteMessage,
  };
}
