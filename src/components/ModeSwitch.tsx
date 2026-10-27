import { useLayoutEffect, useRef, useState } from "react";
import { useStore } from "../store";
import { I } from "./Icons";

/**
 * Switch Chat / Voix, dans le cœur, sous l'état.
 * Voix : micro ouvert en continu, cerveau vocal rapide, réponses dites à voix haute.
 * Chat : réponses écrites par la boucle centrale, réflexion activée.
 * Changer de mode décharge le cerveau de l'autre mode et charge le sien.
 */
export function ModeSwitch() {
  const voiceOn = useStore((s) => s.voiceOn);
  const voiceConn = useStore((s) => s.voiceConn);
  const status = useStore((s) => s.voiceStatus);
  const toggleVoice = useStore((s) => s.toggleVoice);
  const [pending, setPending] = useState(false);
  const chat = useRef<HTMLButtonElement>(null);
  const vox = useRef<HTMLButtonElement>(null);
  const [thumb, setThumb] = useState({ left: 3, width: 58 });

  const mode = voiceOn ? "voix" : "chat";
  // position du surlignage : mesurée à chaque changement de taille (retour sur la scène, police chargée,
  // taille du cœur), pas seulement au changement de mode — sinon elle peut être prise quand la scène est cachée
  useLayoutEffect(() => {
    const measure = () => {
      const el = voiceOn ? vox.current : chat.current;
      if (el && el.offsetWidth > 0) setThumb({ left: el.offsetLeft, width: el.offsetWidth });
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (chat.current) ro.observe(chat.current);
    if (vox.current) ro.observe(vox.current);
    document.fonts?.ready.then(measure).catch(() => {});
    return () => ro.disconnect();
  }, [voiceOn]);

  const choose = async (m: "chat" | "voix", e: React.MouseEvent) => {
    e.stopPropagation(); // ne pas ouvrir le panneau du cœur
    if (m === mode || pending) return;
    setPending(true);
    await toggleVoice();
    setPending(false);
  };

  const installed = status?.installed !== false;

  return (
    <div className={`mode-switch ${mode} ${pending ? "pending" : ""}`} role="radiogroup" aria-label="Mode de conversation">
      <span className="mode-thumb" style={{ left: thumb.left, width: thumb.width }} />
      <button ref={chat} className={mode === "chat" ? "on" : ""} role="radio" aria-checked={mode === "chat"} onClick={(e) => choose("chat", e)}
        title="Chat : réponses écrites, avec réflexion approfondie">
        <I.chat size={12} /> Chat
      </button>
      <button ref={vox} className={mode === "voix" ? "on" : ""} role="radio" aria-checked={mode === "voix"} onClick={(e) => choose("voix", e)}
        disabled={!installed} title={installed ? "Voix : conversation orale continue, réponses rapides" : `Module voix incomplet : ${status?.missing.join(", ")}`}>
        {mode === "voix" && voiceConn === "on" ? <span className="mode-dot" /> : <I.mic size={12} />} Voix
      </button>
    </div>
  );
}
