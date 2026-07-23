import * as React from "react";
import {
  CheckCircle2,
  Eye,
  EyeOff,
  LogIn,
  Network,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { callBackend } from "../backend";
import type { ProviderProfile, ProviderSnapshot } from "../types";

function modeLabel(snapshot: ProviderSnapshot | null): string {
  if (!snapshot) return "加载中";
  if (snapshot.externalProvider) return "纯API态（外部管理）";
  switch (snapshot.mode) {
    case "hybrid":
      return "中转态";
    case "api":
      return "纯API态";
    case "official":
      return "登录态";
    default:
      return snapshot.chatgptAuthenticated ? "登录态（推断）" : "未知";
  }
}

export function ProviderView({
  provider,
  onRefresh,
  onMessage,
  onProgress,
}: {
  provider: ProviderSnapshot | null;
  onRefresh: () => Promise<unknown>;
  onMessage: (msg: string) => void;
  onProgress: (msg: string) => void;
}) {
  const profiles = provider?.profiles ?? [];
  const activeProfileId = provider?.activeProfileId ?? "";
  const [editingId, setEditingId] = React.useState("");
  const [profileName, setProfileName] = React.useState("");
  const [baseUrl, setBaseUrl] = React.useState("");
  const [bearerToken, setBearerToken] = React.useState("");
  const [upstreamProtocol, setUpstreamProtocol] = React.useState("responses");
  const [showToken, setShowToken] = React.useState(false);
  const [isCreating, setIsCreating] = React.useState(false);
  const [pendingAction, setPendingAction] = React.useState("");
  const [pendingDeleteId, setPendingDeleteId] = React.useState("");
  const [switchConfirm, setSwitchConfirm] = React.useState<string | null>(null);
  const [error, setError] = React.useState("");

  const editingProfile = profiles.find((p) => p.id === editingId) ?? null;

  React.useEffect(() => {
    if (isCreating) {
      setProfileName("");
      setBaseUrl("");
      setBearerToken("");
      setUpstreamProtocol("responses");
      return;
    }
    if (!editingProfile) return;
    setProfileName(editingProfile.name);
    setBaseUrl(editingProfile.baseUrl);
    setBearerToken(editingProfile.bearerToken);
    setUpstreamProtocol(editingProfile.upstreamProtocol ?? "responses");
  }, [editingProfile?.id, isCreating]);

  const runAction = async (action: string, fn: () => Promise<unknown>) => {
    if (pendingAction) return;
    setPendingAction(action);
    setError("");
    try {
      await fn();
      await onRefresh();
    } catch (e) {
      const msg = String(e);
      setError(msg);
      onMessage(msg);
    } finally {
      setPendingAction("");
    }
  };

  const saveProfile = () => {
    if (!profileName.trim() || !baseUrl.trim()) {
      setError("名称和 Base URL 不能为空");
      return;
    }
    runAction("save", () =>
      callBackend("save_provider_profile", {
        request: {
          id: isCreating ? `${Date.now()}-${Math.random().toString(36).slice(2, 8)}` : editingId,
          name: profileName.trim(),
          baseUrl: baseUrl.trim(),
          bearerToken: bearerToken.trim(),
          upstreamProtocol,
        },
      })
    ).then(() => {
      setIsCreating(false);
      setEditingId("");
    });
  };

  const deleteProfile = (id: string) => {
    runAction("delete", () => callBackend("delete_provider_profile", { profileId: id })).then(() => {
      setPendingDeleteId("");
    });
  };

  const activateProfile = (id: string) => {
    runAction("activate", () => callBackend("activate_provider_profile", { profileId: id }));
  };

  const switchMode = (mode: string, profileId?: string) => {
    runAction("switch", () =>
      callBackend("switch_provider_mode", {
        request: {
          mode,
          profileId: profileId ?? "",
          baseUrl: "",
          apiKey: "",
          upstreamProtocol: "",
        },
      })
    ).then(() => setSwitchConfirm(null));
  };

  const handleSwitch = (mode: string) => {
    if (switchConfirm === mode) {
      // For hybrid/api, use active profile
      const pid = activeProfileId || "";
      switchMode(mode, pid);
    } else {
      setSwitchConfirm(mode);
    }
  };

  const cancelSwitch = () => {
    setSwitchConfirm(null);
  };

  const isLoading = !!pendingAction;

  return (
    <div className="taskStack">
      {/* 当前状态卡 */}
      <section className="taskPanel primaryTask">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <Network size={16} />
              </span>
              <h2>模型通道</h2>
            </div>
            <p className="taskSummary">
              显式切换 Codex 的运行态：登录态 / 中转态 / 纯API态。切换前会校验前置条件，不满足时报清晰错误。
            </p>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <div className="metric">
            <dt>当前模式</dt>
            <dd>{modeLabel(provider)}</dd>
          </div>
          <div className="metric">
            <dt>ChatGPT 登录</dt>
            <dd>
              {provider?.chatgptAuthenticated
                ? provider.chatgptAccountLabel ?? "已登录"
                : "未登录"}
            </dd>
          </div>
          <div className="metric">
            <dt>官方快照</dt>
            <dd>{provider?.officialSnapshotAvailable ? "可用" : "无"}</dd>
          </div>
          <div className="metric">
            <dt>管理权</dt>
            <dd>{provider?.ownedByCodexPilot ? "CodexPilot" : provider?.externalProvider ? "外部" : "—"}</dd>
          </div>
        </dl>
      </section>

      {/* 三态切换 */}
      <section className="taskPanel">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <LogIn size={16} />
              </span>
              <h2>切换运行态</h2>
            </div>
          </div>
        </div>
        <div className="providerModeSwitches">
          <div className="providerModeRow">
            <div className="providerModeInfo">
              <span className="providerModeName">登录态</span>
              <span className="providerModeDesc">恢复官方基线配置，ChatGPT OAuth 直连。</span>
            </div>
            {switchConfirm === "official" ? (
              <div className="confirmGroup">
                <button
                  className="primary"
                  disabled={isLoading || !provider?.chatgptAuthenticated && !provider?.officialSnapshotAvailable}
                  onClick={() => switchMode("official")}
                  type="button"
                >
                  确认切换
                </button>
                <button className="secondary" onClick={cancelSwitch} type="button">
                  取消
                </button>
              </div>
            ) : (
              <button
                className="primary"
                disabled={isLoading || provider?.mode === "official"}
                onClick={() => handleSwitch("official")}
                type="button"
              >
                {isLoading && pendingAction === "switch" ? "切换中" : provider?.mode === "official" ? "当前" : "切换到登录态"}
              </button>
            )}
          </div>
          <div className="providerModeRow">
            <div className="providerModeInfo">
              <span className="providerModeName">中转态</span>
              <span className="providerModeDesc">本地协议代理拦截模型请求，同时保留 ChatGPT 登录。</span>
            </div>
            {switchConfirm === "hybrid" ? (
              <div className="confirmGroup">
                <button
                  className="primary"
                  disabled={isLoading || !provider?.chatgptAuthenticated}
                  onClick={() => switchMode("hybrid", activeProfileId)}
                  type="button"
                >
                  确认切换
                </button>
                <button className="secondary" onClick={cancelSwitch} type="button">
                  取消
                </button>
              </div>
            ) : (
              <button
                className="primary"
                disabled={isLoading || provider?.mode === "hybrid" || !provider?.chatgptAuthenticated}
                onClick={() => handleSwitch("hybrid")}
                type="button"
              >
                {isLoading && pendingAction === "switch" ? "切换中" : provider?.mode === "hybrid" ? "当前" : "切换到中转态"}
              </button>
            )}
          </div>
          <div className="providerModeRow">
            <div className="providerModeInfo">
              <span className="providerModeName">纯API态</span>
              <span className="providerModeDesc">API Key 直连，不保留 ChatGPT 登录。</span>
            </div>
            {switchConfirm === "api" ? (
              <div className="confirmGroup">
                <button
                  className="primary"
                  disabled={isLoading}
                  onClick={() => switchMode("api", activeProfileId)}
                  type="button"
                >
                  确认切换
                </button>
                <button className="secondary" onClick={cancelSwitch} type="button">
                  取消
                </button>
              </div>
            ) : (
              <button
                className="primary"
                disabled={isLoading || provider?.mode === "api"}
                onClick={() => handleSwitch("api")}
                type="button"
              >
                {isLoading && pendingAction === "switch" ? "切换中" : provider?.mode === "api" ? "当前" : "切换到纯API态"}
              </button>
            )}
          </div>
        </div>
        {switchConfirm && (
          <div className="taskFooter">
            <span>点击"确认切换"以执行切换到 <strong>{switchConfirm === "official" ? "登录态" : switchConfirm === "hybrid" ? "中转态" : "纯API态"}</strong>。</span>
          </div>
        )}
      </section>

      {/* 中转 Profile 管理 */}
      <section className="taskPanel">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <Network size={16} />
              </span>
              <h2>中转 Profile 管理</h2>
            </div>
            <p className="taskSummary">
              Profile 记录一个中转端点的连接信息。中转态和纯API态都可选择 profile 作为目标。
            </p>
          </div>
        </div>

        {(isCreating || editingId) && (
          <div className="profileEditForm">
            <div className="formField">
              <label>名称</label>
              <input
                type="text"
                value={profileName}
                onChange={(e) => setProfileName(e.target.value)}
                placeholder="我的中转"
              />
            </div>
            <div className="formField">
              <label>Base URL</label>
              <input
                type="text"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                placeholder="https://relay.example.com/v1"
              />
            </div>
            <div className="formField">
              <label>Bearer Token / API Key</label>
              <div className="inputWithToggle">
                <input
                  type={showToken ? "text" : "password"}
                  value={bearerToken}
                  onChange={(e) => setBearerToken(e.target.value)}
                  placeholder="sk-..."
                />
                <button
                  className="iconButton secondary"
                  onClick={() => setShowToken((s) => !s)}
                  type="button"
                  title={showToken ? "隐藏" : "显示"}
                >
                  {showToken ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
            </div>
            <div className="formField">
              <label>上游协议</label>
              <select
                value={upstreamProtocol}
                onChange={(e) => setUpstreamProtocol(e.target.value)}
              >
                <option value="responses">Responses（直连）</option>
                <option value="chatCompletions">Chat Completions</option>
                <option value="anthropicMessages">Anthropic Messages</option>
              </select>
            </div>
            <div className="formActions">
              <button className="primary" disabled={isLoading} onClick={saveProfile} type="button">
                {isLoading && pendingAction === "save" ? "保存中" : "保存"}
              </button>
              <button
                className="secondary"
                onClick={() => {
                  setIsCreating(false);
                  setEditingId("");
                }}
                type="button"
              >
                取消
              </button>
            </div>
          </div>
        )}

        {!isCreating && !editingId && (
          <div className="profileActions">
            <button
              className="primary"
              onClick={() => setIsCreating(true)}
              type="button"
            >
              <Plus size={14} />
              新建 Profile
            </button>
          </div>
        )}

        <div className="profileList">
          {profiles.length === 0 && !isCreating && (
            <p className="emptyHint">暂无中转 Profile。新建一个来开始。</p>
          )}
          {profiles.map((profile) => (
            <div
              key={profile.id}
              className={`profileRow ${profile.id === activeProfileId ? "active" : ""}`}
            >
              <div className="profileInfo">
                <span className="profileName">{profile.name}</span>
                <span className="profileUrl">{profile.baseUrl}</span>
                <span className="profileProtocol">{profile.upstreamProtocol}</span>
              </div>
              <div className="profileRowActions">
                {pendingDeleteId === profile.id ? (
                  <div className="confirmGroup">
                    <button
                      className="danger"
                      disabled={isLoading}
                      onClick={() => deleteProfile(profile.id)}
                      type="button"
                    >
                      确认删除
                    </button>
                    <button className="secondary" onClick={() => setPendingDeleteId("")} type="button">
                      取消
                    </button>
                  </div>
                ) : (
                  <>
                    {profile.id !== activeProfileId && (
                      <button
                        className="secondary"
                        disabled={isLoading}
                        onClick={() => activateProfile(profile.id)}
                        type="button"
                      >
                        <CheckCircle2 size={12} />
                        激活
                      </button>
                    )}
                    <button
                      className="secondary"
                      disabled={isLoading}
                      onClick={() => setEditingId(profile.id)}
                      type="button"
                    >
                      编辑
                    </button>
                    <button
                      className="secondary"
                      disabled={isLoading}
                      onClick={() => setPendingDeleteId(profile.id)}
                      type="button"
                    >
                      <Trash2 size={12} />
                    </button>
                  </>
                )}
              </div>
            </div>
          ))}
        </div>
      </section>

      {error && (
        <div className="taskPanel errorPanel">
          <div className="taskHeader">
            <span className="errorText">{error}</span>
            <button className="secondary" onClick={() => setError("")} type="button">
              <X size={14} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
