import { FormEvent, useState } from "react";
import { Campaign, DeliveryGuardrails, Scene } from "../domain";
import { DeliveryLab } from "./delivery-lab";

export function Audiences({
  scenes,
  campaigns,
  guardrails,
  toggle,
  onAdd,
  onRemove,
  onSaveGuardrails,
  notify,
}: {
  scenes: Scene[];
  campaigns: Campaign[];
  guardrails: DeliveryGuardrails;
  toggle: (id: string) => void;
  onAdd: (scene: Scene) => void;
  onRemove: (id: string) => void;
  onSaveGuardrails: (guardrails: DeliveryGuardrails) => void;
  notify: (message: string) => void;
}) {
  const [policy, setPolicy] = useState<"frequency" | "diversity" | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [removing, setRemoving] = useState<Scene | null>(null);
  const [error, setError] = useState("");
  const add = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const name = String(data.get("route"));
    const node = String(data.get("node"));
    const equipment = String(data.get("equipment"));
    if (scenes.some((scene) => scene.name === name && scene.node === node)) {
      setError("该路线行动节点已经存在投放策略。");
      return;
    }
    onAdd({
      id: `scene-${Date.now()}`,
      name,
      node,
      equipment,
      audience: String(data.get("audience")),
      reach: `${Number(data.get("reach")).toLocaleString()} 人`,
      enabled: false,
    });
    setShowForm(false);
    setError("");
    notify("新场景已创建，默认停用，审核后可启用。");
  };
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">场景化投放</p>
          <h1>定向与场景</h1>
          <p className="muted">
            只面向用户主动打开的公开路线节点，不使用健康、精确位置或敏感个人信息。
          </p>
        </div>
        <div className="heading-actions">
          <button
            className="button primary"
            onClick={() => notify("场景策略已保存并提交审核。")}
          >
            保存策略
          </button>
          <button
            className="button secondary"
            onClick={() => setShowForm(true)}
          >
            ＋ 新增场景
          </button>
        </div>
      </div>
      <div className="guardrail">
        <span>✓</span>
        <p>
          <strong>隐私护栏已启用</strong>
          定向条件仅来自路线节点、行动进度和公开兴趣标签。
        </p>
      </div>
      <div className="panel table-panel">
        <div className="panel-header">
          <div>
            <h2>路线节点投放范围</h2>
            <p>启用后，绑定活动可在该节点参与竞价</p>
          </div>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>路线</th>
                <th>行动节点</th>
                <th>场景装备</th>
                <th>允许的人群信号</th>
                <th>近 7 日触达</th>
                <th>投放开关</th>
              </tr>
            </thead>
            <tbody>
              {scenes.map((scene) => (
                <tr key={scene.id}>
                  <td>
                    <strong>{scene.name}</strong>
                  </td>
                  <td>{scene.node}</td>
                  <td>{scene.equipment}</td>
                  <td>{scene.audience}</td>
                  <td>{scene.reach}</td>
                  <td>
                    <div className="table-actions">
                      <button
                        className={`toggle ${scene.enabled ? "on" : ""}`}
                        aria-pressed={scene.enabled}
                        onClick={() => {
                          toggle(scene.id);
                          notify(
                            `${scene.name} 已${scene.enabled ? "暂停" : "启用"}投放。`,
                          );
                        }}
                      >
                        <i />
                        {scene.enabled ? "已启用" : "已停用"}
                      </button>
                      <button
                        className="table-action danger-action"
                        onClick={() => setRemoving(scene)}
                      >
                        移除
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      {showForm && (
        <SceneForm
          error={error}
          close={() => setShowForm(false)}
          submit={add}
        />
      )}
      {removing && (
        <RemoveScene
          scene={removing}
          close={() => setRemoving(null)}
          remove={() => {
            onRemove(removing.id);
            setRemoving(null);
            notify("场景策略已移除。");
          }}
        />
      )}
      <div className="lower-grid">
        <Policy
          title="频控与打散"
          icon="⌁"
          text={`单用户每日最多展示 ${guardrails.userDailyCap} 次，同一活动连续展示间隔 ${guardrails.cooldownMinutes} 分钟。`}
          onClick={() => setPolicy("frequency")}
        />
        <Policy
          title="防茧房策略"
          icon="◌"
          text={`广告混排保留 ${guardrails.explorationShare}% 探索流量，连续同路线最多 ${guardrails.sameRouteLimit} 次。`}
          onClick={() => setPolicy("diversity")}
        />
      </div>
      {policy && (
        <PolicyDialog
          policy={policy}
          guardrails={guardrails}
          onSave={onSaveGuardrails}
          close={() => setPolicy(null)}
        />
      )}
      <DeliveryLab
        campaigns={campaigns}
        guardrails={guardrails}
        scenes={scenes}
      />
    </section>
  );
}
function Policy({
  title,
  icon,
  text,
  onClick,
}: {
  title: string;
  icon: string;
  text: string;
  onClick: () => void;
}) {
  return (
    <article className="panel scene-policy">
      <div>
        <span>{icon}</span>
        <h2>{title}</h2>
        <p>{text}</p>
      </div>
      <button className="button secondary" onClick={onClick}>
        查看规则
      </button>
    </article>
  );
}
function SceneForm({
  error,
  close,
  submit,
}: {
  error: string;
  close: () => void;
  submit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <form
        className="modal"
        onSubmit={submit}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialog-header">
          <div>
            <p className="eyebrow">场景化投放</p>
            <h2>新增路线场景</h2>
          </div>
          <button type="button" className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <label>
          路线
          <select name="route">
            <option>周末轻徒步入门</option>
            <option>城市骑行第一课</option>
            <option>夏日露营清单</option>
            <option>晨间公园慢跑</option>
          </select>
        </label>
        <label>
          行动节点
          <select name="node">
            <option>装备准备</option>
            <option>出发前检查</option>
            <option>夜间照明</option>
            <option>热身与拉伸</option>
          </select>
        </label>
        <label>
          场景装备
          <input
            name="equipment"
            required
            placeholder="例如：防晒衣与补水装备"
          />
        </label>
        <label>
          允许的人群信号
          <select name="audience">
            <option>主动查看节点</option>
            <option>完成上一步打卡</option>
            <option>收藏路线用户</option>
            <option>公开兴趣标签匹配</option>
          </select>
        </label>
        <label>
          预估近 7 日触达
          <input
            name="reach"
            type="number"
            min="0"
            required
            defaultValue="1000"
          />
        </label>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">创建场景</button>
        </div>
      </form>
    </div>
  );
}
function RemoveScene({
  scene,
  close,
  remove,
}: {
  scene: Scene;
  close: () => void;
  remove: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">定向策略</p>
            <h2>移除场景</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <p className="modal-copy">
          “{scene.name} · {scene.node} · {scene.equipment}
          ”将停止参与新的广告投放，历史报告不会被修改。
        </p>
        <div className="dialog-actions">
          <button className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button danger-button" onClick={remove}>
            确认移除
          </button>
        </div>
      </div>
    </div>
  );
}
function PolicyDialog({
  policy,
  guardrails,
  onSave,
  close,
}: {
  policy: "frequency" | "diversity";
  guardrails: DeliveryGuardrails;
  onSave: (guardrails: DeliveryGuardrails) => void;
  close: () => void;
}) {
  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const next = {
      userDailyCap: Number(data.get("userDailyCap")),
      campaignDailyCap: Number(data.get("campaignDailyCap")),
      cooldownMinutes: Number(data.get("cooldownMinutes")),
      explorationShare: Number(data.get("explorationShare")),
      sameRouteLimit: Number(data.get("sameRouteLimit")),
    };
    if (
      !Object.values(next).every(Number.isInteger) ||
      next.userDailyCap < next.campaignDailyCap ||
      next.campaignDailyCap < 1 ||
      next.cooldownMinutes < 1 ||
      next.explorationShare < 10 ||
      next.explorationShare > 50 ||
      next.sameRouteLimit < 1
    ) {
      return;
    }
    onSave(next);
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
            <h2>{policy === "frequency" ? "频控与打散" : "防茧房规则"}</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        {policy === "frequency" ? (
          <>
            <label>
              单用户全局日曝光上限
              <input
                name="userDailyCap"
                type="number"
                min="1"
                max="30"
                required
                defaultValue={guardrails.userDailyCap}
              />
            </label>
            <label>
              单活动日曝光上限
              <input
                name="campaignDailyCap"
                type="number"
                min="1"
                max="10"
                required
                defaultValue={guardrails.campaignDailyCap}
              />
            </label>
            <label>
              同活动冷却时间（分钟）
              <input
                name="cooldownMinutes"
                type="number"
                min="1"
                max="240"
                required
                defaultValue={guardrails.cooldownMinutes}
              />
            </label>
            <input
              type="hidden"
              name="explorationShare"
              value={guardrails.explorationShare}
            />
            <input
              type="hidden"
              name="sameRouteLimit"
              value={guardrails.sameRouteLimit}
            />
          </>
        ) : (
          <>
            <label>
              探索流量占比（%）
              <input
                name="explorationShare"
                type="number"
                min="10"
                max="50"
                required
                defaultValue={guardrails.explorationShare}
              />
            </label>
            <label>
              连续同路线展示上限
              <input
                name="sameRouteLimit"
                type="number"
                min="1"
                max="5"
                required
                defaultValue={guardrails.sameRouteLimit}
              />
            </label>
            <input
              type="hidden"
              name="userDailyCap"
              value={guardrails.userDailyCap}
            />
            <input
              type="hidden"
              name="campaignDailyCap"
              value={guardrails.campaignDailyCap}
            />
            <input
              type="hidden"
              name="cooldownMinutes"
              value={guardrails.cooldownMinutes}
            />
          </>
        )}
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">保存策略</button>
        </div>
      </form>
    </div>
  );
}
