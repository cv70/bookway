import { FormEvent, useState } from "react";
import { ActionNodeBinding, Creative, formatBinding } from "../domain";

export function Creatives({
  creatives,
  bindings,
  addCreative,
  reviewCreative,
  onRemove,
  notify,
}: {
  creatives: Creative[];
  bindings: ActionNodeBinding[];
  addCreative: (event: FormEvent<HTMLFormElement>) => void;
  reviewCreative: (name: string, status: Creative["status"]) => void;
  onRemove: (name: string) => void;
  notify: (message: string) => void;
}) {
  const [filter, setFilter] = useState("全部状态");
  const [preview, setPreview] = useState<Creative | null>(null);
  const [removing, setRemoving] = useState<Creative | null>(null);
  const visible = creatives.filter(
    (item) => filter === "全部状态" || item.status === filter,
  );
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">创意资产</p>
          <h1>广告素材</h1>
          <p className="muted">
            素材必须绑定路线行动节点，审核通过后才会参与 eCPM 竞价。
          </p>
        </div>
        <button
          className="button primary"
          onClick={() =>
            document
              .getElementById("creative-form")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          ＋ 上传素材
        </button>
      </div>
      <div className="dashboard-grid">
        <div className="panel table-panel">
          <div className="panel-header">
            <div>
              <h2>素材列表</h2>
              <p>
                显示 {visible.length} / {creatives.length} 个素材 · 最近更新优先
              </p>
            </div>
            <select
              className="compact-select"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
            >
              <option>全部状态</option>
              <option>已通过</option>
              <option>审核中</option>
              <option>需修改</option>
            </select>
          </div>
          <div className="responsive-table">
            <table>
              <thead>
                <tr>
                  <th>素材名称</th>
                  <th>格式</th>
                  <th>关联节点 / 场景装备</th>
                  <th>状态</th>
                  <th>更新时间</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {visible.map((item) => (
                  <tr key={item.name}>
                    <td>
                      <strong>{item.name}</strong>
                    </td>
                    <td>{item.format}</td>
                    <td>{formatBinding(item.binding)}</td>
                    <td>
                      <span
                        className={`status ${item.status === "已通过" ? "approved" : item.status === "审核中" ? "draft" : "paused"}`}
                      >
                        {item.status}
                      </span>
                    </td>
                    <td>{item.updated}</td>
                    <td>
                      <div className="table-actions">
                        <button
                          className="table-action"
                          onClick={() =>
                            item.status === "需修改"
                              ? notify("已发送修改提醒。")
                              : setPreview(item)
                          }
                        >
                          查看
                        </button>
                        {item.status === "审核中" && (
                          <>
                            <button
                              className="table-action"
                              onClick={() =>
                                reviewCreative(item.name, "已通过")
                              }
                            >
                              通过
                            </button>
                            <button
                              className="table-action danger-action"
                              onClick={() =>
                                reviewCreative(item.name, "需修改")
                              }
                            >
                              退回
                            </button>
                          </>
                        )}
                        <button
                          className="table-action danger-action"
                          onClick={() => setRemoving(item)}
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
        <CreativeForm bindings={bindings} onSubmit={addCreative} />
      </div>
      {preview && <Preview creative={preview} close={() => setPreview(null)} />}
      {removing && (
        <RemoveCreative
          creative={removing}
          close={() => setRemoving(null)}
          remove={() => {
            onRemove(removing.name);
            setRemoving(null);
          }}
        />
      )}
    </section>
  );
}

function CreativeForm({
  bindings,
  onSubmit,
}: {
  bindings: ActionNodeBinding[];
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <form
      id="creative-form"
      className="panel creative-form"
      onSubmit={onSubmit}
    >
      <div className="panel-header">
        <div>
          <h2>提交新素材</h2>
          <p>绑定场景后进入审核队列</p>
        </div>
      </div>
      <label>
        素材名称
        <input name="name" required placeholder="例如：登山鞋节点卡片" />
      </label>
      <label>
        素材格式
        <select name="format">
          <option>路线节点卡片</option>
          <option>节点信息流</option>
          <option>路线装备推荐</option>
        </select>
      </label>
      <label>
        关联行动节点与场景装备
        <select name="actionNodeId" required>
          {bindings.map((binding) => (
            <option value={binding.id} key={binding.id}>
              {formatBinding(binding)}
            </option>
          ))}
        </select>
      </label>
      <label>
        行动节点说明
        <textarea
          name="contextNote"
          required
          minLength={12}
          maxLength={140}
          placeholder="说明素材如何帮助用户完成当前节点，不要使用强促销文案"
        />
      </label>
      <label>
        素材文件
        <input name="asset" type="file" accept="image/*" required />
      </label>
      <div className="frequency">
        <strong>审核规则</strong>
        <p>不允许硬广文案；素材需与节点任务和场景装备直接相关。</p>
      </div>
      <button className="button primary">提交审核</button>
    </form>
  );
}
function Preview({
  creative,
  close,
}: {
  creative: Creative;
  close: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div
        className="modal creative-preview"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialog-header">
          <div>
            <p className="eyebrow">素材预览</p>
            <h2>{creative.name}</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        {creative.assetData ? (
          <div className="preview-art preview-image">
            <img src={creative.assetData} alt={creative.name} />
            <span>{creative.format}</span>
            <small>{formatBinding(creative.binding)} · 仅在行动节点展示</small>
            <p>{creative.contextNote}</p>
          </div>
        ) : (
          <div className="preview-art">
            <span>{creative.format}</span>
            <strong>{formatBinding(creative.binding)}</strong>
            <small>场景装备推荐 · 仅在行动节点展示</small>
            <p>{creative.contextNote}</p>
          </div>
        )}
        <div className="dialog-actions">
          <button className="button secondary" onClick={close}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
function RemoveCreative({
  creative,
  close,
  remove,
}: {
  creative: Creative;
  close: () => void;
  remove: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">素材管理</p>
            <h2>移除素材</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <p className="modal-copy">
          “{creative.name}”将从审核队列移除，已关联的投放活动不会再读取该素材。
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
