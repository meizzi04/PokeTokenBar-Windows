import { ReactNode, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { LANG_LABELS, Lang } from "./i18n";
import {
  Companion,
  PET_SIZE,
  Settings,
  nameOf,
  ownedSpecies,
  petSize,
  resizePet,
  useI18n,
  useUpdater,
} from "./shared";

const INTERVALS = [1, 2, 5, 10, 15, 0];

// Growth multiplier slider is logarithmic like the original: equal ratios take equal distance.
const [D_MIN, D_MAX] = [0.1, 2.0];
const toPosition = (d: number) => Math.log(d / D_MIN) / Math.log(D_MAX / D_MIN);
function fromPosition(p: number) {
  if (Math.abs(p - toPosition(1)) < 0.01) return 1; // snap to default
  const v = D_MIN * Math.pow(D_MAX / D_MIN, p);
  const mag = Math.pow(10, Math.floor(Math.log10(v)) - 1);
  return Math.round(v / mag) * mag; // two significant digits
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="card settings">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div>
        <div>{label}</div>
        {hint && <div className="hint">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

const Toggle = ({ on, set }: { on: boolean; set: (v: boolean) => void }) => (
  <input type="checkbox" className="switch" checked={on} onChange={(e) => set(e.target.checked)} />
);

function UpdateSection({ updater }: { updater: ReturnType<typeof useUpdater> }) {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion);
  }, []);
  const s = updater.state;
  const status =
    s.status === "checking" ? t.checking
    : s.status === "none" ? t.upToDate
    : s.status === "error" ? t.updateError
    : s.status === "available" ? t.updateAvailable(s.update.version)
    : s.status === "downloading" ? t.downloading(Math.round(s.progress * 100))
    : "";
  return (
    <Section title={t.updates}>
      <Row label={t.currentVersion(version)} hint={status}>
        {s.status === "available" ? (
          <button className="primary" onClick={updater.install}>{t.updateNow}</button>
        ) : (
          <button disabled={s.status === "checking" || s.status === "downloading"} onClick={updater.checkNow}>
            {t.checkUpdates}
          </button>
        )}
      </Row>
    </Section>
  );
}

export default function SettingsView(props: {
  c: Companion;
  settings: Settings;
  saveSettings: (s: Settings) => void;
  updater: ReturnType<typeof useUpdater>;
}) {
  const { t, lang } = useI18n();
  const { settings: s, saveSettings } = props;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => saveSettings({ ...s, [k]: v });

  const [autostart, setAutostart] = useState(false);
  const [autostartError, setAutostartError] = useState("");
  const [size, setSize] = useState(120);
  const [difficulty, setDifficulty] = useState(props.c.difficulty);
  useEffect(() => {
    invoke<boolean>("get_autostart").then(setAutostart);
    petSize().then(setSize);
  }, []);
  useEffect(() => setDifficulty(props.c.difficulty), [props.c.difficulty]);

  const rep = s.representative != null ? ownedSpecies(props.c).get(s.representative) : undefined;

  return (
    <>
      <UpdateSection updater={props.updater} />
      <Section title={t.general}>
        <Row label={t.language}>
          <select value={s.language} onChange={(e) => set("language", e.target.value as Lang)}>
            {(Object.keys(LANG_LABELS) as Lang[]).map((l) => (
              <option key={l} value={l}>{LANG_LABELS[l]}</option>
            ))}
          </select>
        </Row>
        <Row label={t.representative} hint={t.chooseInDex}>
          {rep ? (
            <button onClick={() => set("representative", null)} title={t.followCurrent}>
              ★ {nameOf(rep.names, rep.id, lang)} ✕
            </button>
          ) : (
            <span className="muted">{t.followCurrent}</span>
          )}
        </Row>
        <Row label={t.refreshInterval}>
          <select value={s.refresh_minutes} onChange={(e) => set("refresh_minutes", Number(e.target.value))}>
            {INTERVALS.map((m) => (
              <option key={m} value={m}>{m === 0 ? t.manual : t.everyMinutes(m)}</option>
            ))}
          </select>
        </Row>
        <Row label={t.limitDisplay}>
          <div className="segmented small">
            {(["used", "remaining"] as const).map((v) => (
              <button key={v} className={s.limit_display === v ? "on" : ""} onClick={() => set("limit_display", v)}>
                {v === "used" ? t.used : t.remaining}
              </button>
            ))}
          </div>
        </Row>
        <Row label={t.launchAtLogin} hint={autostartError || undefined}>
          <Toggle
            on={autostart}
            set={(v) =>
              invoke<boolean>("set_autostart", { enabled: v })
                .then((on) => {
                  setAutostart(on);
                  setAutostartError("");
                })
                .catch((e) => setAutostartError(String(e)))
            }
          />
        </Row>
      </Section>

      <Section title={t.difficulty}>
        <Row label={t.growthDifficulty} hint={`${t.faster} ← → ${t.slower}`}>
          <span className="slider">
            <input
              type="range"
              min={0}
              max={1}
              step={0.001}
              value={toPosition(difficulty)}
              onChange={(e) => setDifficulty(fromPosition(Number(e.target.value)))}
            />
            <span className="value">×{difficulty.toFixed(difficulty < 1 ? 2 : 1)}</span>
          </span>
        </Row>
        {difficulty !== props.c.difficulty && (
          <div className="actions">
            <button className="primary" onClick={() => invoke("set_difficulty", { value: difficulty })}>{t.save}</button>
          </div>
        )}
      </Section>

      <Section title={t.tray}>
        <Row label={t.trayTokens}>
          <Toggle on={s.tray_tokens} set={(v) => set("tray_tokens", v)} />
        </Row>
        <Row label={t.trayLimit} hint={t.trayHint}>
          <Toggle on={s.tray_limit} set={(v) => set("tray_limit", v)} />
        </Row>
      </Section>

      <Section title={t.floatingPet}>
        <Row label={t.petEnabled}>
          <Toggle on={s.pet_enabled} set={(v) => set("pet_enabled", v)} />
        </Row>
        {s.pet_enabled && (
          <Row label={t.petSize}>
            <span className="slider">
              <input
                type="range"
                min={PET_SIZE.min}
                max={PET_SIZE.max}
                step={PET_SIZE.step}
                value={size}
                onChange={(e) => {
                  const px = Number(e.target.value);
                  setSize(px);
                  resizePet(px);
                }}
              />
              <span className="value">{size}px</span>
            </span>
          </Row>
        )}
      </Section>

      <Section title={t.notifications}>
        <Row label={t.limitNotifications}>
          <Toggle on={s.limit_notifications} set={(v) => set("limit_notifications", v)} />
        </Row>
        {s.limit_notifications && (
          <>
            <Row label={t.warning}>
              <span className="slider">
                <input type="range" min={50} max={95} step={5} value={s.warn_threshold}
                  onChange={(e) => set("warn_threshold", Number(e.target.value))} />
                <span className="value">{s.warn_threshold}%</span>
              </span>
            </Row>
            <Row label={t.critical}>
              <span className="slider">
                <input type="range" min={80} max={100} step={5} value={s.crit_threshold}
                  onChange={(e) => set("crit_threshold", Number(e.target.value))} />
                <span className="value">{s.crit_threshold}%</span>
              </span>
            </Row>
          </>
        )}
        <Row label={t.companionNotifications}>
          <Toggle on={s.companion_notifications} set={(v) => set("companion_notifications", v)} />
        </Row>
      </Section>
    </>
  );
}
