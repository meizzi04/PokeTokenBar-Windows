import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import Dex from "./Dex";
import SettingsView from "./Settings";
import { DICTS, natureName } from "./i18n";
import {
  Companion,
  CompanionEvent,
  EGG,
  I18n,
  ProviderLimits,
  Settings,
  TodayUsage,
  fmt,
  nameOf,
  sprite,
  sum,
  useBackend,
  useI18n,
  useUpdater,
} from "./shared";

const PROVIDER: Record<string, string> = { claude: "Claude Code", codex: "Codex" };

function Progress({ value, max, label }: { value: number; max: number; label: string }) {
  const pct = Math.min(100, (value / max) * 100);
  return (
    <div className="progress">
      <div className="bar"><div style={{ width: `${pct}%` }} /></div>
      <p className="muted">{label} · {fmt.format(value)} / {fmt.format(max)}</p>
    </div>
  );
}

function CompanionCard({ c }: { c: Companion }) {
  const { t, lang } = useI18n();
  const mon = c.active;
  if (!mon) {
    return (
      <section className="card center">
        <img className="sprite" src={EGG} alt="" />
        <h2>{t.egg}</h2>
        <Progress value={c.egg_usage} max={c.egg_threshold} label={t.toHatch} />
      </section>
    );
  }
  const id = mon.path_ids[mon.stage_index];
  const isFinal = mon.stage_index === mon.planned_ids.length - 1;
  return (
    <section className="card center">
      <img className="sprite" src={sprite(id, mon.is_shiny)} alt="" />
      <h2>
        {mon.is_shiny && "✨ "}
        {nameOf(mon.line.names, id, lang)}
      </h2>
      <p className="muted">
        <span className={`badge ${mon.line.rarity}`}>{t.rarity[mon.line.rarity]}</span>{" "}
        {natureName(mon.nature, lang)} {t.natureSuffix}
      </p>
      <div className="evo">
        {mon.planned_ids.map((pid, i) =>
          i <= mon.stage_index ? (
            <img key={pid} src={sprite(pid, mon.is_shiny)} alt={nameOf(mon.line.names, pid, lang)} />
          ) : (
            <span key={pid} className="mystery">?</span>
          ),
        )}
      </div>
      <Progress value={mon.used_at_stage} max={c.threshold ?? 1} label={isFinal ? t.toGraduate : t.toEvolve} />
    </section>
  );
}

function resetIn(iso: string | null, t: (typeof DICTS)["ko"]) {
  if (!iso) return "";
  const mins = Math.max(0, Math.round((Date.parse(iso) - Date.now()) / 60000));
  const d = Math.floor(mins / 1440), h = Math.floor((mins % 1440) / 60), m = mins % 60;
  const text = d > 0 ? `${d}${t.days} ${h}${t.hours}` : h > 0 ? `${h}${t.hours} ${m}${t.minutes}` : `${m}${t.minutes}`;
  return t.resetIn(text);
}

function windowName(key: string, t: (typeof DICTS)["ko"]) {
  if (t.windows[key]) return t.windows[key];
  const [, n, unit] = key.match(/^(\d+)([dhm])$/) ?? [];
  return n ? `${n}${unit === "d" ? t.days : unit === "h" ? t.hours : t.minutes}` : key;
}

function LimitsCard({ all, mode }: { all: ProviderLimits[]; mode: Settings["limit_display"] }) {
  const { t } = useI18n();
  return (
    <section className="card">
      <strong>{t.officialLimits}</strong>
      {all.map((p) => (
        <div key={p.provider} className="limit-provider">
          <p className="muted">{PROVIDER[p.provider] ?? p.provider}</p>
          {p.error && <p className="muted">{t.limitErrors[p.error] ?? p.error}</p>}
          {p.windows.map((w) => {
            const shown = mode === "remaining" ? 100 - w.used_percent : w.used_percent;
            const tone = w.used_percent >= 95 ? "danger" : w.used_percent >= 80 ? "warn" : "";
            return (
              <div key={w.key} className="limit">
                <div className="row">
                  <span>{windowName(w.key, t)}</span>
                  <span>{mode === "remaining" ? t.remaining : t.used} {Math.round(shown)}%</span>
                </div>
                <div className="bar">
                  <div className={tone} style={{ width: `${Math.min(100, Math.max(0, shown))}%` }} />
                </div>
                <p className="muted">{resetIn(w.resets_at, t)}</p>
              </div>
            );
          })}
        </div>
      ))}
    </section>
  );
}

function eventText(e: CompanionEvent, c: Companion | null, t: (typeof DICTS)["ko"], lang: "ko" | "en" | "ja") {
  const names = c?.active?.line.names ?? c?.dex[c.dex.length - 1]?.names ?? {};
  const name = nameOf(names, e.id, lang);
  if (e.kind === "hatched") return e.shiny ? t.shinyHatched(name) : t.hatched(name);
  if (e.kind === "evolved") return t.evolved(name);
  return t.graduated(name);
}

function Home(props: { usage: TodayUsage | null; companion: Companion | null; limits: ProviderLimits[]; settings: Settings }) {
  const { t } = useI18n();
  const { usage, companion, limits } = props;
  return (
    <>
      {companion && <CompanionCard c={companion} />}
      {usage && (
        <section className="card">
          <div className="row">
            <strong>{t.today}</strong>
            <strong>{fmt.format(usage.total)}</strong>
          </div>
          <div className="row muted"><span>Claude Code</span><span>{fmt.format(sum(usage.claude))}</span></div>
          <div className="row muted"><span>Codex</span><span>{fmt.format(sum(usage.codex))}</span></div>
        </section>
      )}
      {limits.length > 0 && <LimitsCard all={limits} mode={props.settings.limit_display} />}
    </>
  );
}

type Tab = "home" | "dex" | "settings";

function App() {
  const { usage, companion, limits, settings, saveSettings } = useBackend();
  const [tab, setTab] = useState<Tab>("home");
  const updater = useUpdater();
  const updateReady = updater.state.status === "available";
  const [toast, setToast] = useState<CompanionEvent | null>(null);

  useEffect(() => {
    const sub = listen<CompanionEvent>("companion-event", (e) => {
      setToast(e.payload);
      setTimeout(() => setToast(null), 6000);
    });
    return () => {
      sub.then((f) => f());
    };
  }, []);

  if (!settings) return null;
  const lang = settings.language;
  const t = DICTS[lang];

  return (
    <I18n.Provider value={{ lang, t }}>
      <nav className="tabs">
        {(["home", "dex", "settings"] as Tab[]).map((k) => (
          <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
            {k === "home" ? t.tabHome : k === "dex" ? t.tabDex : t.tabSettings}
            {k === "settings" && updateReady && <span className="dot" aria-label={t.updates} />}
          </button>
        ))}
        <button className="icon" title={t.refresh} onClick={() => invoke("refresh_now")}>↻</button>
      </nav>
      <main className="container">
        {toast && <div className="toast">{eventText(toast, companion, t, lang)}</div>}
        {tab === "home" && <Home usage={usage} companion={companion} limits={limits} settings={settings} />}
        {tab === "dex" && companion && <Dex c={companion} settings={settings} saveSettings={saveSettings} />}
        {tab === "settings" && companion && (
          <SettingsView c={companion} settings={settings} saveSettings={saveSettings} updater={updater} />
        )}
      </main>
    </I18n.Provider>
  );
}

export default App;
