import { FormEvent, useState } from "react";
import {
  actionNodes,
  formatActionNode,
  Product,
  productSkuLabel,
  productSpu,
  RouteOffer,
} from "../domain";

export function RouteOffers({
  offers,
  products,
  onToggle,
  onCreate,
  onRemove,
}: {
  offers: RouteOffer[];
  products: Product[];
  onToggle: (id: string) => void;
  onCreate: (offer: RouteOffer) => void;
  onRemove: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const [removing, setRemoving] = useState<RouteOffer | null>(null);
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const binding = actionNodes.find(
      (node) => node.id === data.get("actionNodeId"),
    );
    if (!binding) {
      setError("请选择有效的路线行动节点与场景装备。");
      return;
    }
    const { route, node, equipment } = binding;
    const product = products.find(
      (item) => item.sku === String(data.get("sku")),
    );
    if (!product) {
      setError("请选择有效 SKU。");
      return;
    }
    const sku = product.sku;
    if (
      offers.some(
        (offer) =>
          offer.route === route && offer.node === node && offer.sku === sku,
      )
    ) {
      setError("该 SKU 已挂载到此路线节点，请选择其他场景装备。");
      return;
    }
    onCreate({
      id: "",
      productId: product.id,
      routeId: binding.routeId,
      actionNodeId: binding.id,
      route,
      node,
      equipment,
      product: product.name,
      sku,
      clicks: 0,
      orders: 0,
      wegu: 0,
      routeCompletion: 0,
      enabled: true,
    });
    setOpen(false);
    setError("");
  };
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">场景化商城</p>
          <h1>路线商品</h1>
          <p className="muted">
            把 SKU 挂载到路线行动节点，商品只在用户执行相应任务时出现。
          </p>
        </div>
        <button className="button primary" onClick={() => setOpen(true)}>
          ＋ 关联商品
        </button>
      </div>
      <div className="guardrail">
        <span>✓</span>
        <p>
          <strong>场景挂载护栏</strong>
          未绑定路线节点的商品不会进入推荐、搜索或广告混排。
        </p>
      </div>
      <div className="panel table-panel">
        <div className="panel-header">
          <div>
            <h2>已关联商品</h2>
            <p>点击与订单数据按节点归因</p>
          </div>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>路线 / 行动节点</th>
                <th>商品 / SKU</th>
                <th>场景装备</th>
                <th>节点点击</th>
                <th>成交订单</th>
                <th>WEGU</th>
                <th>路线完成度</th>
                <th>状态</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {offers.map((offer) => (
                <tr key={offer.id}>
                  <td>
                    <strong>{offer.route}</strong>
                    <small>{offer.node}</small>
                  </td>
                  <td>{offer.equipment}</td>
                  <td>
                    {offer.product}
                    <small>{offer.sku}</small>
                  </td>
                  <td>{offer.clicks.toLocaleString()}</td>
                  <td>{offer.orders}</td>
                  <td>{offer.wegu ? `${offer.wegu.toFixed(2)}%` : "--"}</td>
                  <td>
                    {offer.routeCompletion
                      ? `${offer.routeCompletion.toFixed(1)}%`
                      : "--"}
                  </td>
                  <td>
                    <span
                      className={`status ${offer.enabled ? "success" : "neutral"}`}
                    >
                      {offer.enabled ? "投放中" : "已停用"}
                    </span>
                  </td>
                  <td>
                    <div className="table-actions">
                      <button
                        className="table-action"
                        onClick={() => onToggle(offer.id)}
                      >
                        {offer.enabled ? "停用" : "启用"}
                      </button>
                      <button
                        className="table-action danger-action"
                        onClick={() => setRemoving(offer)}
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
      {open && (
        <OfferForm
          error={error}
          products={products}
          close={() => setOpen(false)}
          submit={submit}
        />
      )}
      {removing && (
        <RemoveOffer
          offer={removing}
          close={() => setRemoving(null)}
          remove={() => {
            onRemove(removing.id);
            setRemoving(null);
          }}
        />
      )}
    </section>
  );
}

function OfferForm({
  error,
  products,
  close,
  submit,
}: {
  error: string;
  products: Product[];
  close: () => void;
  submit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={submit}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">场景化挂载</p>
            <h2>关联路线商品</h2>
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
          路线行动节点与场景装备
          <select name="actionNodeId" required>
            {actionNodes.map((node) => (
              <option value={node.id} key={node.id}>
                {formatActionNode(node)}
              </option>
            ))}
          </select>
        </label>
        <label>
          选择 SKU
          <select name="sku">
            {products.map((product) => (
              <option value={product.sku} key={product.sku}>
                {product.name} · {product.kind} · {productSpu(product)} ·{" "}
                {productSkuLabel(product)}
              </option>
            ))}
          </select>
        </label>
        <div className="dialog-actions">
          <button type="button" className="button secondary" onClick={close}>
            取消
          </button>
          <button className="button primary">确认关联</button>
        </div>
      </form>
    </div>
  );
}
function RemoveOffer({
  offer,
  close,
  remove,
}: {
  offer: RouteOffer;
  close: () => void;
  remove: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="dialog-header">
          <div>
            <p className="eyebrow">解除场景挂载</p>
            <h2>移除路线商品</h2>
          </div>
          <button className="icon-button" onClick={close}>
            ×
          </button>
        </div>
        <p className="modal-copy">
          “{offer.product}”将不再在“{offer.route} · {offer.node} ·{" "}
          {offer.equipment}”节点曝光。
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
