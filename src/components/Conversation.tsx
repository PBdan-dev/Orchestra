import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore, voice } from "../store";
import { Markdown } from "../markdown";
import { AgentGlyph, I } from "./Icons";
import type { AgentPropose } from "../types";

const SUGGESTIONS = [
  "Présente-toi en trois phrases.",
  "Qu'est-ce que tu retiens de moi pour l'instant ?",
  "Rédige une note de bienvenue pour Orchestra et enregistre-la dans un fichier bienvenue.md.",
  "Fais l'inventaire de ton espace de travail.",
];

export function Conversation() {
  const messages = useStore((s) => s.messages);
  const send = useStore((s) => s.send);
  const clear = useStore((s) => s.clearConversation);
  const busy = useStore((s) => s.busy);
  const voiceOn = useStore((s) => s.voiceOn);
  const file = useStore((s) => s.fileAttente);
  const retirerDeLaFile = useStore((s) => s.retirerDeLaFile);
  const proposes = useStore((s) => s.agentsProposes);
  const lancements = useStore((s) => s.lancements);
  const confirmerLancement = useStore((s) => s.confirmerLancement);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, proposes, lancements]);

  return (
    <section className="panel conversation">
      <header className="panel-head">
        <div className="panel-title"><I.chat size={15} /> Conversation</div>
        <div className="panel-meta">
          {messages.length > 0 && <button className="icon-btn" onClick={clear} disabled={busy} title="Nouvelle conversation (la mémoire est conservée)"><I.trash size={14} /></button>}
        </div>
      </header>
      <div className="panel-body messages" ref={ref}>
        {messages.length === 0 && (
          <div className="empty">
            <p>Bonjour. Je suis Orchestra.</p>
            <p className="muted">Discute avec moi, pose-moi une question, ou confie-moi un travail : je trouverai l'agent qu'il faut et je le superviserai.</p>
            <div className="suggestions">
              {SUGGESTIONS.map((s) => <button key={s} className="suggestion" onClick={() => send(s)}>{s}</button>)}
            </div>
          </div>
        )}
        <AnimatePresence initial={false}>
          {messages.map((m) => (
            <motion.div key={m.id} className={`msg ${m.role} ${m.note ? "note" : ""}`} initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.25 }}>
              {m.role === "assistant" && (
                <div className="msg-meta">
                  <span className="who">Orchestra</span>
                  {m.intent && <span className="chip ghost tiny">{m.intent}</span>}
                  {m.agent && <span className="chip tiny"><I.plug size={10} /> {m.agent}</span>}
                  {m.voice && <span className="voice-tag" title="dit à voix haute"><I.mic size={10} /></span>}
                  {voiceOn && !m.streaming && m.content && <button className="icon-btn" title="Écouter" onClick={() => voice.say(m.content)}><I.speaker size={12} /></button>}
                </div>
              )}
              <div className="bubble">
                {m.role === "user" ? m.content : m.content ? <Markdown text={m.content} /> : null}
                {m.streaming && <span className="caret big" />}
              </div>
            </motion.div>
          ))}
        </AnimatePresence>
        {proposes.map((p) => <CarteInstallation key={p.run_id} p={p} />)}
        {lancements.map((l) => (
          <motion.div key={l.cle} className="carte-lancement" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}>
            <div className="ci-titre"><b><I.play size={11} /> Lancer la mission de {l.agent_nom} ?</b></div>
            <p className="ci-desc">{l.but}</p>
            <p className="small muted">Mode série : pendant la mission, je ne pourrai plus discuter{l.change_modele ? ` (je prête la carte graphique à ${l.modele}${voiceOn ? ", la voix se met en pause" : ""})` : ""}. Je te préviens quand elle est finie.</p>
            <div className="ci-actions">
              <button className="glass-btn sm active" onClick={() => confirmerLancement(l.cle, true)}><I.play size={11} /> Lancer</button>
              <button className="glass-btn sm" onClick={() => confirmerLancement(l.cle, false)}>Pas maintenant</button>
            </div>
          </motion.div>
        ))}
        {file.length > 0 && (
          <div className="file-attente">
            <div className="muted">En attente ({file.length}) : envoyées une à une quand Orchestra aura fini</div>
            {file.map((t, i) => (
              <div key={i} className="fa-item"><span>{i + 1}. {t}</span><button className="link-btn" onClick={() => retirerDeLaFile(i)}>retirer</button></div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

/**
 * Un agent de l'Agent Market sait faire la demande, mais il n'est pas installé : Orchestra
 * n'installe jamais seule. Un clic l'installe (vérifications du lot T4), un autre relance la demande.
 */
function CarteInstallation({ p }: { p: AgentPropose }) {
  const installer = useStore((s) => s.installerPropose);
  const oublier = useStore((s) => s.oublierPropose);
  const send = useStore((s) => s.send);
  const busy = useStore((s) => s.busy);
  const voirAgent = useStore((s) => s.voirAgent);
  const a = p.agent;
  return (
    <motion.div className="carte-install" style={{ ["--c" as any]: a.color || "#818cf8" }} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}>
      <div className="ci-tete">
        <span className="ci-glyph"><AgentGlyph icon={a.icon} size={16} /></span>
        <div className="ci-titre">
          <div><b>{a.name}</b> <span className="muted small">{a.version}</span></div>
          <div className="muted small">sur l'Agent Market · {a.certification === "orchestra_certified" ? "Orchestra Certified" : "non certifié"}</div>
        </div>
        {p.etat !== "installation" && <button className="icon-btn" title="Fermer" onClick={() => oublier(p.run_id)}><I.x size={12} /></button>}
      </div>
      <p className="ci-desc">{a.description}</p>
      {p.etat === "propose" && (
        <div className="ci-actions">
          <button className="glass-btn sm active" onClick={() => installer(p.run_id)}><I.plug size={11} /> Installer</button>
          <button className="link-btn" onClick={() => voirAgent(a.id)}>voir la fiche</button>
        </div>
      )}
      {p.etat === "installation" && <div className="small muted">Vérification et installation…</div>}
      {p.etat === "installe" && (
        <div className="ci-actions">
          <span className="small ok"><I.check size={11} /> Installé.</span>
          {p.detail && <span className="small warn">{p.detail}</span>}
          <button className="glass-btn sm active" disabled={busy} onClick={() => { oublier(p.run_id); send(p.demande); }}>Relancer ma demande</button>
        </div>
      )}
      {p.etat === "erreur" && <div className="small warn">Installation refusée : {p.detail}</div>}
    </motion.div>
  );
}
