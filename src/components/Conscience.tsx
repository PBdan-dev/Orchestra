import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { PHASE_COLOR, PHASE_LABEL, type Entry } from "../types";
import { AgentGlyph, I } from "./Icons";

/** La conscience : tout ce qu'Orchestra fait, pense, rappelle et retient, en direct. */
export function Conscience() {
  const entries = useStore((s) => s.entries);
  const busy = useStore((s) => s.busy);
  const phase = useStore((s) => s.phase);
  const agents = useStore((s) => s.agents);
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
