import { FormEvent, useState } from "react";
import {
  ActionNodeBinding,
  Campaign,
  Creative,
  formatBinding,
} from "../domain";
import { calculateEcpm } from "../lib/auction";

export function Campaigns({
  campaigns,
  query,
  status,
  setQuery,
  setStatus,
  open,
  toggle,
  review,
  approve,
  returnToDraft,
  onEdit,
  notify,
}: {
  campaigns: Campaign[];
  query: string;
  status: string;
  setQuery: (value: string) => void;
  setStatus: (value: string) => void;
  open: () => void;
  toggle: (name: string) => void;
  review: (name: string) => void;
  approve: (name: string) => void;
  returnToDraft: (name: string) => void;
  onEdit: (campaign: Campaign) => void;
  notify: (message: string) => void;
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
            <option value="pending">审核中</option>
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
            {campaigns.map((campaign) => (
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
                        onClick={() => review(campaign.name)}
                      >
                        提交审核
                      </button>
                    ) : campaign.state === "pending" ? (
                      <>
                        <button
                          className="table-action"
                          onClick={() => approve(campaign.name)}
                        >
                          通过审核
                        </button>
                        <button
                          className="table-action danger-action"
                          onClick={() => returnToDraft(campaign.name)}
                        >
                          退回草稿
                        </button>
                      </>
                    ) : (
                      <button
                        className="table-action"
                        onClick={() => {
                          toggle(campaign.name);
                          notify(
                            campaign.state === "running"
                              ? "广告活动已暂停。"
                              : "广告活动已恢复投放。",
                          );
                        }}
                      >
                        {campaign.state === "running" ? "暂停" : "恢复"}
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
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
  ) : state === "pending" ? (
    <span className="status pending">
      <i />
      审核中
    </span>
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
            <dt>绑定行动节点</dt>
            <dd>{campaign.binding.node}</dd>
          </div>
          <div>
            <dt>场景装备</dt>
            <dd>{campaign.binding.equipment}</dd>
          </div>
          <div>
            <dt>投放素材</dt>
            <dd>{campaign.creativeName}</dd>
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
  creatives,
  bindings,
  close,
  submit,
}: {
  campaign?: Campaign;
  creatives: Creative[];
  bindings: ActionNodeBinding[];
  close: () => void;
  submit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  const [actionNodeId, setActionNodeId] = useState(
    campaign?.binding.id || bindings[0]?.id || "",
  );
  const matchingCreatives = creatives.filter(
    (creative) =>
      creative.status === "已通过" && creative.binding.id === actionNodeId,
  );
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
          路线行动节点与场景装备
          <select
            name="actionNodeId"
            value={actionNodeId}
            onChange={(event) => setActionNodeId(event.target.value)}
            required
          >
            {bindings.map((binding) => (
              <option value={binding.id} key={binding.id}>
                {formatBinding(binding)}
              </option>
            ))}
          </select>
        </label>
        <label>
          已审核投放素材
          <select
            key={actionNodeId}
            name="creativeName"
            defaultValue={
              matchingCreatives.some(
                (creative) => creative.name === campaign?.creativeName,
              )
                ? campaign?.creativeName
                : matchingCreatives[0]?.name
            }
            required
            disabled={!matchingCreatives.length}
          >
            {matchingCreatives.length ? (
              matchingCreatives.map((creative) => (
                <option value={creative.name} key={creative.name}>
                  {creative.name} · {formatBinding(creative.binding)}
                </option>
              ))
            ) : (
              <option value="">该节点暂无已通过审核的素材</option>
            )}
          </select>
        </label>
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
        <div className="frequency">
          <strong>频控保护</strong>
          <p>系统将在投放时自动执行活动与用户日频控。</p>
        </div>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button
            className="button primary"
            disabled={!matchingCreatives.length}
          >
            {campaign ? "保存更改" : "创建活动"}
          </button>
        </div>
      </form>
    </div>
  );
}
