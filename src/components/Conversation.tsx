import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useStore, voice } from "../store";
import { Markdown } from "../markdown";
import { I } from "./Icons";

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
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

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
