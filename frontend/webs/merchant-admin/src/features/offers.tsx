import { FormEvent, useState } from "react";
import { Product, productSkuLabel, productSpu, RouteOffer } from "../domain";
import {
  AdminApiError,
  isMerchantAdminApiConfigured,
  merchantAdminApi,
  PublicRouteAction,
} from "../lib/adminApi";

type LoadedRoute = {
  id: string;
  title: string;
  authorId: string;
  actions: PublicRouteAction[];
};

export function RouteOffers({
  offers,
  products,
  onCreate,
  notify,
}: {
  offers: RouteOffer[];
  products: Product[];
  onCreate: (
    offer: RouteOffer,
    mount: { sceneEquipment: string; commissionBps: number; creatorId: string },
  ) => void;
  notify: (message: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">场景化商城</p>
          <h1>路线商品</h1>
          <p className="muted">
            把 SKU 挂载到公开路线的行动节点，商品只在用户执行相应任务时出现。
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
          未绑定路线节点的商品不会进入推荐、搜索或广告混排；只有路线作者能获得该节点的分账。
        </p>
      </div>
      <div className="panel table-panel">
        <div className="panel-header">
          <div>
            <h2>已关联商品</h2>
            <p>节点点击与成交归因暂未由网关提供，如实展示为 --</p>
          </div>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>路线 / 行动节点</th>
                <th>场景装备</th>
                <th>商品 / SKU</th>
                <th>佣金比例</th>
                <th>节点点击</th>
                <th>成交订单</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {offers.length ? (
                offers.map((offer) => (
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
                    <td>
                      {offer.commissionBps !== undefined
                        ? `${(offer.commissionBps / 100).toFixed(1)}%`
                        : "--"}
                    </td>
                    {/* 归因数据没有服务端来源；本地创建时为 0，
                        展示为 -- 避免把占位值冒充服务端统计。 */}
                    <td>{offer.clicks ? offer.clicks.toLocaleString() : "--"}</td>
                    <td>{offer.orders ? offer.orders : "--"}</td>
                    <td>
                      <span className={`status ${offer.enabled ? "success" : "neutral"}`}>
                        {offer.enabled ? "投放中" : "已停用"}
                      </span>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7} className="empty-row">
                    暂无路线商品关联；请通过「关联商品」把 SKU 挂载到公开路线的行动节点。
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
      {open && (
        <OfferForm
          products={products}
          close={() => setOpen(false)}
          onSubmit={(offer, mount) => {
            onCreate(offer, mount);
            setOpen(false);
          }}
          notify={notify}
        />
      )}
    </section>
  );
}

function OfferForm({
  products,
  close,
  onSubmit,
  notify,
}: {
  products: Product[];
  close: () => void;
  onSubmit: (
    offer: RouteOffer,
    mount: { sceneEquipment: string; commissionBps: number; creatorId: string },
  ) => void;
  notify: (message: string) => void;
}) {
  const [routeId, setRouteId] = useState("");
  const [route, setRoute] = useState<LoadedRoute | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [selectedAction, setSelectedAction] = useState<PublicRouteAction | null>(
    null,
  );
  const [equipment, setEquipment] = useState("");
  const loadRoute = async () => {
    const id = routeId.trim();
    if (!id) {
      setError("请输入公开路线的内容 ID。");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const content = await merchantAdminApi.getPublicContent(id);
      if (
        content.status !== 2 /* Published */ ||
        !content.route_template?.actions?.length
      ) {
        setError("该内容不是已发布的路线，无法挂载商品。");
        return;
      }
      setRoute({
        id: content.id,
        title: content.id,
        authorId: content.author_id,
        actions: content.route_template.actions,
      });
      setSelectedAction(null);
      setEquipment("");
    } catch (loadError) {
      const apiError =
        loadError instanceof AdminApiError ? loadError : undefined;
      setError(apiError?.message || "路线加载失败，请确认内容 ID 后重试。");
    } finally {
      setLoading(false);
    }
  };
  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const product = products.find(
      (item) => item.sku === String(data.get("sku")),
    );
    if (!product) {
      setError("请选择有效 SKU。");
      return;
    }
    if (!route || !selectedAction) {
      setError("请先加载公开路线并选择行动节点。");
      return;
    }
    if (!equipment) {
      setError("请选择该行动节点声明的场景装备。");
      return;
    }
    const commissionBps = Number(data.get("commissionPercent"));
    if (
      !Number.isFinite(commissionBps) ||
      commissionBps < 0 ||
      commissionBps > 30
    ) {
      setError("佣金比例需在 0 到 30 之间。");
      return;
    }
    onSubmit(
      {
        id: "",
        productId: product.id,
        routeId: route.id,
        actionNodeId: selectedAction.id,
        route: route.title,
        node: selectedAction.title,
        equipment,
        product: product.name,
        sku: product.sku,
        clicks: 0,
        orders: 0,
        wegu: 0,
        routeCompletion: 0,
        enabled: true,
        commissionBps: Math.round(commissionBps * 100),
      },
      {
        sceneEquipment: equipment,
        commissionBps: Math.round(commissionBps * 100),
        creatorId: route.authorId,
      },
    );
  };
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={handleSubmit}>
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
        {!isMerchantAdminApiConfigured() && (
          <p className="form-error" role="alert">
            未连接商家管理网关，挂载无法写入服务端路线节点。
          </p>
        )}
        <label>
          公开路线内容 ID
          <div className="form-grid">
            <input
              name="routeContentId"
              value={routeId}
              required
              placeholder="例如：route-weekend-hike"
              onChange={(event) => setRouteId(event.target.value)}
            />
            <button
              type="button"
              className="button secondary"
              disabled={loading}
              onClick={() => {
                void loadRoute();
              }}
            >
              {loading ? "加载中…" : "加载路线"}
            </button>
          </div>
        </label>
        {route && (
          <>
            <label>
              行动节点
              <select
                name="actionNode"
                required
                onChange={(event) => {
                  const action = route.actions.find(
                    (item) => item.id === event.target.value,
                  );
                  setSelectedAction(action ?? null);
                  setEquipment("");
                }}
              >
                <option value="" disabled>
                  选择行动节点
                </option>
                {route.actions.map((action) => (
                  <option value={action.id} key={action.id}>
                    {action.title}（{action.id}
                    {action.scene_equipment.length
                      ? ""
                      : "，未声明场景装备"}
                    ）
                  </option>
                ))}
              </select>
            </label>
            {selectedAction && selectedAction.scene_equipment.length > 0 && (
              <label>
                场景装备（节点声明）
                <select
                  name="sceneEquipment"
                  required
                  value={equipment}
                  onChange={(event) => setEquipment(event.target.value)}
                >
                  <option value="" disabled>
                    选择场景装备
                  </option>
                  {selectedAction.scene_equipment.map((item) => (
                    <option value={item} key={item}>
                      {item}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {selectedAction && selectedAction.scene_equipment.length === 0 && (
              <p className="form-error" role="alert">
                该节点未声明场景装备，不能挂载商品；请选择其他节点。
              </p>
            )}
            <label>
              创作者分账接收方（路线作者）
              <input value={route.authorId} readOnly disabled />
            </label>
          </>
        )}
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
        <label>
          佣金比例（%，0-30）
          <input
            name="commissionPercent"
            type="number"
            min="0"
            max="30"
            step="0.5"
            required
            defaultValue="10"
          />
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
