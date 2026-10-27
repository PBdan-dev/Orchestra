import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { brainStatus } from "../brainStatus";
import { runSimulation } from "../simulation";
import { PHASE_COLOR, PHASE_LABEL, type AgentNode } from "../types";
import { Core } from "./Core";
import { AgentGlyph, I } from "./Icons";
import { Inspector } from "./Inspector";
import { AgentsPanel } from "./AgentsPanel";
import { ModeSwitch } from "./ModeSwitch";

const NODE_R = 31;

type Placed = { a: AgentNode; angle: number; x: number; y: number; depth: number };

/** La scène : le cœur au centre, les agents (et sous-agents) qui se branchent autour. */
export function Stage() {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const phase = useStore((s) => s.phase);
  const phaseLabel = useStore((s) => s.phaseLabel);
  const brain = useStore((s) => s.brain);
  const brainLoading = useStore((s) => s.brainLoading);
  const loadModel = useStore((s) => s.loadModel);
  const refreshBrain = useStore((s) => s.refreshBrain);
  const agents = useStore((s) => s.agents);
  const inspect = useStore((s) => s.inspect);
  const setInspect = useStore((s) => s.setInspect);
  const coreScale = useStore((s) => s.coreScale);
  const setCoreScale = useStore((s) => s.setCoreScale);
  const agentsPanel = useStore((s) => s.agentsPanel);
  const setAgentsPanel = useStore((s) => s.setAgentsPanel);
  const busy = useStore((s) => s.busy);
  const simulating = useStore((s) => s.simulating);
  const voiceOn = useStore((s) => s.voiceOn);
  const voiceConn = useStore((s) => s.voiceConn);
  const voiceLlm = useStore((s) => s.config?.voice_llm);
  const switching = useStore((s) => s.switching);

  useEffect(() => {
    const el = ref.current!;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const inspectorOpen = inspect !== null;
  const usableW = inspectorOpen ? Math.max(320, size.w - 380) : size.w;
  const cx = usableW / 2;
  const cy = size.h / 2;
  const minSide = Math.min(usableW, size.h);
  const R = Math.max(70, Math.min(230, minSide * 0.17 * coreScale));
  const orbit = Math.max(R + 70, Math.min(R + 150, minSide / 2 - 64));

  // placement : agents de premier niveau sur l'orbite, sous-agents en éventail derrière leur parent
  const list = Object.values(agents).sort((a, b) => a.attachedAt - b.attachedAt);
  const roots = list.filter((a) => !a.parent || !agents[a.parent]);
  const placed: Record<string, Placed> = {};
  roots.forEach((a, i) => {
    const angle = -Math.PI / 5 + (i * Math.PI * 2) / Math.max(roots.length, 1);
    placed[a.manifest.id] = { a, angle, depth: 0, x: cx + Math.cos(angle) * orbit, y: cy + Math.sin(angle) * orbit };
  });
  const placeChildren = (parent: Placed) => {
    const kids = list.filter((k) => k.parent === parent.a.manifest.id);
    kids.forEach((k, i) => {
      const spread = 0.55;
      let angle = parent.angle + (kids.length === 1 ? 0.35 : -spread + (i * 2 * spread) / (kids.length - 1));
      const dist = 118;
      const inside = (a: number) => {
        const x = parent.x + Math.cos(a) * dist, y = parent.y + Math.sin(a) * dist;
        return x > 70 && x < usableW - 70 && y > 50 && y < size.h - 70;
      };
      // si le sous-agent sortirait de la scène, on fait pivoter la branche jusqu'à ce qu'il tienne
      for (let t = 0; t < 12 && !inside(angle); t++) angle += (t % 2 ? -1 : 1) * (t + 1) * 0.3;
      const x = Math.max(60, Math.min(usableW - 60, parent.x + Math.cos(angle) * dist));
      const y = Math.max(44, Math.min(size.h - 64, parent.y + Math.sin(angle) * dist));
      const p: Placed = { a: k, angle, depth: parent.depth + 1, x, y };
      placed[k.manifest.id] = p;
      placeChildren(p);
    });
  };
  Object.values(placed).filter((p) => p.depth === 0).forEach(placeChildren);
  const nodes = Object.values(placed);

  const color = PHASE_COLOR[phase];
  const status = brainStatus(brain, brainLoading);

  // Ctrl + molette sur la scène : taille du cœur
  const onWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey) { e.preventDefault(); setCoreScale(coreScale * (e.deltaY < 0 ? 1.06 : 0.94)); }
  };

  return (
    <div className="stage" ref={ref} onWheel={onWheel}>
      <div className="stage-grid" />
      <Core phase={phase} cx={cx} cy={cy} R={R} />

      <svg className="stage-svg" width={size.w} height={size.h}>
        <defs>
          <radialGradient id="orbitGlow"><stop offset="0%" stopColor="#a5f3fc" stopOpacity="0.5" /><stop offset="100%" stopColor="#a5f3fc" stopOpacity="0" /></radialGradient>
        </defs>
        <circle cx={cx} cy={cy} r={orbit} className="orbit" style={{ opacity: roots.length ? 0.42 : 0.2 }} />
        <circle cx={cx} cy={cy} r={orbit} className="orbit-runner" />
        {nodes.map((p) => {
          const parent = p.a.parent ? placed[p.a.parent] : undefined;
          const fromX = parent ? parent.x : cx, fromY = parent ? parent.y : cy;
          const fromR = parent ? NODE_R + 4 : R + 8;
          const ang = Math.atan2(p.y - fromY, p.x - fromX);
          const sx = fromX + Math.cos(ang) * fromR, sy = fromY + Math.sin(ang) * fromR;
          const ex = p.x - Math.cos(ang) * (NODE_R + 4), ey = p.y - Math.sin(ang) * (NODE_R + 4);
          const bend = parent ? 14 : 26;
          const mx = (sx + ex) / 2 + Math.sin(ang) * bend, my = (sy + ey) / 2 - Math.cos(ang) * bend;
          const d = `M ${sx} ${sy} Q ${mx} ${my} ${ex} ${ey}`;
          const id = `link-${p.a.manifest.id}`;
          const c = p.a.manifest.color || "#818cf8";
          const running = p.a.status === "running";
          return (
            <g key={p.a.manifest.id}>
              <motion.path d={d} fill="none" stroke={c} strokeWidth={6} strokeOpacity={running ? 0.12 : 0.05}
                initial={{ pathLength: 0 }} animate={{ pathLength: 1 }} transition={{ duration: 0.9, ease: "easeInOut" }} />
              <motion.path id={id} d={d} fill="none" stroke={c} strokeWidth={running ? 1.8 : 1.2} strokeDasharray={parent ? "4 4" : undefined}
                strokeOpacity={running ? 0.95 : 0.55} initial={{ pathLength: 0 }} animate={{ pathLength: 1 }}
                transition={{ duration: 0.9, ease: "easeInOut" }} style={{ filter: running ? `drop-shadow(0 0 6px ${c})` : "none" }} />
              {running && [0, 0.5].map((off) => (
                <circle key={off} r={3.2} fill={c} style={{ filter: `drop-shadow(0 0 6px ${c})` }}>
                  <animateMotion dur="1.4s" begin={`${off}s`} repeatCount="indefinite"><mpath href={`#${id}`} /></animateMotion>
                </circle>
              ))}
              {running && [0.25, 0.75].map((off) => (
                <circle key={`b${off}`} r={2} fill="#e0f2fe" opacity={0.85}>
                  <animateMotion dur="1.4s" begin={`${off}s`} repeatCount="indefinite" keyPoints="1;0" keyTimes="0;1" calcMode="linear"><mpath href={`#${id}`} /></animateMotion>
                </circle>
              ))}
            </g>
          );
        })}
      </svg>

      {/* centre : identité et état du cerveau */}
      <div className="core-label" style={{ left: cx, top: cy, width: Math.max(150, R * 1.75), ["--fs" as any]: Math.max(0.72, Math.min(1.35, R / 140)) }}>
        <button className="core-hit" onClick={() => setInspect(inspect?.type === "core" ? null : { type: "core" })} title="Modèles et détails d'Orchestra" />
        <span className={`brain-pill ${status.tone}`} style={{ ["--c" as any]: status.color }} title={brain?.detail}>
          <span className="pill-dot" />{status.label}
        </span>
        <span className="core-title">Orchestra</span>
        <span className="core-model">{(voiceOn ? voiceLlm : brain?.model) || "aucun modèle"}</span>
        <AnimatePresence mode="wait">
          <motion.span key={phase + phaseLabel} className="core-phase" style={{ color }}
            initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={{ duration: 0.25 }}>
            {phaseLabel || PHASE_LABEL[phase]}
          </motion.span>
        </AnimatePresence>
        <AnimatePresence>
          {status.action && !busy && !voiceOn && (
            <motion.button key={status.action} className="core-action" initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.9 }}
              onClick={() => (status.action === "load" ? loadModel() : refreshBrain())}
              title={status.action === "load" ? "Charger le modèle en mémoire pour qu'il réponde tout de suite" : "Revérifier la connexion"}>
              {status.action === "load" ? <><I.power size={11} /> Charger</> : <><I.refresh size={11} /> Réessayer</>}
            </motion.button>
          )}
        </AnimatePresence>
        <ModeSwitch />
        {(switching || (voiceOn && voiceConn !== "on")) && <span className="core-note">{switching || "Je prépare ma voix… (≈ 30 s)"}</span>}
      </div>

      {/* agents */}
      <AnimatePresence>
        {nodes.map((p) => (
          <AgentBubble key={p.a.manifest.id} node={p.a} x={p.x} y={p.y} sub={p.depth > 0}
            onClick={() => setInspect({ type: "agent", id: p.a.manifest.id })}
            selected={inspect?.type === "agent" && inspect.id === p.a.manifest.id} />
        ))}
      </AnimatePresence>

      {nodes.length === 0 && (
        <motion.div className="stage-hint" initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ delay: 1.2, duration: 1 }}
          style={{ left: cx, top: Math.min(size.h - 30, cy + orbit + 26) }}>
          Les agents qu'Orchestra appelle se brancheront sur cette orbite.
        </motion.div>
      )}

      {/* barre d'outils de la scène */}
      <div className="stage-tools tl">
        <button className={`glass-btn ${agentsPanel ? "active" : ""}`} onClick={() => setAgentsPanel(!agentsPanel)}><I.grid size={13} /> Agents</button>
        <button className="glass-btn" onClick={() => runSimulation()} disabled={busy || simulating} title="Rejoue une demande fictive avec 3 agents et un sous-agent (rien n'est écrit ni envoyé au modèle)">
          <I.play size={12} /> {simulating ? "Simulation…" : "Simulation"}
        </button>
      </div>
      <div className="stage-tools br" style={{ right: inspectorOpen ? 392 : 12 }}>
        <span className="tools-label">Cœur</span>
        <button className="glass-icon" onClick={() => setCoreScale(coreScale - 0.1)} title="Réduire le cœur"><I.minus size={12} /></button>
        <input className="core-slider" type="range" min={0.6} max={1.6} step={0.02} value={coreScale} onChange={(e) => setCoreScale(Number(e.target.value))} title="Taille du cœur (Ctrl + molette)" />
        <button className="glass-icon" onClick={() => setCoreScale(coreScale + 0.1)} title="Agrandir le cœur"><I.plus size={12} /></button>
      </div>

      <AnimatePresence>{agentsPanel && <AgentsPanel />}</AnimatePresence>
      <AnimatePresence>{inspectorOpen && <Inspector />}</AnimatePresence>
    </div>
  );
}

function AgentBubble({ node, x, y, onClick, selected, sub }: { node: AgentNode; x: number; y: number; onClick: () => void; selected: boolean; sub: boolean }) {
  const unplug = useStore((s) => s.unplug);
  const c = node.manifest.color || "#818cf8";
  const status = node.status;
  const statusText = status === "running" ? "en cours" : status === "done" ? "terminé" : status === "error" ? "erreur" : "branché";
  return (
    <motion.div className={`agent-node ${status} ${selected ? "selected" : ""} ${sub ? "sub" : ""}`} style={{ ["--c" as any]: c }}
      initial={{ opacity: 0, scale: 0.2, left: x, top: y }} animate={{ opacity: 1, scale: 1, left: x, top: y }} exit={{ opacity: 0, scale: 0.3, filter: "blur(6px)" }}
      transition={{ type: "spring", stiffness: 220, damping: 20, opacity: { delay: 0.4 }, scale: { delay: 0.4, type: "spring", stiffness: 260, damping: 16 } }}>
      <button className="agent-core" onClick={onClick} title={node.manifest.description}>
        <span className="agent-ring" />
        <span className="agent-glyph"><AgentGlyph icon={node.manifest.icon} size={sub ? 16 : 20} /></span>
      </button>
      <button className="agent-unplug" onClick={(e) => { e.stopPropagation(); unplug(node.manifest.id); }} title="Débrancher (rien n'est supprimé)"><I.x size={10} /></button>
      <span className="agent-name">{node.manifest.name}</span>
      <span className="agent-status">{statusText}</span>
      <AnimatePresence>
        {node.toolsUsed.map((t, i) => {
          const ang = -Math.PI / 2 + i * 1.0 - (node.toolsUsed.length - 1) * 0.5;
          const r = sub ? 46 : 56;
          return (
            <motion.span key={t} className={`tool-chip ${node.lastTool?.tool === t && Date.now() - (node.lastTool?.at ?? 0) < 2500 ? "hot" : ""}`}
              style={{ left: (sub ? 25 : NODE_R) + Math.cos(ang) * r, top: (sub ? 25 : NODE_R) + Math.sin(ang) * r }}
              initial={{ opacity: 0, scale: 0 }} animate={{ opacity: 1, scale: 1 }} transition={{ type: "spring", stiffness: 300, damping: 16 }}>
              <I.tool size={9} /> {t}
            </motion.span>
          );
        })}
      </AnimatePresence>
    </motion.div>
  );
}
