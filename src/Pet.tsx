import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Menu } from "@tauri-apps/api/menu";
import { LogicalSize, PhysicalPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { Companion, EGG, TodayUsage, fmt, nameOf, sprite } from "./App";

const DRAG_SLOP = 4;
/** Same range as the original (48–384px). The window size itself is the setting; window-state saves it. */
const SIZES = [48, 72, 96, 120, 160, 224, 288, 384];
const toggle = (label: string) => invoke("toggle_window", { label });

/** Resizes the pet window, keeping its bottom-center (where the sprite stands) in place. */
async function resize(px: number) {
  const w = getCurrentWindow();
  const [pos, size, scale] = await Promise.all([w.outerPosition(), w.outerSize(), w.scaleFactor()]);
  const next = Math.round(px * scale);
  await w.setSize(new LogicalSize(px, px));
  await w.setPosition(
    new PhysicalPosition(pos.x + Math.round((size.width - next) / 2), pos.y + size.height - next),
  );
}

async function currentSize() {
  const w = getCurrentWindow();
  const [size, scale] = await Promise.all([w.innerSize(), w.scaleFactor()]);
  return Math.round(size.width / scale);
}

/** Floating Pokémon: drag to move, click to open the popover, right-click for a menu. */
export default function Pet() {
  const [c, setC] = useState<Companion | null>(null);
  const [today, setToday] = useState<number | null>(null);
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    invoke<Companion>("companion").then(setC);
    invoke<TodayUsage>("today_usage").then((u) => setToday(u.total));
    const subs = [
      listen<Companion>("companion", (e) => setC(e.payload)),
      listen<TodayUsage>("usage", (e) => setToday(e.payload.total)),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  const mon = c?.active;
  const id = mon?.path_ids[mon.stage_index];
  const src = mon && id ? sprite(id, mon.is_shiny) : EGG;
  const name = mon && id ? nameOf(mon.line.names, id) : "알";
  const title = today === null ? name : `${name} · 오늘 ${fmt.format(today)}`;

  async function showMenu(e: React.MouseEvent) {
    e.preventDefault();
    const current = await currentSize();
    const menu = await Menu.new({
      items: [
        { id: "open", text: "창 열기", action: () => toggle("main") },
        {
          text: "크기",
          items: SIZES.map((px) => ({
            id: `size-${px}`,
            text: `${px}px`,
            checked: px === current,
            action: () => resize(px),
          })),
        },
        { id: "hide", text: "펫 숨기기", action: () => toggle("pet") },
        { id: "quit", text: "종료", action: () => invoke("quit") },
      ],
    });
    await menu.popup();
  }

  return (
    <div
      className="pet"
      onContextMenu={showMenu}
      onMouseDown={(e) => {
        if (e.button === 0) press.current = { x: e.screenX, y: e.screenY };
      }}
      onMouseMove={(e) => {
        const p = press.current;
        if (p && Math.hypot(e.screenX - p.x, e.screenY - p.y) > DRAG_SLOP) {
          press.current = null;
          getCurrentWindow().startDragging();
        }
      }}
      onMouseUp={() => {
        if (press.current) toggle("main");
        press.current = null;
      }}
    >
      <img src={src} alt={name} title={title} draggable={false} />
    </div>
  );
}
