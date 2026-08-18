import { Order } from "../domain";

export function Orders({
  orders,
  filter,
  search,
  onSearch,
  onFilter,
  onShip,
  onTrack,
  onExport,
}: {
  orders: Order[];
  filter: string;
  search: string;
  onSearch: (value: string) => void;
  onFilter: (filter: string) => void;
  onShip: (id: string) => void;
  onTrack: (order: Order) => void;
  onExport: () => void;
}) {
  const tabs = ["all", "待发货", "运输中", "已完成"];
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">订单履约</p>
          <h1>订单中心</h1>
          <p className="muted">支付、发货和售后状态在此统一处理</p>
        </div>
        <button className="button secondary" onClick={onExport}>
          ⇩ 导出订单
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <div className="segmented">
            {tabs.map((tab) => (
              <button
                className={filter === tab ? "selected" : ""}
                onClick={() => onFilter(tab)}
                key={tab}
              >
                {tab === "all" ? "全部" : tab}
              </button>
            ))}
          </div>
          <label className="search">
            <span>⌕</span>
            <input
              value={search}
              onChange={(event) => onSearch(event.target.value)}
              placeholder="订单号、收件人或商品"
            />
          </label>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>订单号</th>
                <th>商品</th>
                <th>收件人</th>
                <th>金额</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {orders.length ? (
                orders.map((order) => (
                  <tr key={order.id}>
                    <td>
                      {order.id}
                      <small>{order.date}</small>
                    </td>
                    <td>{order.product}</td>
                    <td>
                      {order.recipient}
                      <small>{order.city}</small>
                    </td>
                    <td>{order.amount}</td>
                    <td>
                      <Status value={order.status} />
                    </td>
                    <td>
                      <button
                        className="table-action"
                        onClick={() =>
                          order.status === "待发货"
                            ? onShip(order.id)
                            : onTrack(order)
                        }
                      >
                        {order.status === "待发货"
                          ? "发货"
                          : order.status === "运输中"
                            ? "查看物流"
                            : "订单详情"}
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={6} className="empty-row">
                    没有匹配的订单
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function Status({ value }: { value: Order["status"] }) {
  return (
    <span
      className={`status ${value === "待发货" ? "warning" : value === "运输中" ? "info-status" : "success"}`}
    >
      {value}
    </span>
  );
}
