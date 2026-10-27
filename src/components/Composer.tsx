import { useEffect, useRef, useState } from "react";
import { useStore } from "../store";
import { brainStatus } from "../brainStatus";
import { I } from "./Icons";

/** Zone de saisie, centrée sous le cœur. La poignée du haut permet de l'agrandir (le cœur remonte). */
export function Composer() {
  const [text, setText] = useState("");
  const send = useStore((s) => s.send);
  const listening = useStore((s) => s.listening);
  const busy = useStore((s) => s.busy);
  const brain = useStore((s) => s.brain);
  const brainLoading = useStore((s) => s.brainLoading);
  const loadModel = useStore((s) => s.loadModel);
  const setView = useStore((s) => s.setView);
  const phase = useStore((s) => s.phase);
  const height = useStore((s) => s.composerHeight);
  const setHeight = useStore((s) => s.setComposerHeight);
  const config = useStore((s) => s.config);
  const voiceOn = useStore((s) => s.voiceOn);
  const voiceConn = useStore((s) => s.voiceConn);
  const voiceUser = useStore((s) => s.voiceUser);
  const voiceHeld = useStore((s) => s.voiceHeld);
  const voiceInterim = useStore((s) => s.voiceInterim);
  const voiceError = useStore((s) => s.voiceError);
  const voiceLatency = useStore((s) => s.voiceLatency);
  const toggleVoice = useStore((s) => s.toggleVoice);
  const holdVoice = useStore((s) => s.holdVoice);
  const stopSpeaking = useStore((s) => s.stopSpeaking);
  const ref = useRef<HTMLTextAreaElement>(null);
  const ptt = config?.voice_mode === "appuyer";
  const pttKey = config?.voice_ptt_key || "F2";

  // appuyer-pour-parler au clavier : maintenir la touche, relâcher pour envoyer
  useEffect(() => {
    if (!voiceOn || !ptt) return;
    const down = (e: KeyboardEvent) => { if (e.code === pttKey && !e.repeat) { e.preventDefault(); holdVoice(true); } };
    const up = (e: KeyboardEvent) => { if (e.code === pttKey) { e.preventDefault(); holdVoice(false); } };
    const blur = () => holdVoice(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => { window.removeEventListener("keydown", down); window.removeEventListener("keyup", up); window.removeEventListener("blur", blur); };
  }, [voiceOn, ptt, pttKey]);
  const drag = useRef<{ y: number; h: number } | null>(null);

  // retour en veille quand on arrête d'écrire
  useEffect(() => {
    if (phase !== "listening") return;
    const t = setTimeout(() => { if (!useStore.getState().busy) useStore.setState({ phase: "idle", phaseLabel: "" }); }, 1800);
    return () => clearTimeout(t);
  }, [text, phase]);

  // agrandissement automatique tant que l'utilisateur n'a pas fixé une grande taille
  useEffect(() => {
    const el = ref.current;
    if (!el || drag.current || !text) return;
    // mesure la hauteur réelle du texte (le textarea est étiré par défaut)
    el.style.height = "0px";
    const content = el.scrollHeight;
    el.style.height = "";
    const needed = Math.min(260, content + 22);
    if (needed > height) setHeight(needed);
  }, [text]);

  const submit = () => {
    if (!text.trim() || busy) return;
    send(text);
    setText("");
  };

  const onDown = (e: React.PointerEvent) => {
    drag.current = { y: e.clientY, h: height };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    if (!drag.current) return;
    setHeight(drag.current.h + (drag.current.y - e.clientY));
  };
  const onUp = () => { drag.current = null; };

  const status = brainStatus(brain, brainLoading);
  const voiceEnabled = !!config?.voice_enabled;
  const speaking = phase === "speaking" && voiceOn;
  const voiceHint = !voiceOn ? "" :
    voiceConn !== "on" ? "Je prépare ma voix… (premier lancement : environ 30 s)" :
    voiceInterim ? voiceInterim :
    voiceUser ? "Je t'écoute…" :
    ptt ? (voiceHeld ? "Je t'écoute… relâche pour envoyer" : `Maintiens ${pttKey} ou le micro pour parler`) :
    "Parle quand tu veux, je t'écoute";

  return (
    <div className="composer" style={{ height: height + 34 }}>
      <div className="composer-grip" onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onDoubleClick={() => setHeight(height > 120 ? 64 : 260)} title="Glisser pour agrandir la zone de saisie (double-clic : agrandir/réduire)">
        <span />
      </div>
      {(voiceOn || voiceError) && (
        <div className={`voice-hint ${voiceError ? "warn" : ""} ${voiceUser || voiceHeld ? "hot" : ""}`}>
          <span className="voice-dot" />
          <span className="voice-hint-text">{voiceError || voiceHint}</span>
          {speaking && <button className="link-btn" onClick={stopSpeaking}>couper la parole</button>}
          {voiceLatency && !voiceError && <span className="muted small" title={`fin de parole ${voiceLatency.vad_ms} ms · écoute ${voiceLatency.stt_ms ?? "?"} ms · cerveau ${voiceLatency.llm_ms ?? "?"} ms · voix ${voiceLatency.tts_ms ?? "?"} ms`}>· réponse en {(voiceLatency.total_ms / 1000).toFixed(2)} s</span>}
        </div>
      )}
      <div className="composer-inner glass">
        <textarea ref={ref} value={text} placeholder={busy ? "Orchestra travaille…" : "Parle à Orchestra…"}
          onChange={(e) => { setText(e.target.value); listening(); }}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); } }} />
        {voiceEnabled && (
          <button className={`mic-btn ${voiceOn ? "on" : ""} ${voiceUser || voiceHeld ? "hot" : ""}`}
            onClick={() => { if (!ptt || !voiceOn) toggleVoice(); }}
            onPointerDown={() => { if (ptt && voiceOn) holdVoice(true); }}
            onPointerUp={() => holdVoice(false)} onPointerLeave={() => holdVoice(false)}
            onContextMenu={(e) => { e.preventDefault(); if (voiceOn) toggleVoice(); }}
            title={!voiceOn ? "Parler à Orchestra (mode vocal)" : ptt ? `Maintenir pour parler (ou ${pttKey}) · clic droit : fermer le mode vocal` : "Fermer le mode vocal"}>
            <I.mic size={18} />
          </button>
        )}
        <button className="send-btn" onClick={submit} disabled={!text.trim() || busy} title="Envoyer (Entrée)"><I.send size={18} /></button>
      </div>
      <div className="composer-foot">
        <span className="foot-status" style={{ ["--c" as any]: status.color }}><span className="pill-dot" /> {status.label}{brain?.model ? ` · ${brain.model}` : ""}</span>
        {status.action === "load" && !busy && <button className="link-btn" onClick={loadModel}><I.power size={11} /> charger maintenant</button>}
        {status.tone === "off" && <button className="link-btn warn" onClick={() => setView("settings")}>paramètres</button>}
        <span className="muted small hide-narrow">· Entrée pour envoyer · Maj+Entrée : nouvelle ligne</span>
      </div>
    </div>
  );
}
