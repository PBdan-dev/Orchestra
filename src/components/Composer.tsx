import { useEffect, useRef, useState } from "react";
import { useStore } from "../store";
import { I } from "./Icons";

export function Composer() {
  const [text, setText] = useState("");
  const send = useStore((s) => s.send);
  const listening = useStore((s) => s.listening);
  const busy = useStore((s) => s.busy);
  const brain = useStore((s) => s.brain);
  const setView = useStore((s) => s.setView);
  const phase = useStore((s) => s.phase);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = Math.min(160, el.scrollHeight) + "px";
  }, [text]);

  // retour en veille quand on arrête d'écrire
  useEffect(() => {
    if (phase !== "listening") return;
    const t = setTimeout(() => { if (!useStore.getState().busy) useStore.setState({ phase: "idle", phaseLabel: "" }); }, 1800);
    return () => clearTimeout(t);
  }, [text, phase]);

  const submit = () => {
    if (!text.trim() || busy) return;
    send(text);
    setText("");
  };

  const offline = brain && !brain.online;

  return (
    <div className="composer">
      <div className="composer-inner">
        <textarea ref={ref} value={text} rows={1} placeholder={busy ? "Orchestra travaille…" : "Parle à Orchestra…"}
          onChange={(e) => { setText(e.target.value); listening(); }}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); } }} />
        <button className="send-btn" onClick={submit} disabled={!text.trim() || busy} title="Envoyer (Entrée)"><I.send size={18} /></button>
      </div>
      <div className="composer-foot">
        {offline ? (
          <button className="link-btn warn" onClick={() => setView("settings")}><I.alert size={12} /> Cerveau hors ligne : {brain?.detail} — ouvrir les paramètres</button>
        ) : (
          <span className="muted small">{brain ? <>● {brain.provider} · {brain.model}</> : "détection du cerveau…"} · Entrée pour envoyer, Maj+Entrée pour une nouvelle ligne</span>
        )}
      </div>
    </div>
  );
}
