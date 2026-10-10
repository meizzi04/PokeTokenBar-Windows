import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Tokens = { input: number; output: number; cache_write: number; cache_read: number };
type TodayUsage = { claude: Tokens; codex: Tokens; total: number };
type Rarity = "common" | "uncommon" | "rare" | "legendary";
type Names = Record<string, Record<string, string>>;
type Mon = {
  line: { base_id: number; rarity: Rarity; names: Names };
  path_ids: number[];
  planned_ids: number[];
  stage_index: number;
  used_at_stage: number;
  is_shiny: boolean;
  nature: string;
};
type DexEntry = { final_id: number; chain: number[]; rarity: Rarity; is_shiny: boolean; names: Names };
type Companion = {
  active: Mon | null;
  egg_usage: number;
  egg_threshold: number;
  threshold: number | null;
  dex: DexEntry[];
};
type CompanionEvent =
  | { kind: "hatched"; id: number; shiny: boolean }
  | { kind: "evolved"; id: number }
  | { kind: "graduated"; id: number };

const SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
const sprite = (id: number, shiny = false) =>
  `${SPRITES}/versions/generation-v/black-white/animated/${shiny ? "shiny/" : ""}${id}.gif`;
const EGG = `${SPRITES}/egg.png`;

const RARITY: Record<Rarity, string> = { common: "일반", uncommon: "고급", rare: "희귀", legendary: "전설" };
const NATURE: Record<string, string> = {
  hardy: "노력", lonely: "외로움", brave: "용감", adamant: "고집", naughty: "개구쟁이",
  bold: "대담", docile: "온순", relaxed: "무사태평", impish: "장난꾸러기", lax: "촐랑",
  timid: "겁쟁이", hasty: "성급", serious: "성실", jolly: "명랑", naive: "천진난만",
  modest: "조심", mild: "의젓", quiet: "냉정", bashful: "수줍음", rash: "덜렁",
  calm: "차분", gentle: "얌전", sassy: "건방", careful: "신중", quirky: "변덕",
};

const sum = (t: Tokens) => t.input + t.output + t.cache_write + t.cache_read;
const fmt = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
const nameOf = (names: Names, id: number) => {
  const n = names[id] ?? {};
  return n.ko ?? n.en ?? `#${id}`;
};

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
  const mon = c.active;
  if (!mon) {
    return (
      <section className="card center">
        <img className="sprite" src={EGG} alt="알" />
        <h2>알</h2>
        <Progress value={c.egg_usage} max={c.egg_threshold} label="부화까지" />
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
        {nameOf(mon.line.names, id)}
      </h2>
      <p className="muted">
        <span className={`badge ${mon.line.rarity}`}>{RARITY[mon.line.rarity]}</span> {NATURE[mon.nature]} 성격
      </p>
      <div className="evo">
        {mon.planned_ids.map((pid, i) =>
          i <= mon.stage_index ? (
            <img key={pid} src={sprite(pid, mon.is_shiny)} alt={nameOf(mon.line.names, pid)} />
          ) : (
            <span key={pid} className="mystery">?</span>
          ),
        )}
      </div>
      <Progress
        value={mon.used_at_stage}
        max={c.threshold ?? 1}
        label={isFinal ? "졸업까지" : "진화까지"}
      />
    </section>
  );
}

function eventText(e: CompanionEvent, c: Companion | null) {
  const names = c?.active?.line.names ?? c?.dex[c.dex.length - 1]?.names ?? {};
  const name = nameOf(names, e.id);
  if (e.kind === "hatched") return e.shiny ? `✨ 이로치 ${name} 부화!` : `${name} 부화!`;
  if (e.kind === "evolved") return `${name}(으)로 진화했어요!`;
  return `${name} 졸업! 도감에 등록됐어요`;
}

function App() {
  const [usage, setUsage] = useState<TodayUsage | null>(null);
  const [companion, setCompanion] = useState<Companion | null>(null);
  const [toast, setToast] = useState<CompanionEvent | null>(null);

  useEffect(() => {
    invoke<TodayUsage>("today_usage").then(setUsage);
    invoke<Companion>("companion").then(setCompanion);
    const subs = [
      listen<TodayUsage>("usage", (e) => setUsage(e.payload)),
      listen<Companion>("companion", (e) => setCompanion(e.payload)),
      listen<CompanionEvent>("companion-event", (e) => {
        setToast(e.payload);
        setTimeout(() => setToast(null), 6000);
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  return (
    <main className="container">
      {toast && <div className="toast">{eventText(toast, companion)}</div>}
      {companion && <CompanionCard c={companion} />}

      {usage && (
        <section className="card">
          <div className="row">
            <strong>오늘</strong>
            <strong>{fmt.format(usage.total)}</strong>
          </div>
          <div className="row muted"><span>Claude Code</span><span>{fmt.format(sum(usage.claude))}</span></div>
          <div className="row muted"><span>Codex</span><span>{fmt.format(sum(usage.codex))}</span></div>
        </section>
      )}

      {companion && companion.dex.length > 0 && (
        <section className="card">
          <strong>도감 {companion.dex.length}</strong>
          <div className="dex">
            {companion.dex.map((d, i) => (
              <img key={i} src={sprite(d.final_id, d.is_shiny)} title={nameOf(d.names, d.final_id)} alt="" />
            ))}
          </div>
        </section>
      )}
    </main>
  );
}

export default App;
