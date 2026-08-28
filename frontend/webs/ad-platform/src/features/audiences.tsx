import { useState } from "react";
import {
  ActionNodeBinding,
  Campaign,
  DeliveryGuardrails,
} from "../domain";
import { DeliveryLab } from "./delivery-lab";

export function Audiences({
  campaigns,
  bindings,
  guardrails,
  onSaveCap,
}: {
  campaigns: Campaign[];
  bindings: ActionNodeBinding[];
  guardrails: DeliveryGuardrails | null;
  onSaveCap: (cap: number) => void;
}) {
  const [policy, setPolicy] = useState(false);
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">场景化投放</p>
          <h1>定向与护栏</h1>
          <p className="muted">
            广告只出现在其绑定的公开路线行动节点上；定向维度在活动上配置，此处查看护栏与演练。
          </p>
        </div>
      </div>
      <div className="guardrail">
        <span>✓</span>
        <p>
          <strong>隐私护栏已启用</strong>
          定向条件仅来自路线节点、行动进度和公开兴趣标签，不使用健康、精确位置或敏感个人信息。
        </p>
      </div>
      <div className="panel table-panel">
        <div className="panel-header">
          <div>
            <h2>当前活动的场景绑定</h2>
            <p>
              绑定关系由广告活动持有，服务端在创建、召回与决策时校验路线公开状态与节点装备声明。
              新增或修改绑定请编辑对应广告活动。
            </p>
          </div>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>公开路线 ID</th>
                <th>行动节点 ID</th>
                <th>场景装备</th>
                <th>关联活动</th>
              </tr>
            </thead>
            <tbody>
              {bindings.map((binding) => {
                const related = campaigns
                  .filter(
                    (campaign) =>
                      campaign.binding.routeId === binding.routeId &&
                      campaign.binding.actionNodeId === binding.actionNodeId,
                  )
                  .map((campaign) => campaign.name);
                return (
                  <tr
                    key={`${binding.routeId}/${binding.actionNodeId}`}
                  >
                    <td>
                      <strong>{binding.routeId}</strong>
                    </td>
                    <td>{binding.actionNodeId}</td>
                    <td>{binding.equipment}</td>
                    <td>{related.join("、") || "—"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
      <div className="lower-grid">
        <Policy
          title="频控与护栏"
          icon="⌁"
          text={
            guardrails
              ? `单用户每日最多展示 ${guardrails.userDailyCap} 次（跨所有活动，服务端强制）。单活动频控在各活动的投放设置中配置。`
              : "单用户每日曝光上限由服务端在每次曝光受理时强制执行；当前未读取到配置（未连接网关或权限不足）。"
          }
          disabled={!guardrails}
          onClick={() => setPolicy(true)}
        />
      </div>
      {policy && guardrails && (
        <CapDialog
          cap={guardrails.userDailyCap}
          onSave={onSaveCap}
          close={() => setPolicy(false)}
        />
      )}
      {guardrails ? (
        <DeliveryLab
          campaigns={campaigns}
          bindings={bindings}
          guardrails={guardrails}
        />
      ) : (
        <p className="panel-note">
          未连接广告管理网关或护栏未读取，投放演练暂不可用。
        </p>
      )}
    </section>
  );
}
function Policy({
  title,
  icon,
  text,
  disabled,
  onClick,
}: {
  title: string;
  icon: string;
  text: string;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <article className="panel scene-policy">
      <div>
        <span>{icon}</span>
        <h2>{title}</h2>
        <p>{text}</p>
      </div>
      <button
        className="button secondary"
        disabled={disabled}
        title={disabled ? "未读取到服务端护栏，无法编辑" : undefined}
        onClick={onClick}
      >
        查看规则
      </button>
    </article>
  );
}
function CapDialog({
  cap,
  onSave,
  close,
}: {
  cap: number;
  onSave: (cap: number) => void;
  close: () => void;
}) {
  const [error, setError] = useState("");
  const save = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = Number(new FormData(event.currentTarget).get("userDailyCap"));
    if (!Number.isInteger(value) || value < 1 || value > 30) {
      setError("请输入 1 到 30 的整数。");
      return;
    }
    onSave(value);
    close();
  };
  return (
    <div className="modal-backdrop" onClick={close}>
      <form
        className="modal"
        onClick={(event) => event.stopPropagation()}
        onSubmit={save}
      >
        <div className="dialog-header">
          <div>
            <p className="eyebrow">投放护栏</p>
            <h2>单用户全局日曝光上限</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <label>
          每位用户每日跨活动曝光上限（次）
          <input
            name="userDailyCap"
            type="number"
            min="1"
            max="30"
            required
            defaultValue={cap}
          />
        </label>
        <div className="frequency">
          <strong>生效范围</strong>
          <p>
            该上限由服务端在每次曝光受理时强制执行，删除配置也不会静默关闭；
            仅平台管理员可以修改。
          </p>
        </div>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">保存护栏</button>
        </div>
      </form>
    </div>
  );
}
