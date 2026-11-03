import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { brainStatus } from "../brainStatus";
import { PHASE_COLOR, PHASE_LABEL, type AgentNode } from "../types";
import { Core } from "./Core";
import { AgentGlyph, I } from "./Icons";
import { Inspector } from "./Inspector";
import { AgentsPanel } from "./AgentsPanel";
import { ModeSwitch, VoiceControls } from "./ModeSwitch";

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
  const voiceEtape = useStore((s) => s.voiceEtape);
  const voicePause = useStore((s) => s.voicePause);
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
  // le cœur peut occuper presque toute la scène (bouton ⤢), sans dépasser
  const R = Math.max(70, Math.min(minSide * 0.46, minSide * 0.17 * coreScale));
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

  // sous-bulles de tâche : une par agent qui a une mission, chacune dans son propre espace
  const [ouvertes, setOuvertes] = useState<Record<string, boolean>>({});
  const cartes = placerCartes(nodes, placed, { cx, cy, w: usableW, h: size.h, R }, ouvertes);

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

      {/* liens agent → sa sous-bulle de tâche */}
      <svg className="stage-svg" width={size.w} height={size.h} style={{ zIndex: 2 }}>
        {cartes.map((k) => {
          const c = k.p.a.manifest.color || "#818cf8";
          const nr = k.p.depth > 0 ? 25 : NODE_R;
          const tx = Math.max(k.x, Math.min(k.x + k.w, k.p.x)), ty = Math.max(k.y, Math.min(k.y + k.h, k.p.y));
          const ang = Math.atan2(ty - k.p.y, tx - k.p.x);
          const sx = k.p.x + Math.cos(ang) * (nr + 3), sy = k.p.y + Math.sin(ang) * (nr + 3);
          return <line key={k.p.a.manifest.id} x1={sx} y1={sy} x2={tx} y2={ty} stroke={c} strokeOpacity={0.5} strokeWidth={1} strokeDasharray="2 3" />;
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
        {voiceOn && <VoiceControls />}
        {(switching || (voiceOn && voiceConn !== "on")) && <span className="core-note">{switching || (voicePause ? "Voix en pause pendant la mission" : voiceEtape ? `Presque prête : ${voiceEtape}…` : "Je prépare ma voix… (≈ 30 s)")}</span>}
      </div>

      {/* agents */}
      <AnimatePresence>
        {cartes.map((k) => (
          <SousBulle key={`t-${k.p.a.manifest.id}`} carte={k} ouverte={!!ouvertes[k.p.a.manifest.id]}
            basculer={() => setOuvertes((o) => ({ ...o, [k.p.a.manifest.id]: !o[k.p.a.manifest.id] }))} />
        ))}
      </AnimatePresence>
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
        <ModeExecution />
      </div>
      <div className="stage-tools br" style={{ right: inspectorOpen ? 392 : 12 }}>
        <span className="tools-label">Cœur</span>
        <button className="glass-icon" onClick={() => setCoreScale(coreScale - 0.1)} title="Réduire le cœur"><I.minus size={12} /></button>
        <input className="core-slider" type="range" min={0.6} max={2.6} step={0.02} value={coreScale} onChange={(e) => setCoreScale(Number(e.target.value))} title="Taille du cœur (Ctrl + molette)" />
        <button className="glass-icon" onClick={() => setCoreScale(coreScale + 0.1)} title="Agrandir le cœur"><I.plus size={12} /></button>
        <button className={`glass-icon ${coreScale >= 2.5 ? "active" : ""}`} onClick={() => setCoreScale(coreScale >= 2.5 ? 1 : 2.6)}
          title={coreScale >= 2.5 ? "Taille normale" : "Cœur en grand"}>
          {coreScale >= 2.5 ? <I.shrink size={12} /> : <I.expand size={12} />}
        </button>
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
    </motion.div>
  );
}

/** Sous-bulle d'une tâche d'agent : sa mission, puis ses appels d'outils, le plus récent en bas. */
type Carte = { p: Placed; x: number; y: number; w: number; h: number; visibles: number };

const APPELS_REPLIES = 4;
const APPELS_OUVERTS = 12;
const LIGNE_APPEL = 18;

function hauteurCarte(n: AgentNode, ouverte: boolean, w: number) {
  const tache = n.detail || n.reason || "";
  const parLigne = Math.max(18, Math.floor((w - 20) / 6.3));
  const lignesTache = Math.min(ouverte ? 6 : 2, Math.max(1, Math.ceil(tache.length / parLigne)));
  const total = n.appels?.length ?? 0;
  const visibles = Math.min(total, ouverte ? APPELS_OUVERTS : APPELS_REPLIES);
  const plus = total > visibles ? 16 : 0;
  return { h: 30 + lignesTache * 15 + visibles * LIGNE_APPEL + plus + (visibles ? 6 : 0), visibles };
}

/**
 * Place chaque sous-bulle là où elle a le plus de place : d'abord vers l'extérieur (à l'opposé du
 * cœur ou de l'agent parent), sinon sous l'agent, au-dessus, ou du côté intérieur. Une position qui
 * recouvre un agent, le cœur ou une autre bulle, ou qui sort de la scène, est pénalisée.
 */
type Rect = { x: number; y: number; w: number; h: number };
function placerCartes(nodes: Placed[], placed: Record<string, Placed>, scene: { cx: number; cy: number; w: number; h: number; R: number }, ouvertes: Record<string, boolean>): Carte[] {
  const recouvrement = (a: Rect, b: Rect, m = 6) =>
    Math.max(0, Math.min(a.x + a.w, b.x + b.w) + m - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.h, b.y + b.h) + m - Math.max(a.y, b.y));
  const horsScene = (r: Rect) =>
    Math.max(0, 8 - r.x) * r.h + Math.max(0, r.x + r.w - (scene.w - 8)) * r.h + Math.max(0, 8 - r.y) * r.w + Math.max(0, r.y + r.h - (scene.h - 8)) * r.w;
  // obstacles : le cœur, et chaque agent avec son nom et son état en dessous
  const obstacles: Rect[] = [
    { x: scene.cx - scene.R, y: scene.cy - scene.R, w: scene.R * 2, h: scene.R * 2 },
    ...nodes.map((p) => ({ x: p.x - 48, y: p.y - 36, w: 96, h: 84 })),
  ];
  const cartes: Carte[] = [];
  // les agents qui travaillent d'abord : leur bulle prend la meilleure place
  const ordre = [...nodes].sort((a, b) => (a.a.status === "running" ? 0 : 1) - (b.a.status === "running" ? 0 : 1) || a.depth - b.depth);
  for (const p of ordre) {
    const n = p.a;
    if (!n.detail && !(n.appels?.length) && n.status === "attached") continue;
    const sub = p.depth > 0;
    const w = sub ? 178 : 204;
    const { h, visibles } = hauteurCarte(n, !!ouvertes[n.manifest.id], w);
    const o = n.parent && placed[n.parent] ? placed[n.parent] : { x: scene.cx, y: scene.cy };
    let dx = p.x - o.x, dy = p.y - o.y;
    const d = Math.hypot(dx, dy) || 1;
    dx /= d; dy /= d;
    const nr = sub ? 25 : NODE_R;
    const exterieur = dx >= 0 ? p.x + nr + 16 : p.x - nr - 16 - w;
    const interieur = dx >= 0 ? p.x - nr - 16 - w : p.x + nr + 16;
    const candidats: Rect[] = [
      { x: exterieur, y: p.y - h / 2, w, h },
      { x: p.x - w / 2, y: p.y + nr + 44, w, h },
      { x: p.x - w / 2, y: p.y - nr - 12 - h, w, h },
      { x: exterieur, y: p.y - h + nr, w, h },
      { x: exterieur, y: p.y - nr, w, h },
      { x: interieur, y: p.y - h / 2, w, h },
    ];
    // vers le bas ou le haut d'abord quand l'agent est en haut ou en bas de la scène
    if (Math.abs(dy) > 0.82) candidats.unshift(dy > 0 ? candidats[1] : candidats[2]);
    let meilleur = candidats[0], cout = Infinity;
    candidats.forEach((c, i) => {
      const k = horsScene(c) * 3 + obstacles.reduce((t, ob) => t + recouvrement(c, ob), 0) * 2 + cartes.reduce((t, b) => t + recouvrement(c, b), 0) * 2 + i * 40;
      if (k < cout) { cout = k; meilleur = c; }
    });
    const r = { ...meilleur };
    r.x = Math.max(8, Math.min(scene.w - r.w - 8, r.x));
    r.y = Math.max(8, Math.min(scene.h - r.h - 8, r.y));
    cartes.push({ p, ...r, visibles });
  }
  return cartes;
}

function SousBulle({ carte, ouverte, basculer }: { carte: Carte; ouverte: boolean; basculer: () => void }) {
  const n = carte.p.a;
  const c = n.manifest.color || "#818cf8";
  const appels = n.appels ?? [];
  const vus = appels.slice(-carte.visibles);
  const caches = appels.length - vus.length;
  const etat = n.status === "running" ? "en cours" : n.status === "done" ? "terminé" : n.status === "error" ? "erreur" : "en attente";
  const recent = (at: number) => Date.now() - at < 2500;
  return (
    <motion.div className={`sous-bulle ${n.status} ${carte.p.depth > 0 ? "sub" : ""}`} style={{ ["--c" as any]: c, width: carte.w }}
      initial={{ opacity: 0, scale: 0.85, left: carte.x, top: carte.y }} animate={{ opacity: 1, scale: 1, left: carte.x, top: carte.y, height: carte.h }}
      exit={{ opacity: 0, scale: 0.85 }} transition={{ type: "spring", stiffness: 240, damping: 26, opacity: { delay: 0.2 } }}
      onClick={basculer} title={ouverte ? "Replier" : "Déplier la tâche et tous ses appels"}>
      <div className="sb-tete">
        <span className="sb-point" />
        <span className="sb-agent">{n.manifest.name}</span>
        <span className="sb-etat">{etat}</span>
      </div>
      <div className={`sb-tache ${ouverte ? "ouverte" : ""}`}>{n.detail || n.reason || "mission en préparation"}</div>
      {vus.length > 0 && (
        <div className="sb-appels">
          {caches > 0 && <div className="sb-plus">+ {caches} appel(s) avant</div>}
          {vus.map((a, i) => (
            <div key={`${a.at}-${i}`} className={`sb-appel ${recent(a.at) ? "hot" : ""}`} title={a.output}>
              <I.tool size={9} /> <span className="sb-outil">{a.tool}</span>
              {a.output && <span className="sb-sortie">{a.output}</span>}
            </div>
          ))}
        </div>
      )}
    </motion.div>
  );
}

/**
 * Série ou parallèle (retour de PB du 03/11), à la place du bouton Simulation.
 * Série : une mission rend la conversation indisponible, après ton feu vert. Parallèle : la
 * conversation garde son modèle ; refusé (et expliqué) si la carte graphique ne peut pas en porter deux.
 */
function ModeExecution() {
  const mode = useStore((s) => s.config?.mode_execution ?? "serie");
  const choisir = useStore((s) => s.choisirMode);
  const busy = useStore((s) => s.busy);
  const [motif, setMotif] = useState("");
  const [verif, setVerif] = useState(false);
  useEffect(() => { if (!motif) return; const t = setTimeout(() => setMotif(""), 12000); return () => clearTimeout(t); }, [motif]);
  const changer = async (m: "serie" | "parallele") => {
    if (m === mode || verif) return;
    setVerif(true);
    try { setMotif(await choisir(m)); } catch (e) { setMotif(String(e).replace(/^Error: /, "")); } finally { setVerif(false); }
  };
  return (
    <div className="mode-exec">
      <div className="mode-exec-choix glass" role="radiogroup" aria-label="Mode d'exécution des missions">
        <button role="radio" aria-checked={mode !== "parallele"} className={mode !== "parallele" ? "on" : ""} disabled={busy} onClick={() => changer("serie")}
          title="Une mission à la fois ; la conversation attend la fin de la mission (tu confirmes chaque lancement)">Série</button>
        <button role="radio" aria-checked={mode === "parallele"} className={mode === "parallele" ? "on" : ""} disabled={busy} onClick={() => changer("parallele")}
          title="La conversation garde son modèle pendant les missions, si la carte graphique peut porter les deux">{verif ? "Vérification…" : "Parallèle"}</button>
      </div>
      {motif && <div className={`mode-exec-motif ${/impossible|illisible/.test(motif) ? "warn" : ""}`} onClick={() => setMotif("")}>{motif}</div>}
    </div>
  );
}
