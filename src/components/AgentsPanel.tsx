import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useStore } from "../store";
import { api } from "../tauri";
import type { Manifest } from "../types";
import { AgentGlyph, I } from "./Icons";

/**
 * Panneau modulaire des agents : chaque agent est indépendant. On peut l'installer,
 * le brancher sur la scène, le débrancher ou le désinstaller sans toucher aux autres.
 * Désinstaller retire seulement son manifeste : sa mémoire reste dans memoire/agents/.
 */
export function AgentsPanel() {
  const registry = useStore((s) => s.registry);
  const installed = useStore((s) => s.installed);
  const agents = useStore((s) => s.agents);
  const plug = useStore((s) => s.plug);
  const unplug = useStore((s) => s.unplug);
  const installAgent = useStore((s) => s.installAgent);
  const uninstallAgent = useStore((s) => s.uninstallAgent);
  const setAgentsPanel = useStore((s) => s.setAgentsPanel);
  const autorisations = useStore((s) => s.autorisations);
  const retirerAutorisation = useStore((s) => s.retirerAutorisation);
  const reinitialiserMemoire = useStore((s) => s.reinitialiserMemoire);
  const refreshAutorisations = useStore((s) => s.refreshAutorisations);
  useEffect(() => { refreshAutorisations().catch(() => {}); }, [refreshAutorisations]);
  const remettreAZero = async (m: Manifest) => {
    if (!window.confirm(`Remettre à zéro la mémoire de ${m.name} ? L'ancienne mémoire est archivée (agents/memoires-archivees/), pas effacée.`)) return;
    try {
      const archive = await reinitialiserMemoire(m.id);
      window.alert(`Mémoire de ${m.name} remise à zéro. Ancienne mémoire : ${archive}`);
    } catch (e) {
      window.alert(String(e));
    }
  };
  const PORTEE: Record<string, string> = { type_session: "pour la session", tout_session: "tout, pour la session", permanent: "tout, permanent" };
  const [busyId, setBusyId] = useState<string | null>(null);

  const all: Manifest[] = [...installed, ...registry.filter((r) => !installed.some((i) => i.id === r.id))];
  const wrap = async (id: string, fn: () => Promise<void>) => { setBusyId(id); try { await fn(); } finally { setBusyId(null); } };

  return (
    <motion.aside className="agents-panel glass" initial={{ opacity: 0, y: -10, scale: 0.98 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: -10, scale: 0.98 }} transition={{ duration: 0.2 }}>
      <header className="ap-head">
        <span><I.grid size={13} /> Agents disponibles</span>
        <button className="icon-btn" onClick={() => setAgentsPanel(false)}><I.x size={14} /></button>
      </header>
      <p className="ap-note">Chaque agent est un module indépendant. Brancher ou débrancher ne touche ni aux autres agents ni à la mémoire.</p>
      <div className="ap-list">
        {all.length === 0 && <p className="muted small">Aucun agent. Ajoute-en dans agents/registre-local.json.</p>}
        {all.map((m) => {
          const isInstalled = installed.some((i) => i.id === m.id);
          const onStage = !!agents[m.id];
          const c = m.color || "#818cf8";
          return (
            <div key={m.id} className={`ap-row ${onStage ? "on" : ""}`} style={{ ["--c" as any]: c }}>
              <span className="ap-glyph"><AgentGlyph icon={m.icon} size={15} /></span>
              <div className="ap-info">
                <div className="ap-name">{m.name} {isInstalled ? <span className="tag ok">installé</span> : <span className="tag">registre</span>}</div>
                <div className="ap-desc">{m.description}</div>
                <div className="ap-tools">{m.tools.map((t) => <span key={t} className="mini">{t}</span>)}</div>
              </div>
              <div className="ap-actions">
                <button className={`glass-btn sm ${onStage ? "active" : ""}`} onClick={() => (onStage ? unplug(m.id) : plug(m))}>
                  {onStage ? <><I.unplug size={11} /> Débrancher</> : <><I.plug size={11} /> Brancher</>}
                </button>
                {isInstalled && <button className="link-btn" disabled={busyId === m.id} onClick={() => wrap(m.id, () => remettreAZero(m))}>remettre sa mémoire à zéro</button>}
                {isInstalled ? (
                  <button className="link-btn danger" disabled={busyId === m.id} onClick={() => wrap(m.id, () => uninstallAgent(m.id))}>désinstaller</button>
                ) : (
                  <button className="link-btn" disabled={busyId === m.id} onClick={() => wrap(m.id, () => installAgent(m.id))}>installer</button>
                )}
              </div>
            </div>
          );
        })}
      </div>
      <div className="ap-autorisations">
        <div><I.alert size={12} /> Actions irréversibles autorisées sans question</div>
        {autorisations.length === 0 && <div className="muted small">aucune : chaque action irréversible te sera demandée</div>}
        {autorisations.map((a) => (
          <div key={`${a.agent_id}-${a.action}-${a.portee}`} className="ap-aut">
            <span>{a.agent_id} · {a.action ? <code>{a.action}</code> : "toutes"} · <span className="muted">{PORTEE[a.portee] ?? a.portee}</span></span>
            <button className="link-btn danger" onClick={() => retirerAutorisation(a)}>retirer</button>
          </div>
        ))}
      </div>
      <footer className="ap-foot">
        <button className="link-btn" onClick={() => api.openFolder("agents")}><I.external size={11} /> ouvrir le dossier agents</button>
      </footer>
    </motion.aside>
  );
}
