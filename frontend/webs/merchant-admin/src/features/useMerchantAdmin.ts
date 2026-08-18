import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  affiliateSettlements,
  AffiliateRule,
  initialAffiliateRules,
  initialOffers,
  initialOrders,
  initialProducts,
  initialSettlements,
  Order,
  Product,
  productImages,
  productSkuLabel,
  productSpu,
  RouteOffer,
  StockAdjustment,
} from "../domain";
import { downloadCsv, useStoredState } from "../lib/storage";
import {
  isMerchantAdminApiConfigured,
  merchantAdminApi,
  AdminApiError,
} from "../lib/adminApi";
import {
  createMallProduct,
  productFromMall,
  productsFromMall,
  updateMallProduct,
} from "../lib/adminMappers";

export function useMerchantAdmin(notify: (message: string) => void) {
  const [products, setProducts] = useStoredState<Product[]>(
    "merchant-products-v7",
    initialProducts,
  );
  const [orders, setOrders] = useStoredState<Order[]>(
    "merchant-orders-v7",
    initialOrders,
  );
  const [offers, setOffers] = useStoredState<RouteOffer[]>(
    "merchant-route-offers-v7",
    initialOffers,
  );
  const [stockAdjustments, setStockAdjustments] = useStoredState<
    StockAdjustment[]
  >("merchant-stock-adjustments-v7", []);
  const [affiliateRules, setAffiliateRules] = useStoredState<AffiliateRule[]>(
    "merchant-affiliate-rules-v1",
    initialAffiliateRules,
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
  const settlements = initialSettlements;

  useEffect(() => {
    if (!isMerchantAdminApiConfigured()) return;
    let cancelled = false;
    merchantAdminApi
      .listProducts()
      .then(({ items }) => {
        if (cancelled) return;
        setProducts(productsFromMall(items));
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
  }, [setProducts]);

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
  const toggleProductStatus = (sku: string) => {
    const product = products.find((item) => item.sku === sku);
    const offerSku = sku;
    if (
      product?.status !== "销售中" &&
      !offers.some((offer) => offer.sku === offerSku && offer.enabled)
    ) {
      notify("请先将商品挂载到启用的路线行动节点，再上架销售。");
      return;
    }
    setProducts((current) =>
      current.map((item) =>
        item.sku === sku
          ? { ...item, status: item.status === "销售中" ? "待发布" : "销售中" }
          : item,
      ),
    );
    notify(
      `“${product?.name || "商品"}”已${product?.status === "销售中" ? "下架" : "上架"}。`,
    );
  };
  const saveProduct = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
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
      if (isMerchantAdminApiConfigured()) {
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
      } else {
        setProducts((current) =>
          editingProduct
            ? current.map((item) =>
                item.sku === editingProduct.sku ? nextProduct : item,
              )
            : [nextProduct, ...current],
        );
      }
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
  const adjustStock = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const requestedQuantity = Number(data.get("quantity"));
    const sku = String(data.get("sku"));
    const product = products.find((item) => item.sku === sku);
    const reason = String(data.get("reason") || "其他");
    const quantity =
      data.get("direction") === "out" ? -requestedQuantity : requestedQuantity;
    if (
      !product ||
      !Number.isInteger(requestedQuantity) ||
      requestedQuantity < 1
    ) {
      notify("请输入有效的调整数量。");
      return;
    }
    if (product.stock + quantity < 0) {
      notify("扣减数量超过当前可售库存，无法完成调整。");
      return;
    }
    setProducts((current) =>
      current.map((item) =>
        item.sku === sku ? { ...item, stock: item.stock + quantity } : item,
      ),
    );
    setStockAdjustments((current) =>
      [
        {
          id: `ADJ-${Date.now()}`,
          sku,
          product: product?.name || "商品",
          warehouse: product?.warehouse || "北京中心仓",
          quantity,
          reason,
          createdAt: new Date().toLocaleString("zh-CN", { hour12: false }),
        },
        ...current,
      ].slice(0, 50),
    );
    setShowStockForm(false);
    notify(
      `“${product.name}”可售库存已${quantity > 0 ? "增加" : "扣减"} ${Math.abs(quantity)} 件。`,
    );
  };
  const confirmShip = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!shippingOrder) return;
    const data = new FormData(event.currentTarget);
    const carrier = String(data.get("carrier") || "顺丰速运");
    const tracking = String(data.get("tracking") || "").trim();
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
  const toggleOffer = (id: string) => {
    const target = offers.find((offer) => offer.id === id);
    setOffers((current) =>
      current.map((offer) =>
        offer.id === id ? { ...offer, enabled: !offer.enabled } : offer,
      ),
    );
    if (
      target?.enabled &&
      !offers.some(
        (offer) => offer.id !== id && offer.sku === target.sku && offer.enabled,
      )
    ) {
      setProducts((current) =>
        current.map((product) =>
          product.sku === target.sku
            ? { ...product, status: "待发布" }
            : product,
        ),
      );
      notify("已停用最后一个节点挂载，关联 SKU 已回到待发布状态。");
      return;
    }
    notify("路线商品投放状态已更新。");
  };
  const createOffer = async (offer: RouteOffer) => {
    try {
      if (isMerchantAdminApiConfigured()) {
        const product = products.find((item) => item.id === offer.productId);
        if (!product) {
          notify("未找到需要挂载的远端商品。");
          return;
        }
        await merchantAdminApi.attachNodeOffer(
          offer.routeId,
          offer.actionNodeId,
          {
            product_id: product.id,
            sku_id: product.skuId,
            creator_id: "merchant-admin",
            commission_bps: 0,
          },
          `offer-${offer.routeId}-${offer.actionNodeId}-${product.skuId}`,
        );
        setRemoteStatus("ready");
      }
      setOffers((current) => [
        { ...offer, id: `offer-${Date.now()}` },
        ...current,
      ]);
      notify("路线商品关联已创建。");
    } catch (error) {
      const apiError = error instanceof AdminApiError ? error : undefined;
      setRemoteStatus(apiError?.requiresAuthentication ? "auth" : "error");
      setRemoteMessage(apiError?.message || "路线商品关联失败，请稍后重试。");
      notify(apiError?.message || "路线商品关联失败，请稍后重试。");
    }
  };
  const removeOffer = (id: string) => {
    const removed = offers.find((offer) => offer.id === id);
    setOffers((current) => current.filter((offer) => offer.id !== id));
    if (
      removed &&
      !offers.some(
        (offer) =>
          offer.id !== id && offer.sku === removed.sku && offer.enabled,
      )
    ) {
      setProducts((current) =>
        current.map((product) =>
          product.sku === removed.sku
            ? { ...product, status: "待发布" }
            : product,
        ),
      );
      notify("已解除最后一个启用挂载，关联 SKU 已回到待发布状态。");
      return;
    }
    notify("路线商品关联已解除。商品不再在该行动节点曝光。");
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
  const exportSettlements = () =>
    downloadCsv(
      "bookway-settlements.csv",
      [
        "结算单",
        "周期",
        "订单成交",
        "平台与分账",
        "应付金额",
        "状态",
        "到账时间",
      ],
      settlements.map((item) => [
        item.id,
        item.period,
        item.gross,
        item.commission,
        item.payable,
        item.status,
        item.date,
      ]),
    );
  const exportAffiliates = () =>
    downloadCsv(
      "bookway-affiliate-settlements.csv",
      [
        "分账单",
        "创作者",
        "路线",
        "行动节点",
        "场景装备",
        "成交订单",
        "分账比例",
        "应付金额",
        "状态",
      ],
      affiliateSettlements.map((item) => [
        item.id,
        item.creator,
        item.route,
        item.node,
        item.equipment,
        String(item.orders),
        item.rate,
        item.payable,
        item.status,
      ]),
    );
  const addAffiliateRule = (rule: Omit<AffiliateRule, "id">) => {
    const matchingRules = affiliateRules.filter(
      (item) =>
        item.route === rule.route &&
        item.node === rule.node &&
        item.equipment === rule.equipment &&
        item.enabled,
    );
    if (matchingRules.some((item) => item.creator === rule.creator)) {
      return "该创作者已配置此行动节点分账规则。";
    }
    const totalRate = matchingRules.reduce((sum, item) => sum + item.rate, 0);
    if (totalRate + rule.rate > 50) {
      return "同一行动节点的启用创作者分账比例不能超过 50%。";
    }
    setAffiliateRules((current) => [
      { ...rule, id: `affiliate-rule-${Date.now()}` },
      ...current,
    ]);
    return null;
  };
  const toggleAffiliateRule = (id: string) => {
    const rule = affiliateRules.find((item) => item.id === id);
    if (!rule) return "未找到分账规则。";
    if (!rule.enabled) {
      const totalRate = affiliateRules
        .filter(
          (item) =>
            item.id !== id &&
            item.route === rule.route &&
            item.node === rule.node &&
            item.equipment === rule.equipment &&
            item.enabled,
        )
        .reduce((sum, item) => sum + item.rate, 0);
      if (totalRate + rule.rate > 50) {
        return "启用后该行动节点的创作者分账比例将超过 50%。";
      }
    }
    setAffiliateRules((current) =>
      current.map((item) =>
        item.id === id ? { ...item, enabled: !item.enabled } : item,
      ),
    );
    return null;
  };
  const removeAffiliateRule = (id: string) => {
    setAffiliateRules((current) => current.filter((item) => item.id !== id));
  };

  return {
    products,
    offers,
    stockAdjustments,
    settlements,
    affiliateSettlements,
    affiliateRules,
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
    toggleOffer,
    createOffer,
    removeOffer,
    exportOrders,
    exportSettlements,
    exportAffiliates,
    addAffiliateRule,
    toggleAffiliateRule,
    removeAffiliateRule,
    orders,
    remoteStatus,
    remoteMessage,
  };
}
