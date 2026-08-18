import { useState } from "react";
import { Campaign, reportRows } from "../domain";
import { downloadCsv } from "../lib/storage";

export function Reports({
  campaigns,
  notify,
}: {
  campaigns: Campaign[];
  notify: (message: string) => void;
}) {
  const [range, setRange] = useState("近 7 天");
  const [metric, setMetric] = useState("全部目标");
  const rows = campaigns.map(
    (campaign) =>
      reportRows.find((row) => row[0] === campaign.name) || [
        campaign.name,
        campaign.goal,
        campaign.spent,
        campaign.impressions,
        "0",
        "--",
        "--",
        "--",
        "--",
        "--",
      ],
  );
  const exportReport = () => {
    downloadCsv(
      `bookway-ad-report-${range}.csv`,
      [
        "广告活动",
        "目标",
        "消耗",
        "已验证展示",
        "转化",
        "每次转化成本",
        "pCTR",
        "pCVR",
        "pWEGU",
        "路线完成度",
      ],
      rows,
    );
    notify(`报告（${range}）已下载。`);
  };
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">数据报告</p>
          <h1>报告中心</h1>
          <p className="muted">
            按已验证展示、点击、转化和 WEGU 归因生成报告。
          </p>
        </div>
        <button className="button primary" onClick={exportReport}>
          ⇩ 导出报告
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <select
            value={range}
            onChange={(event) => setRange(event.target.value)}
          >
            <option>近 7 天</option>
            <option>近 30 天</option>
            <option>本季度</option>
          </select>
          <select
            value={metric}
            onChange={(event) => setMetric(event.target.value)}
          >
            <option>全部目标</option>
            <option>商品成交</option>
            <option>落地页访问</option>
          </select>
          <span className="report-note">
            数据延迟约 15 分钟 · 最后更新 10:45
          </span>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                {[
                  "广告活动",
                  "目标",
                  "消耗",
                  "已验证展示",
                  "转化",
                  "每次转化成本",
                  "pCTR",
                  "pCVR",
                  "pWEGU",
                  "路线完成度",
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows
                .filter((row) => metric === "全部目标" || row[1] === metric)
                .map((row) => (
                  <tr key={row[0]}>
                    {row.map((cell, index) => (
                      <td key={index}>
                        {index === 0 ? <strong>{cell}</strong> : cell}
                      </td>
                    ))}
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="metric-grid report-metrics">
        <article className="metric-card">
          <p>账户 pCTR</p>
          <strong>3.82%</strong>
          <span className="up">↑ 0.4%</span>
          <small>已验证展示</small>
        </article>
        <article className="metric-card">
          <p>账户 pCVR</p>
          <strong>11.36%</strong>
          <span className="up">↑ 1.1%</span>
          <small>有效点击后</small>
        </article>
        <article className="metric-card">
          <p>整体 WEGU</p>
          <strong>6.42%</strong>
          <span className="up">↑ 0.8%</span>
          <small>较上周期</small>
        </article>
        <article className="metric-card">
          <p>路线完成度</p>
          <strong>38.7%</strong>
          <span className="up">↑ 4.1%</span>
          <small>受广告影响用户</small>
        </article>
        <article className="metric-card">
          <p>有效转化</p>
          <strong>376</strong>
          <span className="up">↑ 17.8%</span>
          <small>去重后</small>
        </article>
      </div>
    </section>
  );
}
