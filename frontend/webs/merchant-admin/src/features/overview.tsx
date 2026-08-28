import { Order, Product, productSkuLabel, productSpu, View } from "../domain";

export function Overview({
  products,
  orders,
  onNavigate,
  onNewProduct,
  onShip,
  onStock,
}: {
  products: Product[];
  orders: Order[];
  onNavigate: (view: View) => void;
  onNewProduct: () => void;
  onShip: (id: string) => void;
  onStock: () => void;
}) {
  const lowStock = products.filter((product) => product.stock < 21);
  const pending = orders.filter((order) => order.status === "待发货");
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">商家经营</p>
          <h1>经营总览</h1>
          <p className="muted">
            待发货与库存预警来自服务端商品目录与订单台账
          </p>
        </div>
        <div className="heading-actions">
          <button className="button primary" onClick={onNewProduct}>
            ＋ 新建商品
          </button>
        </div>
      </div>
      <div className="notice">
        <span className="notice-icon">i</span>
        <p>
          <strong>库存提醒</strong>
          <span>有 {lowStock.length} 个商品库存低于预警值，建议及时补货。</span>
        </p>
        <button className="link-button" onClick={() => onNavigate("inventory")}>
          查看库存
        </button>
      </div>
      <section className="panel">
        <div className="panel-header">
          <div>
            <h2>经营指标</h2>
            <p>成交额、访客与转化</p>
          </div>
        </div>
        <p className="panel-note">
          服务端暂未提供经营汇总指标；成交与结算数据请以订单中心、财务结算的实际台账为准。
        </p>
      </section>
      <section className="dashboard-grid lower-grid">
        <article className="panel">
          <div className="panel-header">
            <div>
              <h2>待处理订单</h2>
              <p>请在 24 小时内完成发货</p>
            </div>
            <button
              className="link-button"
              onClick={() => onNavigate("orders")}
            >
              订单中心
            </button>
          </div>
          {pending.length ? (
            pending.slice(0, 3).map((order) => (
              <div className="order-preview" key={order.id}>
                <div>
                  <span className="order-id">{order.id}</span>
                  <strong>{order.product}</strong>
                  <small>
                    {order.recipient} · {order.city}
                  </small>
                </div>
                <b>{order.amount}</b>
                <button className="table-action" onClick={() => onShip(order.id)}>
                  发货
                </button>
              </div>
            ))
          ) : (
            <p className="panel-note">暂无待发货订单。</p>
          )}
        </article>
        <article className="panel stock-panel">
          <div className="panel-header">
            <div>
              <h2>库存预警</h2>
              <p>低于预警值</p>
            </div>
            <button className="link-button" onClick={onStock}>
              去补货
            </button>
          </div>
          {lowStock.length ? (
            lowStock.map((product) => (
              <div className="stock-row" key={product.sku}>
                <img src={product.image} alt={product.name} />
                <div>
                  <strong>{product.name}</strong>
                  <small>
                    {productSpu(product)} · {productSkuLabel(product)}
                  </small>
                </div>
                <b className="danger">仅剩 {product.stock} 件</b>
              </div>
            ))
          ) : (
            <p className="panel-note">
              {products.length ? "没有低于预警值的商品。" : "暂无商品数据。"}
            </p>
          )}
        </article>
      </section>
    </section>
  );
}
