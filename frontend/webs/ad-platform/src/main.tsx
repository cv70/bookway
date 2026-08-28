import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Audiences } from "./features/audiences";
import { CampaignDialog, Campaigns } from "./features/campaigns";
import { Overview } from "./features/overview";
import { Reports } from "./features/reports";
import { useAdPlatform } from "./features/useAdPlatform";
import { nav, titles, View } from "./domain";
import "../styles.css";
import "./modal.css";
import "./enhancements.css";

function App() {
  const [view, setView] = useState<View>(
    (location.hash.slice(1) as View) || "overview",
  );
  const [menu, setMenu] = useState(false);
  const [toast, setToast] = useState("");
  const notify = (message: string) => {
    setToast(message);
    window.setTimeout(() => setToast(""), 2500);
  };
  const platform = useAdPlatform(notify);
  const go = (next: View) => {
    setView(next);
    setMenu(false);
    history.replaceState(null, "", `#${next}`);
  };
  const statusLabel =
    platform.remoteStatus === "loading"
      ? "同步中"
      : platform.remoteStatus === "ready"
        ? "已连接网关"
        : platform.remoteStatus === "auth"
          ? "需登录"
          : platform.remoteStatus === "local"
            ? "未连接网关"
            : "网关异常";
  return (
    <div className="app-shell">
      <aside className={`sidebar ${menu ? "open" : ""}`}>
        <a className="brand" href="#overview" onClick={() => go("overview")}>
          <span className="brand-mark">B</span>
          <span>
            Bookway <b>Ads</b>
          </span>
        </a>
        <div className="workspace">
          <span className="workspace-logo">广</span>
          <span>
            <strong>广告主账户</strong>
            <small>Bookway Ads</small>
          </span>
        </div>
        <nav>
          {nav.map(([value, icon, label]) => (
            <a
              className={`nav-item ${view === value ? "active" : ""}`}
              href={`#${value}`}
              key={value}
              onClick={(event) => {
                event.preventDefault();
                go(value);
              }}
            >
              <span>{icon}</span>
              {label}
            </a>
          ))}
        </nav>
        <div className="sidebar-footer">
          <a
            className="nav-item"
            href="#"
            onClick={(event) => {
              event.preventDefault();
              notify("账单中心暂未接入。");
            }}
          >
            <span>▣</span>账单与付款
          </a>
          <a
            className="nav-item"
            href="#"
            onClick={(event) => {
              event.preventDefault();
              notify("账户设置暂未接入。");
            }}
          >
            <span>⚙</span>账户设置
          </a>
          <div
            className={`account-status ${platform.remoteStatus === "ready" ? "" : "alert"}`}
            title={platform.remoteMessage || "广告管理网关连接状态"}
          >
            <i />
            <span>
              <strong>{statusLabel}</strong>
              <small>广告管理网关</small>
            </span>
          </div>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <button
            className="mobile-menu icon-button"
            onClick={() => setMenu(!menu)}
          >
            ☰
          </button>
          <div className="breadcrumb">
            <span>广告平台</span>
            <i>/</i>
            <strong>{titles[view]}</strong>
          </div>
          <div className="top-actions">
            <span
              className={`sync-status ${platform.remoteStatus}`}
              title={platform.remoteMessage || "广告后台同步状态"}
            >
              {platform.remoteStatus === "loading"
                ? "同步中"
                : platform.remoteStatus === "ready"
                  ? "已同步"
                  : platform.remoteStatus === "auth"
                    ? "需登录"
                    : platform.remoteStatus === "local"
                      ? "未连接"
                      : "同步异常"}
            </span>
            <button
              className="help-button"
              onClick={() => notify("请联系商家支持获取投放帮助。")}
            >
              ?
            </button>
            <button
              className="icon-button"
              onClick={() => notify("通知中心暂未接入。")}
            >
              ♧
            </button>
            <button className="user-button">
              <span className="avatar">广</span>
              <span>广告主账号</span>
            </button>
          </div>
        </header>
        {(platform.remoteStatus === "local" ||
          platform.remoteStatus === "error") && (
          <div className="conn-banner" role="alert">
            <strong>未连接</strong>
            <span>
              {platform.remoteStatus === "local"
                ? "未连接广告管理网关，无法读取服务端广告账本。"
                : `广告管理网关暂时不可用：${platform.remoteMessage || "请检查网络连接。"}`}
            </span>
          </div>
        )}
        {view === "overview" && (
          <Overview
            campaigns={platform.campaigns}
            guardrails={platform.guardrails}
            go={go}
            open={() => platform.setDialog(true)}
          />
        )}
        {view === "campaigns" && (
          <Campaigns
            campaigns={platform.filteredCampaigns}
            query={platform.query}
            status={platform.status}
            setQuery={platform.setQuery}
            setStatus={platform.setStatus}
            open={() => platform.setDialog(true)}
            toggle={platform.toggleCampaign}
            onEdit={platform.setEditing}
          />
        )}
        {view === "audiences" && (
          <Audiences
            campaigns={platform.campaigns}
            bindings={platform.activeBindings}
            guardrails={platform.guardrails}
            onSaveCap={(cap) => {
              void platform.saveUserDailyCap(cap);
            }}
          />
        )}
        {view === "reports" && (
          <Reports campaigns={platform.campaigns} notify={notify} />
        )}
      </main>
      {(platform.dialog || platform.editing) && (
        <CampaignDialog
          campaign={platform.editing || undefined}
          close={() => {
            platform.setDialog(false);
            platform.setEditing(null);
          }}
          submit={platform.saveCampaign}
        />
      )}
      <div id="toast" className={toast ? "visible" : ""}>
        {toast}
      </div>
    </div>
  );
}

createRoot(document.querySelector("#root")!).render(<App />);
