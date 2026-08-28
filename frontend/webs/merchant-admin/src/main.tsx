import { useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ProductDialog,
  ShippingDialog,
  StockDialog,
  TrackingDialog,
} from "./components/dialogs";
import { View, nav, titles } from "./domain";
import { Finance } from "./features/finance";
import { Overview } from "./features/overview";
import { Orders } from "./features/orders";
import { RouteOffers } from "./features/offers";
import { Inventory, Products } from "./features/catalog";
import { useMerchantAdmin } from "./features/useMerchantAdmin";
import "../styles.css";
import "./enhancements.css";

function App() {
  const [view, setView] = useState<View>(
    (location.hash.slice(1) as View) || "overview",
  );
  const [menuOpen, setMenuOpen] = useState(false);
  const [toast, setToast] = useState("");
  const notify = (message: string) => {
    setToast(message);
    window.setTimeout(() => setToast(""), 2600);
  };
  const merchant = useMerchantAdmin(notify);
  const navigate = (next: View) => {
    setView(next);
    setMenuOpen(false);
    history.replaceState(null, "", `#${next}`);
  };
  // 待发货提醒来自服务端订单台账，不再使用写死的“3 条”。
  const pendingShipments = merchant.orders.filter(
    (order) => order.status === "待发货",
  ).length;
  return (
    <div className="app-shell">
      <aside className={`sidebar ${menuOpen ? "open" : ""}`}>
        <a
          className="brand"
          href="#overview"
          onClick={() => navigate("overview")}
        >
          <span className="brand-mark">B</span>
          <span>BOOKWAY</span>
        </a>
        <div className="store-switcher">
          <span className="store-avatar">万</span>
          <span>
            <strong>万卷行</strong>
            <small>商家中心</small>
          </span>
          <button
            className="icon-button"
            title="切换店铺"
            onClick={() => notify("多店铺切换暂未接入。")}
          >
            ⌄
          </button>
        </div>
        <nav>
          {nav.map((item) => (
            <a
              className={`nav-item ${view === item.view ? "active" : ""}`}
              href={`#${item.view}`}
              key={item.view}
              onClick={(event) => {
                event.preventDefault();
                navigate(item.view);
              }}
            >
              <span>{item.icon}</span>
              {item.label}
              {item.view === "orders" && (
                <b>
                  {
                    merchant.orders.filter((order) => order.status === "待发货")
                      .length
                  }
                </b>
              )}
            </a>
          ))}
        </nav>
        <div className="sidebar-footer">
          <a
            className="nav-item"
            href="#settings"
            onClick={(event) => {
              event.preventDefault();
              notify("店铺设置暂未接入。");
            }}
          >
            <span>⚙</span>店铺设置
          </a>
          <div className="help">
            <strong>需要帮助？</strong>
            <span>联系平台商家支持</span>
          </div>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <button
            className="mobile-menu icon-button"
            title="打开导航"
            onClick={() => setMenuOpen(!menuOpen)}
          >
            ☰
          </button>
          <div className="breadcrumb">
            <span>商家中心</span>
            <i>/</i>
            <strong>{titles[view]}</strong>
          </div>
          <div className="top-actions">
            <span
              className={`sync-status ${merchant.remoteStatus}`}
              title={merchant.remoteMessage || "后台数据同步状态"}
            >
              {merchant.remoteStatus === "loading"
                ? "同步中"
                : merchant.remoteStatus === "ready"
                  ? "已同步"
                  : merchant.remoteStatus === "auth"
                    ? "需登录"
                    : merchant.remoteStatus === "local"
                      ? "未连接"
                      : "同步异常"}
            </span>
            <button
              className="icon-button notification"
              title="消息通知"
              onClick={() =>
                notify(
                  pendingShipments
                    ? `你有 ${pendingShipments} 笔待发货订单，请前往订单中心处理。`
                    : "暂无待发货订单。",
                )
              }
            >
              ♢{pendingShipments > 0 && <em>{pendingShipments}</em>}
            </button>
            <button
              className="account"
              onClick={() => notify("账号信息暂未接入，请以网关登录态为准。")}
            >
              <span className="avatar">商</span>
              <span>商家账号</span>
              <small>⌄</small>
            </button>
          </div>
        </header>
        {(merchant.remoteStatus === "local" ||
          merchant.remoteStatus === "error") && (
          <div className="conn-banner" role="alert">
            <strong>未连接</strong>
            <span>
              {merchant.remoteStatus === "local"
                ? "未连接商家管理网关，无法读取服务端数据。"
                : `商家管理网关暂时不可用：${merchant.remoteMessage || "请检查网络连接。"}`}
            </span>
          </div>
        )}
        {view === "overview" && (
          <Overview
            products={merchant.products}
            orders={merchant.orders}
            onNavigate={navigate}
            onNewProduct={() => merchant.setShowProductForm(true)}
            onShip={merchant.beginShip}
            onStock={() => merchant.setShowStockForm(true)}
          />
        )}
        {view === "products" && (
          <Products
            products={merchant.filteredProducts}
            search={merchant.productSearch}
            status={merchant.productStatus}
            onSearch={merchant.setProductSearch}
            onStatus={merchant.setProductStatus}
            onNew={() => merchant.setShowProductForm(true)}
            onEdit={merchant.setEditingProduct}
            onToggleStatus={merchant.toggleProductStatus}
          />
        )}
        {view === "orders" && (
          <Orders
            orders={merchant.filteredOrders}
            filter={merchant.orderFilter}
            search={merchant.orderSearch}
            onSearch={merchant.setOrderSearch}
            onFilter={merchant.setOrderFilter}
            onShip={merchant.beginShip}
            onTrack={merchant.setTrackingOrder}
            onExport={merchant.exportOrders}
          />
        )}
        {view === "inventory" && (
          <Inventory
            products={merchant.products}
            adjustments={merchant.stockAdjustments}
            onAdjust={() => merchant.setShowStockForm(true)}
          />
        )}
        {view === "offers" && (
          <RouteOffers
            offers={merchant.offers}
            products={merchant.products}
            onCreate={merchant.createOffer}
            notify={notify}
          />
        )}
        {view === "finance" && (
          <Finance
            affiliates={merchant.affiliateSettlements}
            onPayAffiliate={merchant.payAffiliate}
            onExportAffiliates={merchant.exportAffiliates}
          />
        )}
      </main>
      {merchant.showProductForm && (
        <ProductDialog
          onClose={merchant.closeProductDialog}
          onSubmit={merchant.saveProduct}
        />
      )}
      {merchant.editingProduct && (
        <ProductDialog
          product={merchant.editingProduct}
          onClose={merchant.closeProductDialog}
          onSubmit={merchant.saveProduct}
        />
      )}
      {merchant.showStockForm && (
        <StockDialog
          products={merchant.products}
          onClose={() => merchant.setShowStockForm(false)}
          onSubmit={merchant.adjustStock}
        />
      )}
      {merchant.shippingOrder && (
        <ShippingDialog
          order={merchant.shippingOrder}
          onClose={() => merchant.setShippingOrder(null)}
          onSubmit={merchant.confirmShip}
        />
      )}
      {merchant.trackingOrder && (
        <TrackingDialog
          order={merchant.trackingOrder}
          onClose={() => merchant.setTrackingOrder(null)}
        />
      )}
      <div id="toast" className={toast ? "visible" : ""}>
        {toast}
      </div>
    </div>
  );
}

createRoot(document.querySelector("#root")!).render(<App />);
