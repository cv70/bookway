import { useState } from "react";
import {
  Product,
  productSkuLabel,
  productSpu,
  StockAdjustment,
} from "../domain";

export function Products({
  products,
  search,
  status,
  onSearch,
  onStatus,
  onNew,
  onEdit,
  onToggleStatus,
}: {
  products: Product[];
  search: string;
  status: string;
  onSearch: (value: string) => void;
  onStatus: (value: string) => void;
  onNew: () => void;
  onEdit: (product: Product) => void;
  onToggleStatus: (sku: string) => void;
}) {
  const spuCount = new Set(products.map((product) => productSpu(product))).size;
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">SPU 与 SKU</p>
          <h1>商品管理</h1>
          <p className="muted">管理店铺中上架、待发布和已下架的商品</p>
        </div>
        <button className="button primary" onClick={onNew}>
          ＋ 新建 SPU/SKU
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <label className="search">
            <span>⌕</span>
            <input
              value={search}
              onChange={(event) => onSearch(event.target.value)}
              placeholder="搜索商品名称、SPU 或 SKU"
            />
          </label>
          <select
            value={status}
            onChange={(event) => onStatus(event.target.value)}
          >
            <option value="all">全部状态</option>
            <option value="selling">销售中</option>
            <option value="draft">待发布</option>
          </select>
        </div>
        <div className="table-summary">
          当前结果包含 {spuCount} 个 SPU、{products.length} 个 SKU
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>商品</th>
                <th>售价</th>
                <th>库存</th>
                <th>销量</th>
                <th>状态</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {products.length ? (
                products.map((product) => (
                  <tr key={product.sku}>
                    <td>
                      <div className="product-cell">
                        <img src={product.image} alt="" />
                        <span>
                          <strong>{product.name}</strong>
                          <small>
                            {product.kind} · {productSpu(product)} ·{" "}
                            {productSkuLabel(product)}
                          </small>
                        </span>
                      </div>
                    </td>
                    <td>{product.price}</td>
                    <td>{product.stock}</td>
                    <td>{product.sales}</td>
                    <td>
                      <Status value={product.status} />
                    </td>
                    <td>
                      <div className="table-actions">
                        <button
                          className="table-action"
                          onClick={() => onEdit(product)}
                        >
                          编辑
                        </button>
                        <button
                          className="table-action"
                          onClick={() => onToggleStatus(product.sku)}
                        >
                          {product.status === "销售中" ? "下架" : "上架"}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <Empty colSpan={6} label="没有匹配的商品" />
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

export function Inventory({
  products,
  adjustments,
  onAdjust,
}: {
  products: Product[];
  adjustments: StockAdjustment[];
  onAdjust: () => void;
}) {
  const [search, setSearch] = useState("");
  const [warehouse, setWarehouse] = useState("all");
  const visible = products
    .filter((product) =>
      `${product.name} ${productSpu(product)} ${productSkuLabel(product)}`
        .toLowerCase()
        .includes(search.toLowerCase()),
    )
    .filter(
      (product) => warehouse === "all" || product.warehouse === warehouse,
    );
  return (
    <section className="content">
      <div className="page-heading">
        <div>
          <p className="eyebrow">仓储管理</p>
          <h1>库存管理</h1>
          <p className="muted">
            可用库存由库存服务统一维护，订单预占会自动扣减可售量；预占与安全库存明细暂未由网关提供
          </p>
        </div>
        <button className="button primary" onClick={onAdjust}>
          ＋ 库存调整
        </button>
      </div>
      <div className="panel table-panel">
        <div className="table-tools">
          <label className="search">
            <span>⌕</span>
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索商品或 SKU"
            />
          </label>
          <select
            value={warehouse}
            onChange={(event) => setWarehouse(event.target.value)}
          >
            <option value="all">全部仓库</option>
            <option value="北京中心仓">北京中心仓</option>
            <option value="数字内容库">数字内容库</option>
          </select>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>商品 / SKU</th>
                <th>可售库存</th>
                <th>预占库存</th>
                <th>安全库存</th>
                <th>状态</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {visible.length ? (
                visible.map((product) => (
                  <tr key={product.sku}>
                    <td>
                      <strong>{product.name}</strong>
                      <small>
                        {product.kind} · {productSpu(product)} ·{" "}
                        {productSkuLabel(product)} · {product.warehouse}
                      </small>
                    </td>
                    <td className="stock-number">{product.stock}</td>
                    {/* 预占与安全库存由 mall-inventory 维护，网关暂未暴露读取接口，
                        如实展示为未知，而非本地编造数值。 */}
                    <td>—</td>
                    <td>—</td>
                    <td>
                      <Status value={product.stock < 21 ? "需补货" : "充足"} />
                    </td>
                    <td>
                      <button className="table-action" onClick={onAdjust}>
                        调整
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <Empty colSpan={6} label="没有匹配的库存" />
              )}
            </tbody>
          </table>
        </div>
      </div>
      <div className="panel table-panel adjustment-panel">
        <div className="panel-header">
          <div>
            <h2>最近库存调整</h2>
            <p>保留最近 50 条操作记录</p>
          </div>
        </div>
        <div className="responsive-table">
          <table>
            <thead>
              <tr>
                <th>调整单</th>
                <th>商品 / SKU</th>
                <th>仓库</th>
                <th>数量</th>
                <th>原因</th>
                <th>操作时间</th>
              </tr>
            </thead>
            <tbody>
              {adjustments.length ? (
                adjustments.slice(0, 8).map((item) => (
                  <tr key={item.id}>
                    <td>
                      <strong>{item.id}</strong>
                    </td>
                    <td>
                      {item.product}
                      <small>{item.sku}</small>
                    </td>
                    <td>{item.warehouse}</td>
                    <td className="stock-number">
                      {item.quantity > 0 ? "+" : ""}
                      {item.quantity}
                    </td>
                    <td>{item.reason}</td>
                    <td>{item.createdAt}</td>
                  </tr>
                ))
              ) : (
                <Empty colSpan={6} label="暂无库存调整记录" />
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function Status({ value }: { value: string }) {
  const className =
    value === "销售中" || value === "充足"
      ? "success"
      : value === "需补货"
        ? "danger-status"
        : "neutral";
  return <span className={`status ${className}`}>{value}</span>;
}
function Empty({ colSpan, label }: { colSpan: number; label: string }) {
  return (
    <tr>
      <td colSpan={colSpan} className="empty-row">
        {label}
      </td>
    </tr>
  );
}
