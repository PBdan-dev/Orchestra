import { useLayoutEffect, useRef, useState } from "react";
import { useStore } from "../store";
import { I } from "./Icons";

/**
 * Switch Chat / Voix, centré sous le cœur.
 * Voix : micro ouvert en continu, cerveau rapide sans réflexion, réponses dites à voix haute.
 * Chat : réponses écrites par la boucle centrale, réflexion activée.
 */
export function ModeSwitch({ x, y }: { x: number; y: number }) {
  const voiceOn = useStore((s) => s.voiceOn);
  const voiceConn = useStore((s) => s.voiceConn);
  const status = useStore((s) => s.voiceStatus);
  const toggleVoice = useStore((s) => s.toggleVoice);
  const [pending, setPending] = useState(false);
  const chat = useRef<HTMLButtonElement>(null);
  const vox = useRef<HTMLButtonElement>(null);
  const [thumb, setThumb] = useState({ left: 3, width: 60 });

  const mode = voiceOn ? "voix" : "chat";
  useLayoutEffect(() => {
    const el = voiceOn ? vox.current : chat.current;
    if (el) setThumb({ left: el.offsetLeft, width: el.offsetWidth });
  }, [voiceOn]);

  const choose = async (m: "chat" | "voix") => {
    if (m === mode || pending) return;
    setPending(true);
    await toggleVoice();
    setPending(false);
  };

  const installed = status?.installed !== false;
  const note = voiceOn && voiceConn !== "on" ? "Je prépare ma voix… (≈ 30 s la première fois)" : "";

  return (
    <>
      <div className={`mode-switch ${mode}`} style={{ left: x, top: y }} role="radiogroup" aria-label="Mode de conversation">
        <span className="mode-thumb" style={{ left: thumb.left, width: thumb.width }} />
        <button ref={chat} className={mode === "chat" ? "on" : ""} role="radio" aria-checked={mode === "chat"} onClick={() => choose("chat")}
          title="Chat : réponses écrites, avec réflexion approfondie">
          <I.chat size={13} /> Chat
        </button>
        <button ref={vox} className={mode === "voix" ? "on" : ""} role="radio" aria-checked={mode === "voix"} onClick={() => choose("voix")}
          disabled={!installed} title={installed ? "Voix : conversation orale continue, réponses rapides" : `Module voix incomplet : ${status?.missing.join(", ")}`}>
          {mode === "voix" && voiceConn === "on" ? <span className="mode-dot" /> : <I.mic size={13} />} Voix
        </button>
      </div>
      {note && <div className="mode-note" style={{ left: x, top: y + 22 }}>{note}</div>}
    </>
  );
}
