import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Audiences } from "./features/audiences";
import { CampaignDialog, Campaigns } from "./features/campaigns";
import { Creatives } from "./features/creatives";
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
          <span className="workspace-logo">N</span>
          <span>
            <strong>Northland</strong>
            <small>广告账户 · 482019</small>
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
              notify("账单中心暂无待处理账单。");
            }}
          >
            <span>▣</span>账单与付款
          </a>
          <a
            className="nav-item"
            href="#"
            onClick={(event) => {
              event.preventDefault();
              notify("账户设置已保存。");
            }}
          >
            <span>⚙</span>账户设置
          </a>
          <div className="account-status">
            <i />
            <span>
              <strong>账户状态正常</strong>
              <small>余额 ¥24,580.00</small>
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
            {platform.remoteStatus !== "local" && (
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
                      : "同步异常"}
              </span>
            )}
            <button
              className="help-button"
              onClick={() => notify("请联系商家支持获取投放帮助。")}
            >
              ?
            </button>
            <button
              className="icon-button"
              onClick={() => notify("暂无新的审核通知。")}
            >
              ♧
            </button>
            <button className="user-button">
              <span className="avatar">陈</span>
              <span>陈铭</span>
            </button>
          </div>
        </header>
        {view === "overview" && (
          <Overview
            campaigns={platform.campaigns}
            guardrails={platform.guardrails}
            go={go}
            open={() => platform.setDialog(true)}
            onApplyBudget={(name) => {
              platform.setCampaigns((current) =>
                current.map((campaign) =>
                  campaign.name === name
                    ? { ...campaign, budget: "¥1,400" }
                    : campaign,
                ),
              );
              notify(`建议已应用：${name} 日预算调整为 ¥1,400。`);
            }}
            notify={notify}
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
            review={platform.submitReview}
            approve={platform.approveCampaign}
            returnToDraft={platform.returnCampaignToDraft}
            onEdit={platform.setEditing}
            notify={notify}
          />
        )}
        {view === "ads" && (
          <Creatives
            creatives={platform.creatives}
            bindings={platform.activeBindings}
            addCreative={platform.addCreative}
            reviewCreative={(name, creativeStatus) => {
              platform.setCreatives((current) =>
                current.map((creative) =>
                  creative.name === name && creative.status === "审核中"
                    ? { ...creative, status: creativeStatus, updated: "刚刚" }
                    : creative,
                ),
              );
              notify(
                creativeStatus === "已通过"
                  ? "素材已审核通过，可供关联活动使用。"
                  : "素材已退回修改，请补充与场景装备的关联说明。",
              );
            }}
            onRemove={(name) => {
              platform.setCreatives((current) =>
                current.filter((creative) => creative.name !== name),
              );
              notify("素材已从审核队列移除。");
            }}
            notify={notify}
          />
        )}
        {view === "audiences" && (
          <Audiences
            scenes={platform.scenes}
            campaigns={platform.campaigns}
            guardrails={platform.guardrails}
            toggle={(id) =>
              platform.setScenes((current) =>
                current.map((scene) =>
                  scene.id === id
                    ? { ...scene, enabled: !scene.enabled }
                    : scene,
                ),
              )
            }
            onAdd={(scene) => platform.setScenes((current) => [scene, ...current])}
            onRemove={(id) =>
              platform.setScenes((current) =>
                current.filter((scene) => scene.id !== id),
              )
            }
            onSaveCap={(cap) => {
              void platform.saveUserDailyCap(cap);
            }}
            notify={notify}
          />
        )}
        {view === "reports" && (
          <Reports campaigns={platform.campaigns} notify={notify} />
        )}
      </main>
      {(platform.dialog || platform.editing) && (
        <CampaignDialog
          campaign={platform.editing || undefined}
          creatives={platform.creatives}
          bindings={
            platform.editing &&
            !platform.activeBindings.some(
              (binding) => binding.id === platform.editing?.binding.id,
            )
              ? [platform.editing.binding, ...platform.activeBindings]
              : platform.activeBindings
          }
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
