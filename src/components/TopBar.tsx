import { motion } from "framer-motion";
import { useStore, type View } from "../store";
import { api } from "../tauri";
import { PHASE_COLOR } from "../types";
import { I } from "./Icons";

const TABS: { id: View; label: string; icon: keyof typeof I }[] = [
  { id: "stage", label: "Scène", icon: "stage" },
  { id: "memory", label: "Mémoire", icon: "memory" },
  { id: "journal", label: "Journal", icon: "journal" },
];

export function TopBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const brain = useStore((s) => s.brain);
  const phase = useStore((s) => s.phase);
  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-ring" style={{ ["--c" as any]: PHASE_COLOR[phase] }} />
        <span className="brand-name">Orchestra</span>
        <span className="brand-tag">v0.1</span>
      </div>
      <nav className="tabs">
        {TABS.map((t) => {
          const Icon = I[t.icon];
          return (
            <button key={t.id} className={`tab ${view === t.id ? "active" : ""}`} onClick={() => setView(t.id)}>
              <Icon size={14} /> {t.label}
              {view === t.id && <motion.span layoutId="tab-underline" className="tab-underline" />}
            </button>
          );
        })}
      </nav>
      <div className="topbar-right">
        <button className={`brain-chip ${brain?.online ? "on" : "off"}`} onClick={() => setView("settings")} title={brain?.detail}>
          <I.brain size={14} />
          <span>{brain ? `${brain.provider} · ${brain.model || "—"}` : "…"}</span>
          <span className="dot" />
        </button>
        <button className="icon-btn" onClick={() => api.openFolder("root")} title="Ouvrir le dossier de données d'Orchestra"><I.folder size={16} /></button>
        <button className={`icon-btn ${view === "settings" ? "active" : ""}`} onClick={() => setView("settings")} title="Paramètres"><I.gear size={16} /></button>
      </div>
    </header>
  );
}
