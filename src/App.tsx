import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Tokens = { input: number; output: number; cache_write: number; cache_read: number };
type TodayUsage = { claude: Tokens; codex: Tokens; total: number };

const sum = (t: Tokens) => t.input + t.output + t.cache_write + t.cache_read;
const fmt = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

function App() {
  const [usage, setUsage] = useState<TodayUsage | null>(null);

  useEffect(() => {
    invoke<TodayUsage>("today_usage").then(setUsage);
    const unlisten = listen<TodayUsage>("usage", (e) => setUsage(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (!usage) return <main className="container">Loading…</main>;

  return (
    <main className="container">
      <h1>{fmt.format(usage.total)}</h1>
      <p className="muted">tokens today</p>
      <table>
        <tbody>
          <tr><td>Claude Code</td><td>{fmt.format(sum(usage.claude))}</td></tr>
          <tr><td>Codex</td><td>{fmt.format(sum(usage.codex))}</td></tr>
        </tbody>
      </table>
    </main>
  );
}

export default App;
