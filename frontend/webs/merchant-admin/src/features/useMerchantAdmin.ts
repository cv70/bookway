import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  AffiliateSettlement,
  Order,
  Product,
  productImages,
  productSkuLabel,
  productSpu,
  RouteOffer,
  StockAdjustment,
} from "../domain";
import { downloadCsv } from "../lib/storage";
import {
  isMerchantAdminApiConfigured,
  merchantAdminApi,
  AdminApiError,
} from "../lib/adminApi";
import {
  affiliateFromMall,
  createMallProduct,
  productFromMall,
  productsFromMall,
  updateMallProduct,
} from "../lib/adminMappers";

// Honest-guardrail wording: with no gateway there is no server truth to show.
const GATEWAY_DISCONNECTED = "未连接商家管理网关，无法读写服务端数据。";

export function useMerchantAdmin(notify: (message: string) => void) {
  // 商品、订单、路线挂载与分账台账只来自服务端网关；内存态从空开始，
  // 不落 localStorage，也不在未连接时用本地数据冒充服务端事实。
  const [products, setProducts] = useState<Product[]>([]);
  const [orders, setOrders] = useState<Order[]>([]);
  const [offers, setOffers] = useState<RouteOffer[]>([]);
  const [stockAdjustments, setStockAdjustments] = useState<StockAdjustment[]>(
    [],
  );
  const [productSearch, setProductSearch] = useState("");
  const [productStatus, setProductStatus] = useState("all");
  const [orderFilter, setOrderFilter] = useState("all");
  const [orderSearch, setOrderSearch] = useState("");
  const [showProductForm, setShowProductForm] = useState(false);
  const [editingProduct, setEditingProduct] = useState<Product | null>(null);
  const [showStockForm, setShowStockForm] = useState(false);
  const [shippingOrder, setShippingOrder] = useState<Order | null>(null);
  const [trackingOrder, setTrackingOrder] = useState<Order | null>(null);
  const [remoteStatus, setRemoteStatus] = useState<
    "local" | "loading" | "ready" | "auth" | "error"
  >(isMerchantAdminApiConfigured() ? "loading" : "local");
  const [remoteMessage, setRemoteMessage] = useState("");
  const [affiliateRows, setAffiliateRows] = useState<AffiliateSettlement[]>([]);

  useEffect(() => {
    if (!isMerchantAdminApiConfigured()) return;
    let cancelled = false;
    Promise.all([
      merchantAdminApi.listProducts(),
      merchantAdminApi.listOrders(),
      merchantAdminApi.listAffiliateSettlements(),
    ])
      .then(([catalog, remoteOrders, remoteSettlements]) => {
        if (cancelled) return;
        setProducts(productsFromMall(catalog.items));
        setOrders(
          remoteOrders.items.map((order) => ({
            id: order.id,
            date: order.created_at,
            product: order.items.map((item) => `${item.title} ×${item.quantity}`).join("、"),
            recipient: "—",
            city: "—",
            amount: `¥${(order.total_cents / 100).toFixed(2)}`,
            status:
              order.fulfillment_status === 3
                ? "已完成"
                : order.fulfillment_status === 2
                  ? "运输中"
                  : "待发货",
            tracking: order.tracking_number || undefined,
          })),
        );
        setAffiliateRows(remoteSettlements.items.map(affiliateFromMall));
        setRemoteStatus("ready");
        setRemoteMessage("");
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const apiError = error instanceof AdminApiError ? error : undefined;
        setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
        setRemoteMessage(
          apiError?.requiresAuthentication
            ? "登录已过期，请重新登录商家后台。"
            : apiError?.message || "远端商品目录暂时不可用。",
        );
      });
    return () => {
      cancelled = true;
    };
  }, [setProducts, setOrders, setAffiliateRows]);

  useEffect(() => {
    setProducts((current) => {
      const normalized = current.map((product) => {
        const sku = product.sku;
        return product.status === "销售中" &&
          !offers.some((offer) => offer.sku === sku && offer.enabled)
          ? { ...product, status: "待发布" as const }
          : product;
      });
      return normalized.some((product, index) => product !== current[index])
        ? normalized
        : current;
    });
  }, [offers, setProducts]);

  const filteredProducts = useMemo(
    () =>
      products.filter(
        (product) =>
          (productStatus === "all" ||
            (productStatus === "selling") === (product.status === "销售中")) &&
          `${product.name} ${productSpu(product)} ${productSkuLabel(product)}`
            .toLowerCase()
            .includes(productSearch.toLowerCase()),
      ),
    [products, productSearch, productStatus],
  );
  const filteredOrders = useMemo(
    () =>
      orders.filter(
        (order) =>
          (orderFilter === "all" || order.status === orderFilter) &&
          `${order.id} ${order.product} ${order.recipient}`
            .toLowerCase()
            .includes(orderSearch.toLowerCase()),
      ),
    [orders, orderFilter, orderSearch],
  );
  const closeProductDialog = () => {
    setShowProductForm(false);
    setEditingProduct(null);
  };
  const beginShip = (id: string) =>
    setShippingOrder(orders.find((order) => order.id === id) || null);
  // 上架/下架通过网关写回 mall-catalog；未连接网关时直接拒绝，
  // 不在本地假装状态已变更。
  const toggleProductStatus = async (sku: string) => {
    const product = products.find((item) => item.sku === sku);
    if (!product) return;
    const nextStatus: Product["status"] =
      product.status === "销售中" ? "待发布" : "销售中";
    if (
      nextStatus === "销售中" &&
      !offers.some((offer) => offer.sku === sku && offer.enabled)
    ) {
      notify("请先将商品挂载到启用的路线行动节点，再上架销售。");
      return;
    }
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    try {
      const remote = await merchantAdminApi.updateProduct(
        product.id,
        updateMallProduct({ ...product, status: nextStatus }),
      );
      const remoteProducts = remote.skus.map((skuItem) =>
        productFromMall(remote, skuItem),
      );
      setProducts((current) =>
        current.map((item) =>
          item.sku === sku
            ? remoteProducts.find((remoteItem) => remoteItem.skuId === item.skuId) ||
              { ...product, status: nextStatus }
            : item,
        ),
      );
      setRemoteStatus("ready");
      notify(`“${product.name}”已${nextStatus === "销售中" ? "上架" : "下架"}。`);
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "商品状态同步失败，请稍后重试。");
      notify(apiError?.message || "商品状态同步失败，请稍后重试。");
    }
  };
  const saveProduct = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    const data = new FormData(event.currentTarget);
    const name = String(data.get("name") || "").trim();
    const description = String(data.get("description") || "").trim();
    const price = Number(data.get("price"));
    const stock = Number(data.get("stock"));
    let nextProduct: Product;
    if (editingProduct) {
      nextProduct = {
        ...editingProduct,
        name,
        description,
        price: `¥${price.toFixed(2)}`,
        stock,
      };
    } else {
      const spu = String(data.get("spu") || "")
        .trim()
        .toUpperCase();
      const sku = String(data.get("sku") || "")
        .trim()
        .toUpperCase();
      const variant = String(data.get("variant") || "").trim();
      const kind = data.get("kind") === "课程" ? "课程" : "装备";
      const warehouse =
        kind === "课程" || data.get("warehouse") === "数字内容库"
          ? "数字内容库"
          : "北京中心仓";
      if (products.some((product) => product.sku === sku)) {
        notify("SKU 编码已存在，请为该 SPU 使用唯一 SKU。");
        return;
      }
      nextProduct = {
        id: `product-${crypto.randomUUID()}`,
        name,
        spu,
        description,
        sku,
        skuId: `sku-${crypto.randomUUID()}`,
        variant,
        kind,
        warehouse,
        price: `¥${price.toFixed(2)}`,
        stock,
        sales: 0,
        status: "待发布",
        image: productImages[3],
      };
    }
    try {
      const remote = editingProduct
        ? await merchantAdminApi.updateProduct(
            editingProduct.id,
            updateMallProduct(nextProduct),
          )
        : await merchantAdminApi.createProduct(createMallProduct(nextProduct));
      const remoteProducts = remote.skus.map((sku) =>
        productFromMall(remote, sku),
      );
      setProducts((current) =>
        editingProduct
          ? current.map((item) =>
              item.id === editingProduct.id
                ? remoteProducts.find((remoteItem) => remoteItem.skuId === item.skuId) ||
                  nextProduct
                : item,
            )
          : [...remoteProducts, ...current],
      );
      setRemoteStatus("ready");
      notify(
        editingProduct
          ? `“${name}”已更新。`
          : `“${name}”已创建，当前为待发布状态。`,
      );
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "商品保存失败，请稍后重试。");
      notify(apiError?.message || "商品保存失败，请稍后重试。");
      return;
    }
    closeProductDialog();
  };
  // Stock management submits the target saleable count directly, matching the
  // inventory service's absolute SetStock semantics: no read-modify-write
  // race, and concurrent reservations can reject an over-reduction.
  const adjustStock = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const targetStock = Number(data.get("target_stock"));
    const sku = String(data.get("sku"));
    const product = products.find((item) => item.sku === sku);
    const reason = String(data.get("reason") || "其他");
    if (
      !product ||
      !Number.isInteger(targetStock) ||
      targetStock < 0
    ) {
      notify("请输入有效的目标可售数量。");
      return;
    }
    const quantity = targetStock - product.stock;
    if (quantity === 0) {
      setShowStockForm(false);
      notify(`“${product.name}”可售库存未变化。`);
      return;
    }
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    try {
      const remote = await merchantAdminApi.setSkuStock(
        product.skuId,
        targetStock,
      );
      setProducts((current) =>
        current.map((item) =>
          item.sku === sku ? { ...item, stock: remote.available } : item,
        ),
      );
      setStockAdjustments((current) =>
        [
          {
            id: `ADJ-${Date.now()}`,
            sku,
            product: product.name,
            warehouse: product.warehouse || "北京中心仓",
            quantity,
            reason,
            createdAt: new Date().toLocaleString("zh-CN", { hour12: false }),
          },
          ...current,
        ].slice(0, 50),
      );
      setRemoteStatus("ready");
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "库存同步失败，请稍后重试。");
      notify(
        apiError?.message ||
          "库存调整被库存服务拒绝（可能低于订单预占），请稍后重试。",
      );
      return;
    }
    setShowStockForm(false);
    notify(
      `“${product.name}”可售库存已${quantity > 0 ? "增加" : "扣减"} ${Math.abs(quantity)} 件。`,
    );
  };
  const confirmShip = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!shippingOrder) return;
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    const data = new FormData(event.currentTarget);
    const carrier = String(data.get("carrier") || "顺丰速运");
    const tracking = String(data.get("tracking") || "").trim();
    try {
      await merchantAdminApi.updateFulfillment(shippingOrder.id, 2, tracking);
    } catch (error) {
      notify(error instanceof AdminApiError ? error.message : "发货状态同步失败。");
      return;
    }
    setOrders((current) =>
      current.map((order) =>
        order.id === shippingOrder.id
          ? { ...order, status: "运输中", carrier, tracking }
          : order,
      ),
    );
    setShippingOrder(null);
    notify(`订单 ${shippingOrder.id} 已发货，物流单号 ${tracking}。`);
  };
  // Payout settles an eligible creator share through the mall-order ledger.
  // The backend is idempotent on replays and rejects non-eligible rows with
  // FailedPrecondition; the local guard only avoids futile calls.
  const payAffiliate = async (settlementId: string) => {
    const target = affiliateRows.find((item) => item.id === settlementId);
    if (!target) {
      notify("未找到该分账单。");
      return;
    }
    if (target.status !== "待结算") {
      notify(`该笔分账当前为“${target.status}”，无法打款。`);
      return;
    }
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    try {
      const remote = await merchantAdminApi.settleAffiliate(settlementId);
      setAffiliateRows((current) =>
        current.map((item) =>
          item.id === settlementId ? affiliateFromMall(remote) : item,
        ),
      );
      setRemoteStatus("ready");
      notify(`分账单 ${remote.id} 已完成打款。`);
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "打款失败，请稍后重试。");
      notify(apiError?.message || "打款失败，请稍后重试。");
    }
  };
  // Mounting an offer requires the node's declared scene equipment, the
  // route author as the commission recipient, and a 0-3000 bps commission —
  // all revalidated server-side; the caller just supplies what it selected.
  const createOffer = async (
    offer: RouteOffer,
    mount: { sceneEquipment: string; commissionBps: number; creatorId: string },
  ) => {
    if (!isMerchantAdminApiConfigured()) {
      notify(GATEWAY_DISCONNECTED);
      return;
    }
    try {
      const product = products.find((item) => item.id === offer.productId);
      if (!product) {
        notify("未找到需要挂载的远端商品。");
        return;
      }
      const remote = await merchantAdminApi.attachNodeOffer(
        offer.routeId,
        offer.actionNodeId,
        {
          product_id: product.id,
          sku_id: product.skuId,
          creator_id: mount.creatorId,
          commission_bps: mount.commissionBps,
          scene_equipment: mount.sceneEquipment,
        },
        `offer-${offer.routeId}-${offer.actionNodeId}-${product.skuId}-${mount.sceneEquipment}`,
      );
      setOffers((current) => [
        { ...offer, id: remote.id || `offer-${Date.now()}` },
        ...current,
      ]);
      setRemoteStatus("ready");
      notify("路线商品关联已创建。");
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "路线商品关联失败，请稍后重试。");
      notify(apiError?.message || "路线商品关联失败，请检查路线节点与装备后重试。");
    }
  };
  const exportOrders = () =>
    downloadCsv(
      `bookway-orders-${new Date().toISOString().slice(0, 10)}.csv`,
      [
        "订单号",
        "日期",
        "商品",
        "收件人",
        "城市",
        "金额",
        "状态",
        "承运商",
        "物流单号",
      ],
      orders.map((order) => [
        order.id,
        order.date,
        order.product,
        order.recipient,
        order.city,
        order.amount,
        order.status,
        order.carrier || "",
        order.tracking || "",
      ]),
    );
  const exportAffiliates = () =>
    downloadCsv(
      "bookway-affiliate-settlements.csv",
      ["分账单", "订单", "创作者", "应付金额", "状态", "时间"],
      affiliateRows.map((item) => [
        item.id,
        item.order_id,
        item.creator,
        item.payable,
        item.status,
        item.date,
      ]),
    );

  return {
    products,
    offers,
    stockAdjustments,
    affiliateSettlements: affiliateRows,
    filteredProducts,
    filteredOrders,
    productSearch,
    productStatus,
    orderFilter,
    orderSearch,
    showProductForm,
    editingProduct,
    showStockForm,
    shippingOrder,
    trackingOrder,
    setProductSearch,
    setProductStatus,
    setOrderFilter,
    setOrderSearch,
    setShowProductForm,
    setEditingProduct,
    setShowStockForm,
    setShippingOrder,
    setTrackingOrder,
    closeProductDialog,
    beginShip,
    toggleProductStatus,
    saveProduct,
    adjustStock,
    confirmShip,
    createOffer,
    exportOrders,
    exportAffiliates,
    payAffiliate,
    orders,
    remoteStatus,
    remoteMessage,
  };
}
