import { useEffect, useMemo, useState } from "react";
import { Campaign } from "../domain";
import { downloadCsv } from "../lib/storage";
import {
  AdminApiError,
  adAdminApi,
  isAdAdminApiConfigured,
} from "../lib/adminApi";

type RangeOption = { label: string; days: number };

const ranges: RangeOption[] = [
  { label: "近 7 天", days: 7 },
  { label: "近 30 天", days: 30 },
  { label: "近 90 天", days: 90 },
];

const isoDaysAgo = (days: number) =>
  new Date(Date.now() - (days - 1) * 24 * 60 * 60 * 1000)
    .toISOString()
    .slice(0, 10);

type CampaignReport = {
  campaignId: string;
  campaignName: string;
  impressions: number;
  clicks: number;
  spentMicros: number;
};

const formatYuan = (micros: number) =>
  `¥${(micros / 1_000_000).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;

export function Reports({
  campaigns,
  notify,
}: {
  campaigns: Campaign[];
  notify: (message: string) => void;
}) {
  const [range, setRange] = useState(ranges[0]);
  const [status, setStatus] = useState<
    "loading" | "ready" | "local" | "error"
  >(
    isAdAdminApiConfigured() ? "loading" : "local",
  );
  const [errorMessage, setErrorMessage] = useState("");
  const [rows, setRows] = useState<CampaignReport[]>([]);

  useEffect(() => {
    if (!isAdAdminApiConfigured()) return;
    let cancelled = false;
    setStatus("loading");
    adAdminApi
      .deliveryReport(isoDaysAgo(range.days), isoDaysAgo(1))
      .then(({ rows: reportRows }) => {
        if (cancelled) return;
        const byCampaign = new Map<string, CampaignReport>();
        for (const row of reportRows) {
          const entry = byCampaign.get(row.campaign_id) || {
            campaignId: row.campaign_id,
            campaignName:
              campaigns.find((item) => item.id === row.campaign_id)?.name ||
              row.campaign_id,
            impressions: 0,
            clicks: 0,
            spentMicros: 0,
          };
          entry.impressions += row.impressions;
          entry.clicks += row.clicks;
          entry.spentMicros += row.spent_micros;
          byCampaign.set(row.campaign_id, entry);
        }
        setRows([...byCampaign.values()]);
        setStatus("ready");
        setErrorMessage("");
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const apiError = error instanceof AdminApiError ? error : undefined;
        setStatus("error");
        setErrorMessage(apiError?.message || "服务端账本暂时不可用。");
      });
    return () => {
      cancelled = true;
    };
  }, [range.days, campaigns]);

  const totals = useMemo(() => {
    const impressions = rows.reduce((sum, row) => sum + row.impressions, 0);
    const clicks = rows.reduce((sum, row) => sum + row.clicks, 0);
    const spentMicros = rows.reduce((sum, row) => sum + row.spentMicros, 0);
    return {
      impressions,
      clicks,
      spentMicros,
      ctr: impressions > 0 ? clicks / impressions : 0,
    };
  }, [rows]);

  const exportReport = () => {
    downloadCsv(
      `bookway-ad-report-${range.days}d.csv`,
      ["广告活动", "展示", "点击", "观察 CTR", "消耗"],
      rows.map((row) => [
        row.campaignName,
        String(row.impressions),
        String(row.clicks),
        row.impressions > 0
          ? `${((row.clicks / row.impressions) * 100).toFixed(2)}%`
          : "--",
        formatYuan(row.spentMicros),
      ]),
    );
    notify(`报告（${range.label}）已下载。`);
  };

  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">数据报告</p>
          <h1>报告中心</h1>
          <p className="muted">
            来自服务端每日投放账本：展示、点击与消耗按活动汇总；转化指标未接入前不做展示。
          </p>
        </div>
        <button
          className="button primary"
          disabled={!rows.length}
          onClick={exportReport}
        >
          ⇩ 导出报告
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <select
            value={range.label}
            onChange={(event) =>
              setRange(
                ranges.find((option) => option.label === event.target.value) ||
                  ranges[0],
              )
            }
          >
            {ranges.map((option) => (
              <option key={option.label}>{option.label}</option>
            ))}
          </select>
          <span className="report-note">
            {status === "loading"
              ? "账本加载中…"
              : status === "local"
                ? "未连接广告管理网关，无法读取服务端账本。"
                : status === "error"
                  ? errorMessage
                  : "统计来自 ad-center 每日账本，按世界时日切分。"}
          </span>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                {["广告活动", "展示", "点击", "观察 CTR", "消耗"].map(
                  (label) => (
                    <th key={label}>{label}</th>
                  ),
                )}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.campaignId}>
                  <td>
                    <strong>{row.campaignName}</strong>
                  </td>
                  <td>{row.impressions.toLocaleString()}</td>
                  <td>{row.clicks.toLocaleString()}</td>
                  <td>
                    {row.impressions > 0
                      ? `${((row.clicks / row.impressions) * 100).toFixed(2)}%`
                      : "--"}
                  </td>
                  <td>{formatYuan(row.spentMicros)}</td>
                </tr>
              ))}
              {!rows.length && status !== "loading" && (
                <tr>
                  <td colSpan={5}>
                    所选区间暂无账本记录。
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
      <div className="metric-grid report-metrics">
        <article className="metric-card">
          <p>合计展示</p>
          <strong>{totals.impressions.toLocaleString()}</strong>
          <small>{range.label}</small>
        </article>
        <article className="metric-card">
          <p>合计点击</p>
          <strong>{totals.clicks.toLocaleString()}</strong>
          <small>{range.label}</small>
        </article>
        <article className="metric-card">
          <p>账户观察 CTR</p>
          <strong>{(totals.ctr * 100).toFixed(2)}%</strong>
          <small>已验证曝光为分母</small>
        </article>
        <article className="metric-card">
          <p>消耗合计</p>
          <strong>{formatYuan(totals.spentMicros)}</strong>
          <small>CPM/CPC 合并入账</small>
        </article>
      </div>
    </section>
  );
}
