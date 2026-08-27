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
import { downloadCsv } from "./lib/storage";
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
  const exportOverview = () =>
    downloadCsv(
      `bookway-store-report-${new Date().toISOString().slice(0, 10)}.csv`,
      ["指标", "数值"],
      [
        ["今日成交额", "¥12,680.00"],
        ["支付订单", "86"],
        ["访客数", "2,438"],
        ["转化率", "3.53%"],
      ],
    );
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
          <span className="store-avatar">青</span>
          <span>
            <strong>青云户外</strong>
            <small>旗舰店</small>
          </span>
          <button
            className="icon-button"
            title="切换店铺"
            onClick={() => notify("当前店铺为青云户外。")}
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
              notify("店铺设置已打开，当前账号具备管理员权限。");
            }}
          >
            <span>⚙</span>店铺设置
          </a>
          <div className="help">
            <strong>需要帮助？</strong>
            <span>商家支持 · 09:00-21:00</span>
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
            {merchant.remoteStatus !== "local" && (
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
                      : "同步异常"}
              </span>
            )}
            <button
              className="icon-button notification"
              title="消息通知"
              onClick={() =>
                notify("你有 3 条待发货订单提醒，请前往订单中心处理。")
              }
            >
              ♢<em>3</em>
            </button>
            <button
              className="account"
              onClick={() => notify("当前登录账号：林青（店铺管理员）。")}
            >
              <span className="avatar">林</span>
              <span>林青</span>
              <small>⌄</small>
            </button>
          </div>
        </header>
        {view === "overview" && (
          <Overview
            products={merchant.products}
            orders={merchant.orders}
            onNavigate={navigate}
            onNewProduct={() => merchant.setShowProductForm(true)}
            onShip={merchant.beginShip}
            onStock={() => merchant.setShowStockForm(true)}
            onExport={exportOverview}
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
            onToggle={merchant.toggleOffer}
            onCreate={merchant.createOffer}
            onRemove={merchant.removeOffer}
          />
        )}
        {view === "finance" && (
          <Finance
            affiliates={merchant.affiliateSettlements}
            affiliateRules={merchant.affiliateRules}
            onPayAffiliate={merchant.payAffiliate}
            onExportAffiliates={merchant.exportAffiliates}
            onCreateAffiliateRule={merchant.addAffiliateRule}
            onToggleAffiliateRule={merchant.toggleAffiliateRule}
            onRemoveAffiliateRule={merchant.removeAffiliateRule}
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
