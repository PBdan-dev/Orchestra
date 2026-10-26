import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useStore } from "../store";
import { api } from "../tauri";
import { PHASE_COLOR, PHASE_LABEL } from "../types";
import { brainStatus, providerName } from "../brainStatus";
import { AgentGlyph, I } from "./Icons";

/** Tiroir de détail : un agent (prompt, outils, mémoire) ou Orchestra lui-même. */
export function Inspector() {
  const inspect = useStore((s) => s.inspect);
  const setInspect = useStore((s) => s.setInspect);
  return (
    <motion.aside className="inspector" initial={{ x: 40, opacity: 0 }} animate={{ x: 0, opacity: 1 }} exit={{ x: 40, opacity: 0 }} transition={{ type: "spring", stiffness: 260, damping: 28 }}>
      <button className="icon-btn inspector-close" onClick={() => setInspect(null)} title="Fermer"><I.x size={16} /></button>
      {inspect?.type === "agent" ? <AgentDetail id={inspect.id} /> : <CoreDetail />}
    </motion.aside>
  );
}

function AgentDetail({ id }: { id: string }) {
  const node = useStore((s) => s.agents[id]);
  const all = useStore((s) => s.agents);
  const parentName = node?.parent ? all[node.parent]?.manifest.name : undefined;
  const children = Object.values(all).filter((a) => a.parent === id);
  const entriesCount = useStore((s) => s.entries.length);
  const setView = useStore((s) => s.setView);
  const setMemoryFocus = useStore((s) => s.setMemoryFocus);
  const [mem, setMem] = useState("");
  const [showPrompt, setShowPrompt] = useState(false);

  useEffect(() => {
    api.readTextFile(`memoire/agents/${id}.md`).then(setMem).catch(() => setMem(""));
  }, [id, entriesCount]);

  if (!node) return <div className="inspector-body"><p className="muted">Agent inconnu.</p></div>;
  const m = node.manifest;
  const c = m.color || "#818cf8";
  const memLines = mem.split("\n").filter((l) => l.startsWith("- ")).slice(-6);

  return (
    <div className="inspector-body">
      <div className="insp-head" style={{ ["--c" as any]: c }}>
        <span className="insp-glyph"><AgentGlyph icon={m.icon} size={22} /></span>
        <div>
          <h2>{m.name}</h2>
          <div className="muted small">v{m.version} · {m.author || "auteur inconnu"} · <code>{m.id}</code></div>
        </div>
      </div>
      <p className="insp-desc">{m.description}</p>

      <Section title="État">
        <div className={`status-pill ${node.status}`}>{node.status === "running" ? "en cours" : node.status === "done" ? "terminé" : node.status === "error" ? "erreur" : "branché"}</div>
        {node.detail && <p className="small">{node.detail}</p>}
        <p className="small muted">Source : {node.source} · {node.reason}</p>
        {node.parent && <p className="small">Sous-agent de <b>{parentName ?? node.parent}</b></p>}
        {children.length > 0 && <p className="small">A délégué à : {children.map((c) => c.manifest.name).join(", ")}</p>}
      </Section>

      <Section title="Cerveau">
        <p className="small">{m.model === "inherit" || !m.model ? "Hérite du cerveau d'Orchestra" : m.model}</p>
      </Section>

      <Section title="Outils">
        <div className="chips">
          {m.tools.length === 0 && <span className="muted small">aucun</span>}
          {m.tools.map((t) => <span key={t} className={`chip ${node.toolsUsed.includes(t) ? "used" : ""}`}><I.tool size={11} /> {t}{node.toolsUsed.includes(t) ? " ✓" : ""}</span>)}
        </div>
        <p className="small muted">✓ = utilisé pendant cette session</p>
      </Section>

      <Section title="Prompt système" action={<button className="link-btn" onClick={() => setShowPrompt(!showPrompt)}>{showPrompt ? "réduire" : "afficher"}</button>}>
        <pre className={`insp-pre ${showPrompt ? "" : "clamp"}`}>{m.system_prompt}</pre>
      </Section>

      <Section title="Mémoire de l'agent" action={<button className="link-btn" onClick={() => { setMemoryFocus(`memoire/agents/${id}.md`); setView("memory"); }}>ouvrir</button>}>
        {memLines.length === 0 ? <p className="muted small">Rien encore.</p> : <ul className="mem-lines">{memLines.map((l, i) => <li key={i}>{l.slice(2)}</li>)}</ul>}
      </Section>

      <Section title="Capacités déclarées">
        <div className="chips">{m.capabilities.map((k) => <span key={k} className="chip ghost">{k}</span>)}</div>
      </Section>
    </div>
  );
}

function CoreDetail() {
  const brain = useStore((s) => s.brain);
  const phase = useStore((s) => s.phase);
  const config = useStore((s) => s.config);
  const paths = useStore((s) => s.paths);
  const agents = useStore((s) => s.agents);
  const installed = useStore((s) => s.installed);
  const registry = useStore((s) => s.registry);
  const messages = useStore((s) => s.messages);
  const setView = useStore((s) => s.setView);
  const [facts, setFacts] = useState<number | null>(null);
  const entriesCount = useStore((s) => s.entries.length);
  const brainLoading = useStore((s) => s.brainLoading);
  const loadModel = useStore((s) => s.loadModel);
  const status = brainStatus(brain, brainLoading);

  useEffect(() => {
    api.readTextFile("memoire/orchestra.md").then((t) => setFacts(t.split("\n").filter((l) => l.startsWith("- ")).length)).catch(() => setFacts(null));
  }, [entriesCount]);

  return (
    <div className="inspector-body">
      <div className="insp-head" style={{ ["--c" as any]: PHASE_COLOR[phase] }}>
        <span className="insp-glyph"><I.stage size={22} /></span>
        <div>
          <h2>Orchestra</h2>
          <div className="muted small">orchestrateur · {PHASE_LABEL[phase]}</div>
        </div>
      </div>
      <p className="insp-desc">Je ne fais pas les tâches moi-même : je discute, je réponds, et je délègue le travail à des agents que je supervise. Tout ce que je fais est visible dans la Conscience, à gauche.</p>

      <Section title="Cerveau greffé" action={<button className="link-btn" onClick={() => setView("settings")}>changer</button>}>
        <p className="small"><b>{providerName(brain?.provider)}</b> · {brain?.model || "—"}</p>
        <p className="small foot-status" style={{ ["--c" as any]: status.color }}><span className="pill-dot" /> {status.label}</p>
        <p className="small muted">{brain?.detail}</p>
        {status.action === "load" && <button className="glass-btn sm" onClick={loadModel}><I.power size={11} /> Charger le modèle</button>}
      </Section>

      <Section title="Mémoire">
        <p className="small">{facts === null ? "…" : `${facts} fait(s) retenu(s)`} dans <code>orchestra.md</code></p>
        {config?.embeddings_enabled ? <p className="small muted">Index vectoriel actif ({config.embedding_model})</p> : <p className="small muted">Rappel par mots communs (index vectoriel désactivé)</p>}
      </Section>

      <Section title="Agents">
        <p className="small">{Object.keys(agents).length} branché(s) maintenant · {installed.length} installé(s) · {registry.length} dans le registre local</p>
        <p className="small muted">Agent Market : {config?.market_url ? config.market_url : "non configuré (registre local uniquement)"}</p>
      </Section>

      <Section title="Session">
        <p className="small">{messages.filter((m) => m.role === "user").length} demande(s) traitée(s)</p>
      </Section>

      <Section title="Dossiers">
        <div className="folder-list">
          {[["memoire", "memoire/", "mémoire lisible"], ["agents", "agents/", "manifestes"], ["workspace", "espace-de-travail/", "où les agents écrivent"], ["journal", "journal/", "tout l'historique"]].map(([k, n, d]) => (
            <button key={k} className="folder-row" onClick={() => api.openFolder(k)}><I.folder size={14} /> <b>{n}</b> <span className="muted">{d}</span> <I.external size={12} /></button>
          ))}
        </div>
        {paths && <p className="small muted path">{paths.root}</p>}
      </Section>
    </div>
  );
}

function Section({ title, children, action }: { title: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <section className="insp-section">
      <header><span>{title}</span>{action}</header>
      {children}
    </section>
  );
}
