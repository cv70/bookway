import { useState } from "react";
import { Campaign, formatBinding } from "../domain";
import { calculateEcpm } from "../lib/auction";

export function Campaigns({
  campaigns,
  query,
  status,
  setQuery,
  setStatus,
  open,
  toggle,
  onEdit,
}: {
  campaigns: Campaign[];
  query: string;
  status: string;
  setQuery: (value: string) => void;
  setStatus: (value: string) => void;
  open: () => void;
  toggle: (name: string, nextStatus?: 1 | 2) => void;
  onEdit: (campaign: Campaign) => void;
}) {
  const [selected, setSelected] = useState<Campaign | null>(null);
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">广告活动管理</p>
          <h1>广告活动</h1>
          <p className="muted">活动会在预算、频控与路线节点约束内参与投放</p>
        </div>
        <button className="button primary" onClick={open}>
          ＋ 创建广告活动
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <label className="search">
            <span>⌕</span>
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索广告活动"
            />
          </label>
          <select
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <option value="all">全部状态</option>
            <option value="running">投放中</option>
            <option value="paused">已暂停</option>
            <option value="draft">草稿</option>
          </select>
        </div>
        <table>
          <thead>
            <tr>
              <th>广告活动</th>
              <th>目标</th>
              <th>日预算</th>
              <th>已消耗</th>
              <th>展示</th>
              <th>eCPM</th>
              <th>状态</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {campaigns.length ? (
              campaigns.map((campaign) => (
              <tr key={campaign.name}>
                <td>
                  <button
                    className="table-link"
                    onClick={() => setSelected(campaign)}
                  >
                    {campaign.name}
                  </button>
                  <small>{formatBinding(campaign.binding)}</small>
                </td>
                <td>{campaign.goal}</td>
                <td>{campaign.budget}</td>
                <td>{campaign.spent}</td>
                <td>{campaign.impressions}</td>
                <td>¥{calculateEcpm(campaign).toFixed(2)}</td>
                <td>
                  <CampaignStatus state={campaign.state} />
                </td>
                <td>
                  <div className="table-actions">
                    <button
                      className="table-action"
                      onClick={() => onEdit(campaign)}
                    >
                      编辑
                    </button>
                    {campaign.state === "draft" ? (
                      <button
                        className="table-action"
                        onClick={() => toggle(campaign.name, 1)}
                      >
                        启用投放
                      </button>
                    ) : (
                      <button
                        className="table-action"
                        onClick={() =>
                          toggle(
                            campaign.name,
                            campaign.state !== "running" ? 1 : 2,
                          )
                        }
                      >
                        {campaign.state === "running" ? "暂停" : "恢复"}
                      </button>
                    )}
                  </div>
                </td>
              </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={8} className="empty-row">
                    {status === "all" && !query
                      ? "暂无广告活动。"
                      : "没有匹配的广告活动。"}
                  </td>
                </tr>
              )}
          </tbody>
        </table>
      </div>
      {selected && (
        <CampaignDetails
          campaign={selected}
          close={() => setSelected(null)}
          edit={() => {
            setSelected(null);
            onEdit(selected);
          }}
        />
      )}
    </section>
  );
}

function CampaignStatus({ state }: { state: Campaign["state"] }) {
  return state === "draft" ? (
    <span className="status draft">草稿</span>
  ) : (
    <span className={`status ${state}`}>
      <i />
      {state === "running" ? "投放中" : "已暂停"}
    </span>
  );
}

function CampaignDetails({
  campaign,
  close,
  edit,
}: {
  campaign: Campaign;
  close: () => void;
  edit: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">广告活动详情</p>
            <h2>{campaign.name}</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <dl className="detail-list">
          <div>
            <dt>投放目标</dt>
            <dd>{campaign.goal}</dd>
          </div>
          <div>
            <dt>绑定路线 / 行动节点</dt>
            <dd>
              {campaign.binding.routeId} / {campaign.binding.actionNodeId}
            </dd>
          </div>
          <div>
            <dt>场景装备</dt>
            <dd>{campaign.binding.equipment}</dd>
          </div>
          <div>
            <dt>创意标题</dt>
            <dd>{campaign.title}</dd>
          </div>
          <div>
            <dt>落地页</dt>
            <dd>{campaign.landingUrl || "未设置"}</dd>
          </div>
          <div>
            <dt>日预算</dt>
            <dd>{campaign.budget}</dd>
          </div>
          <div>
            <dt>累计消耗</dt>
            <dd>{campaign.spent}</dd>
          </div>
          <div>
            <dt>已验证展示</dt>
            <dd>{campaign.impressions}</dd>
          </div>
          <div>
            <dt>频控</dt>
            <dd>单用户每日最多 {campaign.frequencyCap} 次</dd>
          </div>
          <div>
            <dt>地域定向</dt>
            <dd>
              {campaign.geoRegions.length ? campaign.geoRegions.join("、") : "不限"}
            </dd>
          </div>
          <div>
            <dt>设备定向</dt>
            <dd>{campaign.deviceOs.length ? campaign.deviceOs.join("、") : "不限"}</dd>
          </div>
          <div>
            <dt>竞价与预估</dt>
            <dd>
              ¥{campaign.bid.toFixed(2)} / 点击 · pWEGU{" "}
              {(campaign.predictions.pwegu * 100).toFixed(2)}%
            </dd>
          </div>
        </dl>
        <div className="dialog-actions">
          <button className="button primary" onClick={edit}>
            编辑活动
          </button>
          <button className="button secondary" onClick={close}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

export function CampaignDialog({
  campaign,
  close,
  submit,
}: {
  campaign?: Campaign;
  close: () => void;
  submit: (event: React.FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={submit}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">{campaign ? "编辑投放" : "新建投放"}</p>
            <h2>{campaign ? "编辑广告活动" : "创建广告活动"}</h2>
          </div>
          <button type="button" className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <label>
          最高点击出价（元）
          <input
            name="bid"
            required
            type="number"
            min="0.1"
            step="0.01"
            defaultValue={campaign?.bid || 2}
          />
        </label>
        <label>
          广告活动名称
          <input
            name="name"
            required
            disabled={Boolean(campaign)}
            defaultValue={campaign?.name}
            placeholder="例如：秋日徒步装备推广"
          />
        </label>
        <div className="form-grid">
          <label>
            投放目标
            <select name="goal" defaultValue={campaign?.goal}>
              <option>商品成交</option>
              <option>落地页访问</option>
            </select>
          </label>
          <label>
            日预算（元）
            <input
              name="budget"
              required
              type="number"
              min="100"
              defaultValue={campaign ? campaign.budget.replace("¥", "") : "500"}
            />
          </label>
        </div>
        <label>
          公开路线 ID
          <input
            name="routeId"
            required
            disabled={Boolean(campaign)}
            defaultValue={campaign?.binding.routeId}
            placeholder="例如：route-weekend-hike（服务端将校验公开状态）"
          />
        </label>
        <label>
          行动节点 ID
          <input
            name="actionNodeId"
            required
            disabled={Boolean(campaign)}
            defaultValue={campaign?.binding.actionNodeId}
            placeholder="该路线模板中声明的行动节点 ID"
          />
        </label>
        <label>
          场景装备
          <input
            name="equipment"
            required
            defaultValue={campaign?.binding.equipment}
            placeholder="必须与该行动节点声明的场景装备一致"
          />
        </label>
        <label>
          创意标题
          <input
            name="title"
            required
            minLength={4}
            maxLength={60}
            defaultValue={campaign?.title}
            placeholder="在行动节点展示的主标题（4-60 字）"
          />
        </label>
        <label>
          创意正文
          <textarea
            name="body"
            maxLength={200}
            defaultValue={campaign?.body}
            placeholder="说明该内容如何帮助用户完成当前节点（可选，≤200 字）"
          />
        </label>
        <div className="form-grid">
          <label>
            图片 URL（可选）
            <input
              name="imageUrl"
              type="url"
              defaultValue={campaign?.imageUrl}
              placeholder="https://…"
            />
          </label>
          <label>
            落地页链接（可选）
            <input
              name="landingUrl"
              type="url"
              defaultValue={campaign?.landingUrl}
              placeholder="https://…"
            />
          </label>
        </div>
        <label>
          单用户日频控（次）
          <input
            name="frequencyCap"
            type="number"
            min="1"
            max="10"
            required
            defaultValue={campaign?.frequencyCap || 3}
          />
        </label>
        <div className="form-grid">
          <label>
            地域定向（可选）
            <input
              name="geoRegions"
              placeholder="留空不限；多个用逗号分隔，如 cn-bj,cn-sh"
              defaultValue={campaign?.geoRegions.join(",")}
            />
          </label>
          <label>
            设备定向（可选）
            <input
              name="deviceOs"
              placeholder="留空不限；如 ios,android"
              defaultValue={campaign?.deviceOs.join(",")}
            />
          </label>
        </div>
        <div className="frequency">
          <strong>定向说明</strong>
          <p>
            广告只能绑定公开路线的行动节点及其声明的场景装备，服务端在创建、召回与决策时都会校验；
            观察不到地域或设备时，仅投放未做该维度限定的活动。绑定创建后不可修改。
          </p>
        </div>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">
            {campaign ? "保存更改" : "创建活动"}
          </button>
        </div>
      </form>
    </div>
  );
}
