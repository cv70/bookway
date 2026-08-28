import { FormEvent } from "react";
import { Order, Product, productSkuLabel, productSpu } from "../domain";

export function ProductDialog({
  product,
  onClose,
  onSubmit,
}: {
  product?: Product;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={onSubmit}>
        <DialogHeader
          eyebrow="商品信息"
          title={product ? "编辑商品" : "新建商品"}
          onClose={onClose}
        />
        <label>
          商品名称
          <input
            name="name"
            required
            defaultValue={product?.name}
            placeholder="例如：轻量徒步背包 20L"
          />
        </label>
        <label>
          SPU 编码
          <input
            name="spu"
            required
            disabled={Boolean(product)}
            defaultValue={product ? productSpu(product) : undefined}
            placeholder="例如：SPU-TR20"
            pattern="SPU-[A-Za-z0-9-]{2,32}"
          />
        </label>
        <div className="form-grid">
          <label>
            SKU 编码
            <input
              name="sku"
              required
              disabled={Boolean(product)}
              defaultValue={product?.sku}
              placeholder="例如：SKU-TR20-G"
              pattern="SKU-[A-Za-z0-9-]{2,32}"
            />
          </label>
          <label>
            规格
            <input
              name="variant"
              required
              disabled={Boolean(product)}
              defaultValue={product?.variant}
              placeholder="例如：岩灰色 / 20L"
            />
          </label>
        </div>
        <label>
          商品类型
          <select
            name="kind"
            defaultValue={product?.kind || "装备"}
            disabled={Boolean(product)}
          >
            <option value="装备">场景装备</option>
            <option value="课程">行动课程</option>
          </select>
        </label>
        <label>
          库存仓库
          <select
            name="warehouse"
            defaultValue={product?.warehouse || "北京中心仓"}
            disabled={Boolean(product)}
          >
            <option value="北京中心仓">北京中心仓</option>
            <option value="数字内容库">数字内容库</option>
          </select>
        </label>
        <div className="form-grid">
          <label>
            售价
            <input
              name="price"
              required
              type="number"
              min="0"
              step="0.01"
              defaultValue={product?.price.replace("¥", "")}
              placeholder="0.00"
            />
          </label>
          <label>
            初始库存
            <input
              name="stock"
              required
              type="number"
              min="0"
              defaultValue={product?.stock}
              placeholder="0"
            />
          </label>
        </div>
        <label>
          商品描述
          <textarea
            name="description"
            required
            defaultValue={product?.description}
            placeholder="描述商品特点和适用场景"
          />
        </label>
        <DialogActions
          onClose={onClose}
          submit={product ? "保存更改" : "创建商品"}
        />
      </form>
    </div>
  );
}

export function StockDialog({
  products,
  onClose,
  onSubmit,
}: {
  products: Product[];
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={onSubmit}>
        <DialogHeader
          eyebrow="库存调整"
          title="调整可售库存"
          onClose={onClose}
        />
        <label>
          商品
          <select name="sku" required>
            {products.map((product) => (
              <option value={product.sku} key={product.sku}>
                {product.name} · {product.kind} · {productSpu(product)} ·{" "}
                {productSkuLabel(product)}
              </option>
            ))}
          </select>
        </label>
        <label>
          目标可售数量
          <input
            name="target_stock"
            required
            type="number"
            min="0"
            defaultValue="10"
          />
        </label>
        <label>
          调整原因
          <select name="reason">
            <option>采购入库</option>
            <option>盘点调整</option>
            <option>报损/出库</option>
            <option>其他</option>
          </select>
        </label>
        <DialogActions onClose={onClose} submit="确认调整" />
      </form>
    </div>
  );
}

export function ShippingDialog({
  order,
  onClose,
  onSubmit,
}: {
  order: Order;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <div className="modal-backdrop">
      <form className="modal" onSubmit={onSubmit}>
        <DialogHeader eyebrow="订单履约" title="确认发货" onClose={onClose} />
        <div className="shipping-summary">
          <strong>{order.product}</strong>
          <span>
            {order.id} · {order.recipient} · {order.city}
          </span>
        </div>
        <label>
          物流公司
          <select name="carrier">
            <option>顺丰速运</option>
            <option>中通快递</option>
            <option>京东物流</option>
          </select>
        </label>
        <label>
          物流单号
          <input
            name="tracking"
            required
            pattern="[A-Za-z0-9-]{6,32}"
            placeholder="请输入 6-32 位单号"
          />
        </label>
        <DialogActions onClose={onClose} submit="确认发货" />
      </form>
    </div>
  );
}

export function TrackingDialog({
  order,
  onClose,
}: {
  order: Order;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <DialogHeader eyebrow="物流详情" title={order.id} onClose={onClose} />
        <div className="shipping-summary">
          <strong>{order.product}</strong>
          <span>
            {order.recipient} · {order.city}
          </span>
          <span>
            {order.carrier || "物流"} · {order.tracking || "暂无单号"}
          </span>
        </div>
        {/* 网关暂未提供物流轨迹接口；只展示订单台账中的承运商与单号，
            不编造物流事件时间线。 */}
        <p className="modal-copy">
          物流轨迹暂未由网关提供，请以承运商（{order.carrier || "承运方"}
          ）官方渠道的查询结果为准。
        </p>
        <div className="dialog-actions">
          <button className="button secondary" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

function DialogHeader({
  eyebrow,
  title,
  onClose,
}: {
  eyebrow: string;
  title: string;
  onClose: () => void;
}) {
  return (
    <div className="dialog-header">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h2>{title}</h2>
      </div>
      <button
        type="button"
        className="icon-button"
        onClick={onClose}
        title="关闭"
      >
        ×
      </button>
    </div>
  );
}
function DialogActions({
  onClose,
  submit,
}: {
  onClose: () => void;
  submit: string;
}) {
  return (
    <div className="dialog-actions">
      <button type="button" className="button secondary" onClick={onClose}>
        取消
      </button>
      <button className="button primary">{submit}</button>
    </div>
  );
}
