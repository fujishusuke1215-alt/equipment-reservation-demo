import { useEffect, useMemo, useState } from "react";
import { api, type AuditEvent, type Equipment, type EquipmentInput, type EquipmentStatus, type EquipmentUpdate, type Person, type Reservation } from "./api";
import { canCancel, equipmentTone, localDateTime, nextActions } from "./domain";

type Tab = "catalog" | "reservations" | "equipment" | "audit";

const statusLabels: Record<string, string> = {
  pending: "承認待ち",
  approved: "承認済み",
  checked_out: "貸出中",
  returned: "返却済み",
  rejected: "却下",
  cancelled: "キャンセル",
};
const actionLabels: Record<string, string> = {
  approve: "承認する",
  reject: "却下する",
  checkout: "貸し出す",
  return: "返却を記録",
};
const categoryEmoji: Record<string, string> = {
  Video: "◉",
  Presentation: "▰",
  Computing: "⌘",
  Audio: "♫",
  Networking: "⌁",
  Photography: "◌",
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "処理に失敗しました。";
}

function dateTime(value: string): string {
  return new Intl.DateTimeFormat("ja-JP", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function prettyPeriod(start: string, end: string): string {
  return dateTime(start) + " — " + dateTime(end);
}

export default function App() {
  const [user, setUser] = useState<Person | null>(null);
  const [equipment, setEquipment] = useState<Equipment[]>([]);
  const [reservations, setReservations] = useState<Reservation[]>([]);
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
  const [tab, setTab] = useState<Tab>("catalog");
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("すべて");
  const [booking, setBooking] = useState<Equipment | null>(null);
  const [booting, setBooting] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  async function loadData(activeUser: Person | null) {
    setBusy(true);
    try {
      const items = await api.equipment();
      setEquipment(items);
      if (activeUser) {
        const [itemsReservations, events] = await Promise.all([
          api.reservations(activeUser.role === "admin"),
          activeUser.role === "admin" ? api.audit() : Promise.resolve([]),
        ]);
        setReservations(itemsReservations);
        setAuditEvents(events);
      } else {
        setReservations([]);
        setAuditEvents([]);
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void (async () => {
      try {
        const session = await api.session();
        setUser(session.user);
        await loadData(session.user);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setBooting(false);
      }
    })();
  }, []);

  async function login(role: "member" | "admin") {
    setError("");
    setNotice("");
    setBusy(true);
    try {
      const response = await api.login(role);
      setUser(response.user);
      setTab("catalog");
      await loadData(response.user);
      setNotice(role === "admin" ? "管理者用デモにログインしました。" : "メンバー用デモにログインしました。");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function logout() {
    setBusy(true);
    try {
      await api.logout();
      setUser(null);
      setReservations([]);
      setAuditEvents([]);
      setTab("catalog");
      setNotice("ログアウトしました。");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function withAction(message: string, action: () => Promise<unknown>) {
    setError("");
    setNotice("");
    try {
      await action();
      await loadData(user);
      setNotice(message);
      return true;
    } catch (caught) {
      setError(errorMessage(caught));
      return false;
    }
  }

  const categories = useMemo(
    () => ["すべて", ...Array.from(new Set(equipment.map((item) => item.category)))],
    [equipment],
  );
  const filteredEquipment = useMemo(() => {
    const query = search.trim().toLowerCase();
    return equipment.filter((item) => {
      const matchesCategory = category === "すべて" || item.category === category;
      const haystack = [item.name, item.asset_tag, item.description, item.location].join(" ").toLowerCase();
      return matchesCategory && (!query || haystack.includes(query));
    });
  }, [category, equipment, search]);

  async function reserve(input: {
    equipment_id: string;
    starts_at: string;
    ends_at: string;
    purpose: string;
  }) {
    const succeeded = await withAction("予約を受け付けました。承認後に貸し出されます。", () => api.reserve(input));
    if (succeeded) {
      setBooking(null);
      setTab("reservations");
    }
  }

  async function createEquipment(input: EquipmentInput) {
    await withAction("備品を登録しました。", () => api.createEquipment(input));
  }

  async function updateEquipment(item: Equipment, input: EquipmentUpdate): Promise<boolean> {
    return withAction("備品情報を更新しました。", () => api.updateEquipment(item.id, input));
  }

  if (booting) {
    return <div className="boot-screen"><span className="brand-mark">G</span><p>Gearline を準備しています</p></div>;
  }

  const tabTitle = tab === "catalog"
    ? "備品カタログ"
    : tab === "reservations"
      ? "予約・貸出"
      : tab === "equipment"
        ? "備品管理"
        : "監査ログ";

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <span className="brand-mark">G</span>
          <span><strong>gearline</strong><small>WORKPLACE TOOLS</small></span>
        </div>
        <div className="workspace-switcher">
          <span className="workspace-icon">S</span>
          <span><strong>Studio North</strong><small>チームワークスペース</small></span>
          <span className="chevron">⌄</span>
        </div>
        <div className="nav-label">ワークスペース</div>
        <nav className="side-nav" aria-label="メインメニュー">
          <NavButton active={tab === "catalog"} icon="▦" onClick={() => setTab("catalog")}>備品カタログ</NavButton>
          <NavButton active={tab === "reservations"} icon="▤" onClick={() => setTab("reservations")}>予約・貸出</NavButton>
          {user?.role === "admin" && (
            <>
              <div className="nav-label admin-nav-label">管理者</div>
              <NavButton active={tab === "equipment"} icon="◫" onClick={() => setTab("equipment")}>備品管理</NavButton>
              <NavButton active={tab === "audit"} icon="◷" onClick={() => setTab("audit")}>監査ログ</NavButton>
            </>
          )}
        </nav>
        <div className="sidebar-bottom">
          <div className="help-card"><span className="help-icon">?</span><span><strong>予約のルール</strong><small>時間は終了時刻を含みません</small></span></div>
          {user ? (
            <div className="profile-card">
              <span className={user.role === "admin" ? "avatar avatar-admin" : "avatar"}>{user.display_name.slice(0, 1)}</span>
              <span className="profile-meta"><strong>{user.display_name}</strong><small>{user.role === "admin" ? "管理者" : "メンバー"}</small></span>
              <button className="icon-button" title="ログアウト" onClick={() => void logout()}>⇥</button>
            </div>
          ) : (
            <div className="signed-out-note">閲覧モード · ログインすると予約できます</div>
          )}
        </div>
      </aside>

      <main className="main-area">
        <header className="topbar">
          <div className="breadcrumbs"><span>Studio North</span><span className="crumb-separator">/</span><strong>{tabTitle}</strong></div>
          <div className="topbar-actions">
            <a className="api-link" href="/openapi.yaml" target="_blank" rel="noreferrer">API仕様 <span>↗</span></a>
            {user ? (
              <button className="role-switch" onClick={() => void login(user.role === "admin" ? "member" : "admin")} disabled={busy}>
                <span className="role-switch-dot" />{user.role === "admin" ? "管理者として利用中" : "メンバーとして利用中"} <span>⌄</span>
              </button>
            ) : (
              <div className="login-actions">
                <button className="button button-quiet" onClick={() => void login("member")} disabled={busy}>メンバーで試す</button>
                <button className="button button-dark small-button" onClick={() => void login("admin")} disabled={busy}>管理者で試す</button>
              </div>
            )}
          </div>
        </header>

        <div className="page-content">
          <div className="page-heading">
            <div><div className="eyebrow">ASSET MANAGEMENT · DEMO</div><h1>{tabTitle}</h1>
              <p>{tab === "catalog" ? "必要なものを、必要な時間だけ。" : tab === "reservations" ? "予約から返却まで、進捗をひと目で確認。" : tab === "equipment" ? "チームで共有する備品を管理。" : "誰が、いつ、何を変更したかを記録しています。"}</p>
            </div>
            {tab === "catalog" && <div className="availability-summary"><span className="summary-dot" />{equipment.filter((item) => item.status === "active").length} 点が利用可能</div>}
          </div>

          {(error || notice) && (
            <div className={error ? "flash flash-error" : "flash flash-success"} role={error ? "alert" : "status"}>
              <span>{error ? "!" : "✓"}</span><p>{error || notice}</p>
              <button className="icon-button" onClick={() => { setError(""); setNotice(""); }} aria-label="通知を閉じる">×</button>
            </div>
          )}

          {tab === "catalog" && (
            <>
              <section className="hero-panel">
                <div className="hero-copy">
                  <span className="hero-kicker">MAKE ROOM FOR WHAT MATTERS</span>
                  <h2>必要なものを、<br />スムーズに。</h2>
                  <p>空き状況を確認して、備品を予約。<br />承認から返却まで、ここで完結します。</p>
                  {!user && <button className="button button-white" onClick={() => void login("member")}>デモをはじめる <span>→</span></button>}
                </div>
                <div className="hero-art" aria-hidden="true">
                  <div className="orbit orbit-one" /><div className="orbit orbit-two" />
                  <div className="hero-device device-screen"><span /><i /><b /></div>
                  <div className="hero-device device-lens"><span /></div>
                  <div className="hero-device device-projector"><i /><span /></div>
                  <div className="hero-sticker">READY<br />WHEN<br />YOU ARE</div>
                </div>
                <div className="hero-index">01 <span>/</span> 03</div>
              </section>

              <section className="catalog-section">
                <div className="section-heading">
                  <div><h2>備品を探す</h2><p>用途や名前から、ぴったりの一台を。</p></div>
                  <label className="search-box"><span>⌕</span><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="備品を検索..." aria-label="備品を検索" /><kbd>⌘ K</kbd></label>
                </div>
                <div className="filter-row" role="tablist" aria-label="カテゴリ">
                  {categories.map((item) => <button key={item} className={category === item ? "filter-chip selected" : "filter-chip"} onClick={() => setCategory(item)}>{item}</button>)}
                  <span className="filter-count">{filteredEquipment.length} 件</span>
                </div>
                {busy && equipment.length === 0 ? <div className="empty-state">備品を読み込んでいます…</div> : (
                  <div className="equipment-grid">
                    {filteredEquipment.map((item, index) => (
                      <EquipmentCard key={item.id} item={item} index={index} onBook={() => user ? setBooking(item) : setError("予約するには、メンバーまたは管理者としてデモにログインしてください。")} />
                    ))}
                    {filteredEquipment.length === 0 && <div className="empty-state">該当する備品がありません。検索条件を変えてください。</div>}
                  </div>
                )}
              </section>
              <div className="page-footer"><span>GEARLINE · EQUIPMENT RESERVATION DEMO</span><span>REST API <i /> POSTGRESQL</span></div>
            </>
          )}

          {tab === "reservations" && (
            <section className="content-panel">
              <div className="panel-toolbar">
                <div><h2>{user?.role === "admin" ? "すべての予約" : "マイ予約"}</h2><p>{reservations.length} 件の予約があります</p></div>
                <button className="button button-dark" onClick={() => setTab("catalog")}>＋ 備品を探す</button>
              </div>
              {!user && <div className="inline-login"><span>予約の確認にはログインが必要です。</span><button className="button button-dark small-button" onClick={() => void login("member")}>メンバーで試す</button></div>}
              {user && reservations.length === 0 ? <EmptyMessage title="予約はまだありません" detail="カタログから備品を選んで予約できます。" /> : (
                <div className="reservation-list">{reservations.map((item) => (
                  <ReservationCard
                    key={item.id}
                    item={item}
                    user={user}
                    onTransition={(action) => void withAction("予約の状態を更新しました。", () => api.transition(item.id, action))}
                  />
                ))}</div>
              )}
            </section>
          )}

          {tab === "equipment" && user?.role === "admin" && (
            <AdminEquipmentPanel
              equipment={equipment}
              onCreate={(input) => void createEquipment(input)}
              onUpdate={updateEquipment}
            />
          )}
          {tab === "audit" && user?.role === "admin" && <AuditPanel events={auditEvents} />}
        </div>
      </main>

      {booking && <BookingDialog item={booking} onClose={() => setBooking(null)} onSubmit={(input) => void reserve(input)} />}
    </div>
  );
}

function NavButton(props: { active: boolean; icon: string; onClick: () => void; children: React.ReactNode }) {
  return <button className={props.active ? "nav-item active" : "nav-item"} onClick={props.onClick}><span className="nav-icon">{props.icon}</span>{props.children}{props.active && <span className="nav-active-dot" />}</button>;
}

function EquipmentCard(props: { item: Equipment; index: number; onBook: () => void }) {
  const { item, index, onBook } = props;
  const tone = ["peach", "blue", "lavender", "green", "sand", "pink"][index % 6];
  return (
    <article className="equipment-card">
      <div className={"equipment-visual " + tone}>
        <span className="visual-category">{item.category}</span>
        <span className="visual-symbol">{categoryEmoji[item.category] ?? "◉"}</span>
        <span className="visual-tag">{item.asset_tag}</span>
      </div>
      <div className="equipment-card-body">
        <div className="equipment-card-top"><span className="equipment-category">{item.category}</span>
          <span className={"availability-pill " + (equipmentTone(item.status) === "ready" ? "available" : "paused")}>
            <i />{item.status === "active" ? "利用可能" : item.status === "maintenance" ? "整備中" : "利用停止"}
          </span>
        </div>
        <h3>{item.name}</h3>
        <p className="equipment-description">{item.description || "チームで共有する備品です。"}</p>
        <div className="equipment-card-meta"><span>⌖ {item.location}</span><span>{item.asset_tag}</span></div>
        <button className="card-action" onClick={onBook} disabled={item.status !== "active"}>
          {item.status === "active" ? <>空き状況を確認 <span>↗</span></> : <>予約できません <span>—</span></>}
        </button>
      </div>
    </article>
  );
}

function BookingDialog(props: { item: Equipment; onClose: () => void; onSubmit: (input: { equipment_id: string; starts_at: string; ends_at: string; purpose: string }) => void }) {
  const tomorrow = new Date(Date.now() + 24 * 60 * 60 * 1000);
  tomorrow.setHours(10, 0, 0, 0);
  const later = new Date(tomorrow.getTime() + 2 * 60 * 60 * 1000);
  const [start, setStart] = useState(localDateTime(tomorrow));
  const [end, setEnd] = useState(localDateTime(later));
  const [purpose, setPurpose] = useState("");
  const [busySlots, setBusySlots] = useState<{ starts_at: string; ends_at: string }[]>([]);
  const [checking, setChecking] = useState(false);
  const [availabilityError, setAvailabilityError] = useState("");

  useEffect(() => {
    const startAt = new Date(start);
    const endAt = new Date(end);
    if (!start || !end || startAt >= endAt) {
      setBusySlots([]);
      return;
    }
    let active = true;
    setChecking(true);
    setAvailabilityError("");
    void api.availability(props.item.id, startAt.toISOString(), endAt.toISOString())
      .then((response) => { if (active) setBusySlots(response.busy_slots); })
      .catch((caught: unknown) => { if (active) setAvailabilityError(errorMessage(caught)); })
      .finally(() => { if (active) setChecking(false); });
    return () => { active = false; };
  }, [end, props.item.id, start]);

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    props.onSubmit({
      equipment_id: props.item.id,
      starts_at: new Date(start).toISOString(),
      ends_at: new Date(end).toISOString(),
      purpose: purpose.trim(),
    });
  }

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) props.onClose(); }}>
      <section className="booking-dialog" role="dialog" aria-modal="true" aria-labelledby="booking-title">
        <div className="dialog-heading"><div><div className="eyebrow">NEW RESERVATION</div><h2 id="booking-title">備品を予約</h2></div><button className="icon-button close-button" onClick={props.onClose} aria-label="閉じる">×</button></div>
        <div className="dialog-equipment"><span className="dialog-icon">{categoryEmoji[props.item.category] ?? "◉"}</span><span><strong>{props.item.name}</strong><small>{props.item.asset_tag} · {props.item.location}</small></span></div>
        <form onSubmit={submit} className="form-stack">
          <div className="form-grid">
            <label>開始日時<input type="datetime-local" value={start} min={localDateTime(new Date())} onChange={(event) => setStart(event.target.value)} required /></label>
            <label>返却予定<input type="datetime-local" value={end} min={start} onChange={(event) => setEnd(event.target.value)} required /></label>
          </div>
          <label>利用目的<input value={purpose} onChange={(event) => setPurpose(event.target.value)} minLength={3} maxLength={280} placeholder="例：来週のクライアント提案" required /></label>
          <div className="availability-check">
            <span className={busySlots.length ? "availability-state conflict" : "availability-state"}><i />{checking ? "空き状況を確認中…" : busySlots.length ? "この期間には予約があります" : "選択期間の予約はありません"}</span>
            {busySlots.map((slot) => <small key={slot.starts_at}>{prettyPeriod(slot.starts_at, slot.ends_at)}</small>)}
            {availabilityError && <small className="error-text">{availabilityError}</small>}
          </div>
          <div className="dialog-note">予約は承認待ちとして登録されます。同じ備品の重なる時間帯は予約できません。</div>
          <div className="dialog-actions"><button type="button" className="button button-quiet" onClick={props.onClose}>キャンセル</button><button className="button button-dark" disabled={checking || busySlots.length > 0 || !purpose.trim()}>予約を申請 <span>→</span></button></div>
        </form>
      </section>
    </div>
  );
}

function ReservationCard(props: { item: Reservation; user: Person | null; onTransition: (action: string) => void }) {
  const { item, user } = props;
  const actions = user ? nextActions(item.status, user.role) : [];
  const owner = user?.id === item.user_id;
  const canUserCancel = user ? canCancel(item.status, user.role, owner ?? false) : false;
  return (
    <article className="reservation-card">
      <div className="reservation-date-block"><strong>{new Intl.DateTimeFormat("ja-JP", { month: "short", day: "numeric" }).format(new Date(item.starts_at))}</strong><small>{new Intl.DateTimeFormat("ja-JP", { weekday: "short" }).format(new Date(item.starts_at))}</small></div>
      <div className="reservation-main">
        <div className="reservation-title-row"><div><span className="asset-tag">{item.asset_tag}</span><h3>{item.equipment_name}</h3></div><span className={"status-badge status-" + item.status}><i />{statusLabels[item.status]}</span></div>
        <p className="reservation-purpose">{item.purpose}</p>
        <div className="reservation-meta"><span>◷ {prettyPeriod(item.starts_at, item.ends_at)}</span>{user?.role === "admin" && <span>◉ {item.requester_name}</span>}</div>
        {(actions.length > 0 || canUserCancel) && <div className="reservation-actions">
          {actions.map((action) => <button key={action} className={action === "reject" ? "button button-outline-danger small-button" : "button button-outline small-button"} onClick={() => props.onTransition(action)}>{actionLabels[action]}</button>)}
          {canUserCancel && <button className="button button-quiet small-button" onClick={() => props.onTransition("cancel")}>キャンセル</button>}
        </div>}
      </div>
    </article>
  );
}

function AdminEquipmentPanel(props: {
  equipment: Equipment[];
  onCreate: (input: EquipmentInput) => void;
  onUpdate: (item: Equipment, input: EquipmentUpdate) => Promise<boolean>;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    props.onCreate({
      asset_tag: String(form.get("asset_tag")),
      name: String(form.get("name")),
      category: String(form.get("category")),
      description: String(form.get("description")),
      location: String(form.get("location")),
    });
    event.currentTarget.reset();
  }
  function submitEdit(event: React.FormEvent<HTMLFormElement>, item: Equipment) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const input: EquipmentUpdate = {
      asset_tag: String(form.get("asset_tag")),
      name: String(form.get("name")),
      category: String(form.get("category")),
      description: String(form.get("description")),
      location: String(form.get("location")),
      status: String(form.get("status")) as EquipmentStatus,
    };
    void props.onUpdate(item, input).then((saved) => {
      if (saved) setEditingId(null);
    });
  }
  return (
    <section className="admin-layout">
      <div className="content-panel create-panel">
        <div className="panel-toolbar"><div><span className="eyebrow">NEW ASSET</span><h2>備品を登録</h2><p>共有備品をカタログに追加します。</p></div><span className="panel-icon">＋</span></div>
        <form className="form-stack" onSubmit={submit}>
          <div className="form-grid"><label>備品番号<input name="asset_tag" placeholder="例：AV-041" required minLength={3} maxLength={32} /></label><label>カテゴリ<input name="category" placeholder="例：Audio" required minLength={2} maxLength={60} /></label></div>
          <label>備品名<input name="name" placeholder="例：ポータブルスピーカー" required minLength={2} maxLength={120} /></label>
          <label>保管場所<input name="location" placeholder="例：Equipment Cabinet 2F" required minLength={2} maxLength={100} /></label>
          <label>説明 <span className="optional-label">任意</span><textarea name="description" rows={3} maxLength={1000} placeholder="利用者向けのメモ" /></label>
          <button className="button button-dark">備品を登録 <span>→</span></button>
        </form>
      </div>
      <div className="content-panel inventory-panel">
        <div className="panel-toolbar"><div><span className="eyebrow">INVENTORY</span><h2>登録済みの備品</h2><p>{props.equipment.length} 点を管理中</p></div></div>
        <div className="inventory-list">
          {props.equipment.map((item) => <div className="inventory-item" key={item.id}>
            {editingId === item.id ? (
              <form className="inventory-edit-form" onSubmit={(event) => submitEdit(event, item)}>
                <div className="form-grid">
                  <label>備品番号<input name="asset_tag" defaultValue={item.asset_tag} required minLength={3} maxLength={32} /></label>
                  <label>カテゴリ<input name="category" defaultValue={item.category} required minLength={2} maxLength={60} /></label>
                </div>
                <label>備品名<input name="name" defaultValue={item.name} required minLength={2} maxLength={120} /></label>
                <div className="form-grid">
                  <label>保管場所<input name="location" defaultValue={item.location} required minLength={2} maxLength={100} /></label>
                  <label>状態<select name="status" defaultValue={item.status}><option value="active">利用可能</option><option value="maintenance">整備中</option><option value="retired">利用停止</option></select></label>
                </div>
                <label>説明<textarea name="description" defaultValue={item.description} rows={2} maxLength={1000} /></label>
                <div className="inventory-edit-actions"><button type="button" className="button button-quiet small-button" onClick={() => setEditingId(null)}>キャンセル</button><button className="button button-dark small-button">変更を保存</button></div>
              </form>
            ) : (
              <div className="inventory-row">
                <span className="mini-asset-icon">{categoryEmoji[item.category] ?? "◉"}</span>
                <span className="inventory-copy"><strong>{item.name}</strong><small>{item.asset_tag} · {item.location}</small></span>
                <span className={"inventory-status " + item.status}>{item.status === "active" ? "利用可能" : item.status === "maintenance" ? "整備中" : "利用停止"}</span>
                <button className="button button-outline small-button" onClick={() => setEditingId(item.id)}>編集</button>
              </div>
            )}
          </div>)}
        </div>
        <div className="panel-footnote">未完了の予約がある備品は、利用停止にできません。</div>
      </div>
    </section>
  );
}

function AuditPanel({ events }: { events: AuditEvent[] }) {
  return (
    <section className="content-panel audit-panel">
      <div className="panel-toolbar"><div><span className="eyebrow">ACTIVITY HISTORY</span><h2>最近の変更</h2><p>新しい順に100件を表示しています。</p></div><span className="audit-count">{events.length} EVENTS</span></div>
      {events.length === 0 ? <EmptyMessage title="監査イベントはまだありません" detail="予約や備品の変更がここに記録されます。" /> : (
        <div className="audit-list">{events.map((event) => (
          <article className="audit-row" key={event.id}>
            <span className="audit-marker">{event.entity_type === "reservation" ? "↗" : "◫"}</span>
            <div className="audit-copy"><strong>{event.actor_name}<span>{event.action === "reservation.status_changed" ? "予約の状態を変更" : event.action === "reservation.created" ? "予約を作成" : event.action === "equipment.created" ? "備品を登録" : "備品を更新"}</span></strong>
              <small>{event.entity_type === "reservation" ? "Reservation" : "Equipment"} · {event.entity_id}</small>
              {event.from_state && event.to_state && <div className="audit-transition">{statusLabels[event.from_state] ?? event.from_state}<span>→</span>{statusLabels[event.to_state] ?? event.to_state}</div>}
            </div>
            <time>{dateTime(event.occurred_at)}</time>
          </article>
        ))}</div>
      )}
    </section>
  );
}

function EmptyMessage(props: { title: string; detail: string }) {
  return <div className="empty-message"><span className="empty-illustration">◌</span><strong>{props.title}</strong><p>{props.detail}</p></div>;
}
