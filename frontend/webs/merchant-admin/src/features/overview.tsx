import { Order, Product, productSkuLabel, productSpu, View } from "../domain";

export function Overview({
  products,
  orders,
  onNavigate,
  onNewProduct,
  onShip,
  onStock,
  onExport,
}: {
  products: Product[];
  orders: Order[];
  onNavigate: (view: View) => void;
  onNewProduct: () => void;
  onShip: (id: string) => void;
  onStock: () => void;
  onExport: () => void;
}) {
  const lowStock = products.filter((product) => product.stock < 21);
  const pending = orders.filter((order) => order.status === "待发货");
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">2026 年 8 月 18 日，星期二</p>
          <h1>经营总览</h1>
          <p className="muted">店铺经营数据每小时更新</p>
        </div>
        <div className="heading-actions">
          <button className="button secondary" onClick={onExport}>
            ⇩ 下载报表
          </button>
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
      <Metrics />
      <section className="dashboard-grid">
        <Trend />
        <RoutePanel onNavigate={onNavigate} />
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
          {pending.slice(0, 3).map((order) => (
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
          ))}
        </article>
        <article className="panel stock-panel">
          <div className="panel-header">
            <div>
              <h2>库存预警</h2>
              <p>低于安全库存</p>
            </div>
            <button className="link-button" onClick={onStock}>
              去补货
            </button>
          </div>
          {lowStock.map((product) => (
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
          ))}
        </article>
      </section>
    </section>
  );
}

function Metrics() {
  return (
    <section className="metric-grid">
      {[
        ["今日成交额", "¥12,680.00", "↑ 18.6%", "up", "较昨日"],
        ["支付订单", "86", "↑ 12.1%", "up", "较昨日"],
        ["访客数", "2,438", "↑ 8.4%", "up", "较昨日"],
        ["转化率", "3.53%", "↓ 0.3%", "down", "较昨日"],
      ].map(([label, value, delta, tone, note]) => (
        <article className="metric-card" key={label}>
          <p>{label}</p>
          <strong>{value}</strong>
          <span className={tone}>{delta}</span>
          <small>{note}</small>
        </article>
      ))}
    </section>
  );
}
function Trend() {
  return (
    <article className="panel trend-panel">
      <div className="panel-header">
        <div>
          <h2>成交趋势</h2>
          <p>近 7 天</p>
        </div>
        <div className="legend">
          <span>
            <i className="line-dot green" />
            成交额
          </span>
          <span>
            <i className="line-dot orange" />
            订单数
          </span>
        </div>
      </div>
      <div className="chart">
        <div className="y-axis">
          {["¥16k", "¥12k", "¥8k", "¥4k", "¥0"].map((label) => (
            <span key={label}>{label}</span>
          ))}
        </div>
        <div className="plot">
          {[1, 2, 3, 4, 5].map((item) => (
            <div className="grid-line" key={item} />
          ))}
          <div className="chart-line">
            {[40, 55, 48, 68, 59, 80, 73].map((height, index) => (
              <i
                className={index === 6 ? "current" : ""}
                style={{ height: `${height}%` }}
                key={index}
              />
            ))}
          </div>
          <div className="chart-labels">
            {[
              "12 日",
              "13 日",
              "14 日",
              "15 日",
              "16 日",
              "17 日",
              "18 日",
            ].map((label) => (
              <span key={label}>{label}</span>
            ))}
          </div>
        </div>
      </div>
    </article>
  );
}
function RoutePanel({ onNavigate }: { onNavigate: (view: View) => void }) {
  const routes = [
    ["teal", "周末轻徒步入门", "西山 5 公里路线", "¥2,840", "12 单"],
    ["orange", "城市骑行第一课", "城市骑行路线", "¥1,960", "8 单"],
    ["blue", "夏日露营清单", "两天一夜路线", "¥1,280", "5 单"],
  ];
  return (
    <article className="panel route-panel">
      <div className="panel-header">
        <div>
          <h2>路线商品表现</h2>
          <p>来自关联路线的成交</p>
        </div>
        <button className="link-button" onClick={() => onNavigate("offers")}>
          查看全部
        </button>
      </div>
      <div className="route-list">
        {routes.map(([color, title, description, amount, count]) => (
          <div className="route-row" key={title}>
            <span className={`route-icon ${color}`}>⌁</span>
            <div>
              <strong>{title}</strong>
              <small>{description}</small>
            </div>
            <b>{amount}</b>
            <span className="tag good">{count}</span>
          </div>
        ))}
      </div>
    </article>
  );
}
