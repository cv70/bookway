import { ActionNodeBinding, Campaign } from "../domain";
import { AdCampaign, CreateAdCampaign, UpdateAdCampaign } from "./adminApi";

const bindingFor = (remote: AdCampaign): ActionNodeBinding => ({
  id: remote.action_node_id,
  routeId: remote.route_id,
  route: remote.route_id,
  node: remote.action_node_id,
  equipment: remote.scene_equipment,
});

const stateFor = (status: number): Campaign["state"] =>
  status === 1 ? "running" : status === 2 ? "paused" : "draft";

export function campaignFromRemote(remote: AdCampaign): Campaign {
  return {
    id: remote.id,
    name: remote.name,
    goal: remote.pricing_model === 1 ? "落地页访问" : "商品成交",
    budget: `¥${(remote.daily_budget_micros / 1_000_000).toFixed(2)}`,
    spent: `¥${(remote.spent_today_micros / 1_000_000).toFixed(2)}`,
    impressions: remote.impressions.toLocaleString(),
    state: stateFor(remote.status),
    color: "gray",
    binding: bindingFor(remote),
    creativeName: remote.title,
    title: remote.title,
    body: remote.body,
    imageUrl: remote.image_url,
    landingUrl: remote.landing_url,
    frequencyCap: remote.frequency_cap,
    bid: remote.bid_micros / 1_000_000,
    geoRegions: remote.geo_regions,
    deviceOs: remote.device_os,
    predictions: {
      pctr: remote.predicted_ctr,
      pcvr: remote.predicted_cvr,
      pwegu: remote.predicted_ctr * remote.predicted_cvr,
    },
  };
}

export const campaignsFromRemote = (items: AdCampaign[]) =>
  items.map(campaignFromRemote);

export function createRemoteCampaign(campaign: Campaign): CreateAdCampaign {
  return {
    name: campaign.name,
    placement: "action_node",
    route_id: campaign.binding.routeId,
    action_node_id: campaign.binding.id,
    scene_equipment: campaign.binding.equipment,
    title: campaign.title,
    body: campaign.body,
    image_url: campaign.imageUrl,
    landing_url: campaign.landingUrl,
    target_domains: [],
    geo_regions: campaign.geoRegions,
    device_os: campaign.deviceOs,
    pricing_model: campaign.goal === "落地页访问" ? 1 : 0,
    bid_micros: Math.round(campaign.bid * 1_000_000),
    daily_budget_micros: Math.round(
      Number(campaign.budget.replace(/[¥,]/g, "")) * 1_000_000,
    ),
    frequency_cap: campaign.frequencyCap,
    predicted_ctr: campaign.predictions.pctr,
    predicted_cvr: campaign.predictions.pcvr,
    global_frequency_cap: campaign.frequencyCap,
  };
}

export function updateRemoteCampaign(campaign: Campaign): UpdateAdCampaign {
  return {
    name: campaign.name,
    title: campaign.title,
    body: campaign.body,
    image_url: campaign.imageUrl,
    landing_url: campaign.landingUrl,
    status:
      campaign.state === "running" ? 1 : campaign.state === "paused" ? 2 : 0,
    bid_micros: Math.round(campaign.bid * 1_000_000),
    daily_budget_micros: Math.round(
      Number(campaign.budget.replace(/[¥,]/g, "")) * 1_000_000,
    ),
    frequency_cap: campaign.frequencyCap,
    predicted_ctr: campaign.predictions.pctr,
    predicted_cvr: campaign.predictions.pcvr,
    global_frequency_cap: campaign.frequencyCap,
    geo_regions: campaign.geoRegions,
    device_os: campaign.deviceOs,
  };
}
