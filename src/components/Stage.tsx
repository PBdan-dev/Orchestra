import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { PHASE_COLOR, PHASE_LABEL, type AgentNode } from "../types";
import { Core } from "./Core";
import { AgentGlyph, I } from "./Icons";
import { Inspector } from "./Inspector";

/** La scène : le cœur au centre, les agents qui se branchent autour. */
export function Stage() {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const phase = useStore((s) => s.phase);
  const phaseLabel = useStore((s) => s.phaseLabel);
  const brain = useStore((s) => s.brain);
  const agents = useStore((s) => s.agents);
  const inspect = useStore((s) => s.inspect);
  const setInspect = useStore((s) => s.setInspect);

  useEffect(() => {
    const el = ref.current!;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const list = Object.values(agents).sort((a, b) => a.attachedAt - b.attachedAt);
  const inspectorOpen = inspect !== null;
  const usableW = inspectorOpen ? size.w - 380 : size.w;
  const cx = usableW / 2;
  const cy = size.h / 2 - 10;
  const R = Math.max(104, Math.min(160, Math.min(usableW, size.h) * 0.17));
  const orbit = Math.min(R + 165, Math.min(usableW, size.h) / 2 - 70);

  // les agents se répartissent sur l'orbite, le premier en haut à droite
  const nodes = list.map((a, i) => {
    const n = list.length;
    const angle = -Math.PI / 5 + (i * Math.PI * 2) / Math.max(n, 1);
    return { a, angle, x: cx + Math.cos(angle) * orbit, y: cy + Math.sin(angle) * orbit };
  });

  const color = PHASE_COLOR[phase];

  return (
    <div className="stage" ref={ref}>
      <Core phase={phase} cx={cx} cy={cy} R={R} />

      {/* orbite d'accueil */}
      <svg className="stage-svg" width={size.w} height={size.h}>
        <circle cx={cx} cy={cy} r={orbit} className="orbit" style={{ opacity: list.length ? 0.35 : 0.18 }} />
        {nodes.map(({ a, angle, x, y }) => {
          const sx = cx + Math.cos(angle) * (R + 8), sy = cy + Math.sin(angle) * (R + 8);
          const ex = x - Math.cos(angle) * 30, ey = y - Math.sin(angle) * 30;
          const mx = (sx + ex) / 2 + Math.sin(angle) * 24, my = (sy + ey) / 2 - Math.cos(angle) * 24;
          const d = `M ${sx} ${sy} Q ${mx} ${my} ${ex} ${ey}`;
          const id = `link-${a.manifest.id}`;
          const c = a.manifest.color || "#818cf8";
          const running = a.status === "running";
          return (
            <g key={a.manifest.id}>
              <motion.path id={id} d={d} fill="none" stroke={c} strokeWidth={running ? 2 : 1.3}
                strokeOpacity={running ? 0.9 : 0.5} initial={{ pathLength: 0 }} animate={{ pathLength: 1 }}
                transition={{ duration: 0.9, ease: "easeInOut" }} style={{ filter: running ? `drop-shadow(0 0 6px ${c})` : "none" }} />
              {running && [0, 0.45].map((off) => (
                <circle key={off} r={3.2} fill={c} style={{ filter: `drop-shadow(0 0 5px ${c})` }}>
                  <animateMotion dur="1.5s" begin={`${off}s`} repeatCount="indefinite"><mpath href={`#${id}`} /></animateMotion>
                </circle>
              ))}
              {running && [0.2, 0.65].map((off) => (
                <circle key={`b${off}`} r={2.2} fill="#fff" opacity={0.8}>
                  <animateMotion dur="1.5s" begin={`${off}s`} repeatCount="indefinite" keyPoints="1;0" keyTimes="0;1" calcMode="linear"><mpath href={`#${id}`} /></animateMotion>
                </circle>
              ))}
            </g>
          );
        })}
      </svg>

      {/* centre : identité */}
      <button className="core-label" style={{ left: cx, top: cy, width: R * 1.7 }} onClick={() => setInspect(inspect?.type === "core" ? null : { type: "core" })}>
        <span className="core-eyebrow" style={{ color }}>{brain?.online ? "●" : "○"} {brain?.provider ?? "…"}</span>
        <span className="core-title">Orchestra</span>
        <span className="core-model">{brain?.model || (brain ? "aucun modèle" : "détection…")}</span>
        <AnimatePresence mode="wait">
          <motion.span key={phase + phaseLabel} className="core-phase" style={{ color }}
            initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={{ duration: 0.25 }}>
            {phaseLabel || PHASE_LABEL[phase]}
          </motion.span>
        </AnimatePresence>
      </button>

      {/* agents */}
      <AnimatePresence>
        {nodes.map(({ a, x, y }) => <AgentBubble key={a.manifest.id} node={a} x={x} y={y} onClick={() => setInspect({ type: "agent", id: a.manifest.id })} selected={inspect?.type === "agent" && inspect.id === a.manifest.id} />)}
      </AnimatePresence>

      {list.length === 0 && (
        <motion.div className="stage-hint" initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ delay: 1.2, duration: 1 }}
          style={{ left: cx, top: cy + orbit + 28 }}>
          Les agents qu'Orchestra appelle se brancheront ici.
        </motion.div>
      )}

      <AnimatePresence>{inspectorOpen && <Inspector />}</AnimatePresence>
    </div>
  );
}

function AgentBubble({ node, x, y, onClick, selected }: { node: AgentNode; x: number; y: number; onClick: () => void; selected: boolean }) {
  const c = node.manifest.color || "#818cf8";
  const status = node.status;
  const statusText = status === "running" ? "en cours" : status === "done" ? "terminé" : status === "error" ? "erreur" : "branché";
  return (
    <motion.button className={`agent-node ${status} ${selected ? "selected" : ""}`} style={{ left: x, top: y, ["--c" as any]: c }}
      initial={{ opacity: 0, scale: 0.2 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.4 }}
      transition={{ type: "spring", stiffness: 260, damping: 18, delay: 0.5 }} onClick={onClick} title={node.manifest.description}>
      <span className="agent-ring" />
      <span className="agent-glyph"><AgentGlyph icon={node.manifest.icon} size={20} /></span>
      <span className="agent-name">{node.manifest.name}</span>
      <span className="agent-status">{statusText}</span>
      <AnimatePresence>
        {node.toolsUsed.map((t, i) => {
          const ang = -Math.PI / 2 + i * 0.75 - (node.toolsUsed.length - 1) * 0.375;
          const r = 46;
          return (
            <motion.span key={t} className={`tool-chip ${node.lastTool?.tool === t && Date.now() - (node.lastTool?.at ?? 0) < 2500 ? "hot" : ""}`}
              style={{ left: 31 + Math.cos(ang) * r, top: 31 + Math.sin(ang) * r }}
              initial={{ opacity: 0, scale: 0 }} animate={{ opacity: 1, scale: 1 }} transition={{ type: "spring", stiffness: 300, damping: 16 }}>
              <I.tool size={10} /> {t}
            </motion.span>
          );
        })}
      </AnimatePresence>
    </motion.button>
  );
}
