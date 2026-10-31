import { motion } from "framer-motion";
import { useStore, type View } from "../store";
import { api } from "../tauri";
import { brainStatus } from "../brainStatus";
import { PHASE_COLOR } from "../types";
import { I } from "./Icons";

const TABS: { id: View; label: string; icon: keyof typeof I }[] = [
  { id: "stage", label: "Scène", icon: "stage" },
  { id: "missions", label: "Missions", icon: "folder" },
  { id: "memory", label: "Mémoire", icon: "memory" },
  { id: "journal", label: "Journal", icon: "journal" },
];

export function TopBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const brain = useStore((s) => s.brain);
  const brainLoading = useStore((s) => s.brainLoading);
  const phase = useStore((s) => s.phase);
  const status = brainStatus(brain, brainLoading);
  const projets = useStore((s) => s.projets);
  const choisirProjet = useStore((s) => s.choisirProjet);
  const changerProjet = async (v: string) => {
    if (v === "__nouveau__") {
      const nom = window.prompt("Nom du nouveau projet (dossier dans Orchestra/projets/) :");
      if (nom && nom.trim()) await choisirProjet(nom.trim()).catch((e) => window.alert(String(e)));
      return;
    }
    await choisirProjet(v).catch((e) => window.alert(String(e)));
  };
  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-ring" style={{ ["--c" as any]: PHASE_COLOR[phase] }} />
        <span className="brand-name">Orchestra</span>
        <span className="brand-tag">v0.2</span>
      </div>
      <nav className="tabs glass">
        {TABS.map((t) => {
          const Icon = I[t.icon];
          return (
            <button key={t.id} className={`tab ${view === t.id ? "active" : ""}`} onClick={() => setView(t.id)}>
              {view === t.id && <motion.span layoutId="tab-bg" className="tab-bg" transition={{ type: "spring", stiffness: 380, damping: 32 }} />}
              <Icon size={14} /> <span>{t.label}</span>
            </button>
          );
        })}
      </nav>
      <div className="topbar-right">
        <label className="brain-chip glass projet-chip" title="Projet où vont les missions et les livrables des agents (choisi ici seulement)">
          <I.folder size={13} />
          <select value={projets?.actif ?? "divers"} onChange={(e) => changerProjet(e.target.value)}>
            {(projets?.liste ?? ["divers"]).map((p) => <option key={p} value={p}>Projet : {p}</option>)}
            <option value="__nouveau__">Nouveau projet…</option>
          </select>
        </label>
        <button className={`brain-chip glass ${status.tone}`} style={{ ["--c" as any]: status.color }} onClick={() => setView("settings")} title={brain?.detail}>
          <span className="pill-dot" />
          <span>{status.label}</span>
          {brain?.model && <span className="chip-model">{brain.model}</span>}
        </button>
        <button className="icon-btn" onClick={() => api.openFolder("root")} title="Ouvrir le dossier de données d'Orchestra"><I.folder size={16} /></button>
        <button className={`icon-btn ${view === "settings" ? "active" : ""}`} onClick={() => setView("settings")} title="Paramètres"><I.gear size={16} /></button>
      </div>
    </header>
  );
}
