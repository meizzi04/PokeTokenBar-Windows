import { createContext, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize, PhysicalPosition, Window } from "@tauri-apps/api/window";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { DICTS, Lang, nameLang } from "./i18n";

export type Tokens = { input: number; output: number; cache_write: number; cache_read: number };
export type TodayUsage = { claude: Tokens; codex: Tokens; total: number };
export type Rarity = "common" | "uncommon" | "rare" | "legendary";
export type Names = Record<string, Record<string, string>>;
export type Mon = {
  line: { base_id: number; rarity: Rarity; names: Names };
  path_ids: number[];
  planned_ids: number[];
  stage_index: number;
  used_at_stage: number;
  is_shiny: boolean;
  nature: string;
};
export type DexEntry = {
  base_id: number;
  final_id: number;
  chain: number[];
  rarity: Rarity;
  is_shiny: boolean;
  nature: string;
  names: Names;
  caught_at: string;
};
export type Companion = {
  active: Mon | null;
  egg_usage: number;
  egg_threshold: number;
  threshold: number | null;
  difficulty: number;
  dex: DexEntry[];
};
export type LimitWindow = { key: string; used_percent: number; resets_at: string | null };
export type ProviderLimits = { provider: string; windows: LimitWindow[]; error: string | null };
export type Settings = {
  language: Lang;
  refresh_minutes: number;
  limit_display: "used" | "remaining";
  tray_tokens: boolean;
  tray_limit: boolean;
  pet_enabled: boolean;
  limit_notifications: boolean;
  warn_threshold: number;
  crit_threshold: number;
  companion_notifications: boolean;
  representative: number | null;
};
export type CompanionEvent =
  | { kind: "hatched"; id: number; shiny: boolean }
  | { kind: "evolved"; id: number }
  | { kind: "graduated"; id: number };

const SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
export const sprite = (id: number, shiny = false) =>
  `${SPRITES}/versions/generation-v/black-white/animated/${shiny ? "shiny/" : ""}${id}.gif`;
export const EGG = `${SPRITES}/egg.png`;

export const fmt = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
export const sum = (t: Tokens) => t.input + t.output + t.cache_write + t.cache_read;

export function nameOf(names: Names, id: number, lang: Lang) {
  const n = names[id] ?? {};
  return n[nameLang(lang)] ?? n.en ?? `#${id}`;
}

/** Live backend state: initial fetch plus the events the Rust side emits on every refresh. */
export function useBackend() {
  const [usage, setUsage] = useState<TodayUsage | null>(null);
  const [companion, setCompanion] = useState<Companion | null>(null);
  const [limits, setLimits] = useState<ProviderLimits[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    invoke<TodayUsage>("today_usage").then(setUsage);
    invoke<Companion>("companion").then(setCompanion);
    invoke<ProviderLimits[]>("limits").then(setLimits);
    invoke<Settings>("get_settings").then(setSettings);
    const subs = [
      listen<TodayUsage>("usage", (e) => setUsage(e.payload)),
      listen<Companion>("companion", (e) => setCompanion(e.payload)),
      listen<ProviderLimits[]>("limits", (e) => setLimits(e.payload)),
      listen<Settings>("settings", (e) => setSettings(e.payload)),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  const saveSettings = (next: Settings) => invoke<Settings>("set_settings", { value: next }).then(setSettings);
  return { usage, companion, limits, settings, saveSettings };
}

export const I18n = createContext<{ lang: Lang; t: (typeof DICTS)["ko"] }>({ lang: "ko", t: DICTS.ko });
export const useI18n = () => useContext(I18n);

/** Species the user owns: every graduated chain plus the stages the active Pokémon has reached. */
export type OwnedSpecies = {
  id: number;
  names: Names;
  rarity: Rarity;
  shiny: boolean;
  count: number;
  growing: boolean;
};

export function ownedSpecies(c: Companion): Map<number, OwnedSpecies> {
  const out = new Map<number, OwnedSpecies>();
  const add = (id: number, names: Names, rarity: Rarity, shiny: boolean, growing: boolean) => {
    const s = out.get(id) ?? { id, names, rarity, shiny: false, count: 0, growing: false };
    s.shiny ||= shiny;
    s.growing ||= growing;
    if (!growing) s.count += 1;
    out.set(id, s);
  };
  c.dex.forEach((d) => d.chain.forEach((id) => add(id, d.names, d.rarity, d.is_shiny, false)));
  const a = c.active;
  a?.path_ids.slice(0, a.stage_index + 1).forEach((id) => add(id, a.line.names, a.line.rarity, a.is_shiny, true));
  return out;
}

/** What the pet and tray show: the chosen representative if still owned, else the active Pokémon. */
export function displayed(c: Companion, s: Settings | null): { id: number; shiny: boolean; names: Names } | null {
  const rep = s?.representative != null ? ownedSpecies(c).get(s.representative) : undefined;
  if (rep) return { id: rep.id, shiny: rep.shiny, names: rep.names };
  const a = c.active;
  return a ? { id: a.path_ids[a.stage_index], shiny: a.is_shiny, names: a.line.names } : null;
}

export const PET_SIZE = { min: 48, max: 384, step: 8 };

async function petWindow() {
  return Window.getByLabel("pet");
}

export async function petSize() {
  const w = await petWindow();
  if (!w) return 120;
  const [size, scale] = await Promise.all([w.innerSize(), w.scaleFactor()]);
  return Math.round(size.width / scale);
}

/** Resizes the pet window, keeping its bottom-center (where the sprite stands) in place. */
export async function resizePet(px: number) {
  const w = await petWindow();
  if (!w) return;
  const [pos, size, scale] = await Promise.all([w.outerPosition(), w.outerSize(), w.scaleFactor()]);
  const next = Math.round(px * scale);
  await w.setSize(new LogicalSize(px, px));
  await w.setPosition(new PhysicalPosition(pos.x + Math.round((size.width - next) / 2), pos.y + size.height - next));
}

export type UpdateState =
  | { status: "idle" | "checking" | "none" | "error" }
  | { status: "available"; update: Update }
  | { status: "downloading"; update: Update; progress: number };

/** In-app updater: checks GitHub Releases' latest.json once on open, installs on request. */
export function useUpdater() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });

  async function checkNow() {
    setState({ status: "checking" });
    try {
      const update = await check();
      setState(update ? { status: "available", update } : { status: "none" });
    } catch {
      setState({ status: "error" });
    }
  }

  async function install() {
    if (state.status !== "available") return;
    const { update } = state;
    let total = 0;
    let received = 0;
    setState({ status: "downloading", update, progress: 0 });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          received += e.data.chunkLength;
          setState({ status: "downloading", update, progress: total ? received / total : 0 });
        }
      });
      // On Windows the installer usually closes the app itself; this covers the rest.
      await relaunch();
    } catch {
      setState({ status: "error" });
    }
  }

  useEffect(() => {
    checkNow();
  }, []);
  return { state, checkNow, install };
}
