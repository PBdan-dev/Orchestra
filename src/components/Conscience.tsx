import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { PHASE_COLOR, PHASE_LABEL, type Entry, type MissionVue, type ValidationMission } from "../types";
import { api } from "../tauri";
import { AgentGlyph, I } from "./Icons";

/** La conscience : tout ce qu'Orchestra fait, pense, rappelle et retient, en direct. */
export function Conscience() {
  const entries = useStore((s) => s.entries);
  const busy = useStore((s) => s.busy);
  const phase = useStore((s) => s.phase);
  const agents = useStore((s) => s.agents);
  const alertes = useStore((s) => s.alertesIntegrite);
  const vuIntegrite = useStore((s) => s.vuIntegrite);
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [entries]);

  const onScroll = () => {
    const el = ref.current!;
    stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  };

  const counts = {
    recalled: entries.filter((e) => e.kind === "recall").reduce((n, e) => n + (e as any).items.length, 0),
    memorized: entries.filter((e) => e.kind === "memorized").length,
    tools: entries.filter((e) => e.kind === "tool").length,
  };

  return (
    <section className="panel conscience">
      <header className="panel-head">
        <div className="panel-title"><I.brain size={15} /> Conscience</div>
        <div className="panel-meta">
          <span title="souvenirs rappelés"><I.search size={11} /> {counts.recalled}</span>
          <span title="faits enregistrés"><I.save size={11} /> {counts.memorized}</span>
          <span title="outils utilisés"><I.tool size={11} /> {counts.tools}</span>
          <span className={`live-dot ${busy ? "on" : ""}`} style={{ ["--c" as any]: PHASE_COLOR[phase] }} />
        </div>
      </header>
      {alertes.length > 0 && (
        <div className="alerte-integrite">
          <div className="card-head"><I.alert size={13} /> Traces modifiées hors d'Orchestra</div>
          {alertes.map((r, i) => (
            <div key={i} className="small">
              <code>{r.fichier}</code>, ligne {r.ligne} : {r.motif}.{" "}
              {r.agent_id ? <>Agent de la mission : <b>{r.agent_id}</b> (compté comme un écart).</> : "Aucun agent visé."}{" "}
              <span className="muted">Copie gardée : {r.preuve}</span>
            </div>
          ))}
          <div className="small muted">Rien n'a été réparé ni effacé. Cela peut venir d'un agent, mais aussi d'un plantage, du disque ou d'une modification à la main.</div>
          <button className="glass-btn sm" onClick={vuIntegrite}>J'ai vu</button>
        </div>
      )}
      <div className="panel-body feed" ref={ref} onScroll={onScroll}>
        {entries.length === 0 && (
          <div className="empty">
            <p>Ici, rien n'est caché.</p>
            <p className="muted">À chaque demande tu verras ma réflexion en direct, les souvenirs que je rappelle, les agents que je branche, les outils qu'ils utilisent et ce que j'enregistre en mémoire.</p>
          </div>
        )}
        <AnimatePresence initial={false}>
          {entries.map((e, i) => (
            <motion.div key={e.id} layout="position" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.25 }}>
              <EntryView e={e} streaming={busy && i === entries.length - 1 && e.kind === "thought"} agentName={(id: string) => agents[id]?.manifest.name ?? id} agentIcon={(id: string) => agents[id]?.manifest.icon ?? "spark"} />
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </section>
  );
}

function time(at: number) {
  return new Date(at).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function EntryView({ e, streaming, agentName, agentIcon }: { e: Entry; streaming: boolean; agentName: (id: string) => string; agentIcon: (id: string) => string }) {
  switch (e.kind) {
    case "user":
      return <div className="entry user-entry"><span className="t">{time(e.at)}</span><I.user size={12} /> <span className="clamp2">{e.text}</span></div>;
    case "phase":
      return (
        <div className="entry phase-entry" style={{ ["--c" as any]: PHASE_COLOR[e.phase] }}>
          <span className="t">{time(e.at)}</span><span className="phase-dot" /> {e.label || PHASE_LABEL[e.phase]}
        </div>
      );
    case "thought":
      return <ThoughtView text={e.text} streaming={streaming} />;
    case "recall":
      return (
        <div className="card recall">
          <div className="card-head"><I.search size={13} /> Souvenirs rappelés <span className="muted">· {e.items.length === 0 ? "aucun pertinent" : e.items.length}</span></div>
          {e.items.map((r, i) => (
            <div key={i} className="recall-item">
              <div className="recall-src"><code>{r.file}</code> › {r.section} <span className="muted">· {r.reason}</span></div>
              <div className="recall-text">{r.text}</div>
            </div>
          ))}
        </div>
      );
    case "agent":
      return (
        <div className="card agent-card" style={{ ["--c" as any]: e.agent.color || "#818cf8" }}>
          <div className="card-head"><I.plug size={13} /> {e.parent ? "Sous-agent branché" : "Agent branché"} : <b>{e.agent.name}</b></div>
          {e.parent && <div className="small muted">sur {agentName(e.parent)}</div>}
          <div className="small muted">depuis {e.source} · {e.reason}</div>
          <div className="small">Outils qu'il se donne : {e.agent.tools.length ? e.agent.tools.join(", ") : "aucun"}</div>
        </div>
      );
    case "agent_status":
      return (
        <div className={`entry status-entry ${e.status}`}>
          <span className="t">{time(e.at)}</span>
          <AgentGlyph icon={agentIcon(e.agentId)} size={12} /> <b>{agentName(e.agentId)}</b> · {e.status === "running" ? "mission reçue" : e.status === "done" ? "mission terminée" : "échec"}
          {e.detail && <div className="small muted indent">{e.detail}</div>}
        </div>
      );
    case "tool":
      return <ToolView e={e} name={agentName(e.agentId)} />;
    case "memorized":
      return (
        <div className="card memorized">
          <div className="card-head"><I.save size={13} /> Enregistré dans <code>{e.file}</code> › {e.section}</div>
          <div className="mem-line">{e.line}</div>
        </div>
      );
    case "nothing":
      return <div className="entry muted"><span className="t">{time(e.at)}</span><I.save size={12} /> Rien de durable à retenir de cet échange.</div>;
    case "mission":
      return <MissionCard m={e} name={agentName(e.agentId)} icon={agentIcon(e.agentId)} />;
    case "avertissement":
      return <div className="card avert"><div className="card-head"><I.alert size={13} /> Avertissement</div><div className="small">{e.message}</div></div>;
    case "error":
      return <div className="card error"><div className="card-head"><I.alert size={13} /> Erreur</div><div className="small">{e.message}</div></div>;
  }
}

function ThoughtView({ text, streaming }: { text: string; streaming: boolean }) {
  const [open, setOpen] = useState(true);
  const long = text.length > 900;
  return (
    <div className={`thought ${streaming ? "streaming" : ""}`}>
      <div className="thought-head" onClick={() => setOpen(!open)}>
        <I.spark size={12} /> Réflexion {streaming && <span className="caret" />}
        <span className="muted small right">{text.length} car. {long && (open ? "· replier" : "· déplier")}</span>
      </div>
      {open && <pre className="thought-text">{text}</pre>}
    </div>
  );
}

function ToolView({ e, name }: { e: Extract<Entry, { kind: "tool" }>; name: string }) {
  const [open, setOpen] = useState(false);
  const input = typeof e.input === "string" ? e.input : JSON.stringify(e.input);
  return (
    <div className="card tool" onClick={() => setOpen(!open)}>
      <div className="card-head"><I.tool size={13} /> <b>{name}</b> utilise <code>{e.tool}</code> <span className="muted small right">{open ? "réduire" : "détails"}</span></div>
      <div className="small mono clamp1">{input}</div>
      {open && <pre className="tool-out">{e.output}</pre>}
    </div>
  );
}

const ETATS: Record<string, string> = {
  termine: "terminée", partiel: "partielle", echec: "échec", bloque: "bloquée", precision_requise: "précision demandée",
  arrete_proprement: "arrêtée proprement", arrete_force: "arrêtée de force", en_pause: "en pause",
};

/**
 * Carte d'une mission (lot T3, D14) : trois parties bien séparées.
 * Pensée : ce que le modèle de l'agent a pensé et répondu, vu par la porte.
 * Actions : « observée » (appel d'outil vu par la porte) ou « déclarée » (compte rendu de l'agent).
 * Livrables : fichiers annoncés, et s'ils existent vraiment sur le disque.
 */
function MissionCard({ m, name, icon }: { m: MissionVue; name: string; icon: string }) {
  const [penseeOuverte, setPenseeOuverte] = useState(false);
  const enCours = !m.etat;
  return (
    <div className={`card mission-card ${enCours ? "live" : m.etat === "termine" ? "ok" : "ko"}`}>
      <div className="card-head">
        <AgentGlyph icon={icon} size={13} /> Mission de <b>{name}</b>
        <span className="muted small"> · projet {m.projet}</span>
        <span className={`tag right ${enCours ? "" : m.etat === "termine" ? "ok" : ""}`}>{enCours ? "en cours" : ETATS[m.etat!] ?? m.etat}</span>
      </div>
      {m.motif && <div className="small muted">{m.motif}</div>}

      {m.validations.map((v) => <ValidationCard key={v.cle} v={v} agent={name} />)}

      <div className="mission-part">
        <div className="mission-part-head" onClick={() => setPenseeOuverte(!penseeOuverte)}>
          <I.spark size={12} /> Pensée <span className="muted small">· {m.pensees.length} réponse(s) du modèle, vue(s) par la porte {m.pensees.length > 0 && (penseeOuverte ? "· replier" : "· déplier")}</span>
        </div>
        {penseeOuverte && m.pensees.map((p, i) => <pre key={i} className="thought-text">{p}</pre>)}
        {m.pensees.length === 0 && <div className="small muted indent">aucun modèle utilisé, ou rien encore</div>}
      </div>

      <div className="mission-part">
        <div className="mission-part-head"><I.tool size={12} /> Actions <span className="muted small">· {m.actions.length}</span></div>
        {m.actions.map((a, i) => (
          <div key={i} className="mission-action">
            <span className={`tag ${a.nature === "observee" ? "ok" : ""}`} title={a.nature === "observee" ? "vue par la porte du modèle" : "déclarée par l'agent dans son compte rendu"}>{a.nature === "observee" ? "observée" : "déclarée"}</span>{" "}
            <code>{a.action}</code> {a.raison && <span className="muted small">— {a.raison}</span>}
            {a.detail && <div className="small muted indent clamp2">{a.detail}</div>}
          </div>
        ))}
        {m.actions.length === 0 && <div className="small muted indent">aucune</div>}
      </div>

      <div className="mission-part">
        <div className="mission-part-head"><I.save size={12} /> Livrables</div>
        {m.livrables === null && <div className="small muted indent">{enCours ? "à la fin de la mission" : "aucun"}</div>}
        {m.livrables?.length === 0 && <div className="small muted indent">aucun fichier annoncé</div>}
        {m.livrables?.map((f, i) => (
          <div key={i} className="small indent">{f.existe ? "✓" : "✗"} <code>{f.chemin}</code> {!f.existe && <span className="muted">absent du disque (supprimé ou déplacé ?)</span>}</div>
        ))}
      </div>

      {m.memoire.length > 0 && (
        <div className="mission-part">
          <div className="mission-part-head"><I.memory size={12} /> Mémoire de l'agent modifiée</div>
          {m.memoire.map((c, i) => (
            <div key={i} className="small indent">
              <code>{c.fichier}</code> <span className="muted">({c.etat})</span>
              {c.ajoutees.map((l, j) => <div key={`a${j}`} className="diff-plus">+ {l}</div>)}
              {c.retirees.map((l, j) => <div key={`r${j}`} className="diff-moins">− {l}</div>)}
            </div>
          ))}
        </div>
      )}

      {m.verification && (
        <div className={`mission-part verif ${m.verification.ecarts.length ? "ko" : "ok"}`}>
          <div className="mission-part-head" title={m.verification.controles.join("\n")}>
            <I.check size={12} /> Vérification{" "}
            <span className="muted small">· {m.verification.ecarts.length === 0 ? "aucun écart entre ce que l'agent dit et ce qu'Orchestra constate" : `${m.verification.ecarts.length} écart(s)`}</span>
          </div>
          {m.verification.ecarts.map((e, i) => <div key={i} className="small indent warn">✗ {e}</div>)}
          {m.verification.ecarts.length > 0 && (
            <div className="small indent">
              {m.verification.statut === "quarantaine"
                ? `${name} est en quarantaine (${m.verification.totalEcarts} écarts au total) : je ne le choisirai plus tant que tu ne l'auras pas libéré dans le panneau Agents.`
                : `${name} perd sa confiance${m.verification.confianceRetiree ? " (retiré des Agents de confiance)" : ""} : ${m.verification.totalEcarts} écart(s) au total, quarantaine à 3.`}
            </div>
          )}
        </div>
      )}

      <div className="mission-foot">
        <button className="link-btn" onClick={() => { api.verifierIntegrite(m.missionId).catch(() => {}); api.openFolder(`mission:${m.missionId}`); }}><I.external size={11} /> dossier de la mission</button>
        <button className="link-btn" onClick={() => api.openFolder("projet")}><I.folder size={11} /> dossier du projet</button>
      </div>
    </div>
  );
}

const PORTEES: Record<string, string> = {
  cette_fois: "cette fois",
  type_session: "pour ce type d'action, pendant la session",
  tout_session: "tout, pendant la session",
  permanent: "tout, de façon permanente",
};

/** Validation d'une action irréversible (D28) : deux choix mis en avant, deux plus discrets avec confirmation. */
function ValidationCard({ v, agent }: { v: ValidationMission; agent: string }) {
  const repondre = useStore((s) => s.repondreValidation);
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const go = async (accepte: boolean, portee: string, confirmation?: string) => {
    if (confirmation && !window.confirm(confirmation)) return;
    setEnvoi(true);
    setErreur("");
    try {
      await repondre(v.cle, accepte, portee);
    } catch (e) {
      setErreur(String(e));
      setEnvoi(false);
    }
  };
  if (v.etat !== "attente") {
    return (
      <div className={`validation reglee ${v.etat}`}>
        {v.etat === "accepte" ? "✓ Autorisé" : "✗ Refusé"} {v.action && <code>{v.action}</code>} <span className="muted small">· {PORTEES[v.portee ?? ""] ?? v.portee}{v.automatique ? " (autorisation déjà donnée)" : ""}</span>
      </div>
    );
  }
  return (
    <div className="validation attente">
      <div className="card-head"><I.alert size={13} /> {agent} demande ta validation</div>
      <div><code>{v.action}</code></div>
      <div className="small">Raison : {v.raison || "non précisée"}</div>
      <div className="validation-boutons">
        <button className="glass-btn sm active" disabled={envoi} onClick={() => go(true, "cette_fois")}>Oui, cette fois</button>
        <button className="glass-btn sm" disabled={envoi} onClick={() => go(true, "type_session")}>Oui, pour ce type d'action pendant la session</button>
        <button className="glass-btn sm danger" disabled={envoi} onClick={() => go(false, "cette_fois")}>Non</button>
      </div>
      <div className="validation-discrets">
        <button className="link-btn" disabled={envoi} onClick={() => go(true, "tout_session", `Autoriser ${agent} à faire TOUTES ses actions irréversibles sans te demander, jusqu'à la fermeture d'Orchestra ?`)}>tout autoriser pour la session</button>
        <button className="link-btn" disabled={envoi} onClick={() => go(true, "permanent", `Autoriser ${agent} à faire TOUTES ses actions irréversibles sans te demander, de façon permanente ? Tu pourras retirer cette autorisation dans le panneau Agents.`)}>tout autoriser de façon permanente</button>
      </div>
      {erreur && <div className="small warn">{erreur}</div>}
    </div>
  );
}
