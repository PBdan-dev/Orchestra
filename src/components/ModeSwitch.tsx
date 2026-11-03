import { useEffect, useLayoutEffect, useRef, useState } from "react";
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

/** Dans le cœur, en mode voix : couper le micro, et couper la parole quand Orchestra parle. */
export function VoiceControls() {
  const muted = useStore((s) => s.voiceMuted);
  const setMuted = useStore((s) => s.setMuted);
  const phase = useStore((s) => s.phase);
  const stopSpeaking = useStore((s) => s.stopSpeaking);
  const speaking = phase === "speaking";

  // Échap coupe la parole
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && useStore.getState().phase === "speaking") stopSpeaking(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="voice-controls" onClick={(e) => e.stopPropagation()}>
      <button className={`vc-btn mute ${muted ? "on" : ""}`} onClick={() => setMuted(!muted)} aria-pressed={muted}
        title={muted ? "Micro coupé : Orchestra ne t'entend pas. Clique pour le rouvrir." : "Couper le micro"}>
        {muted ? <I.micOff size={13} /> : <I.mic size={13} />} {muted ? "Micro coupé" : "Micro ouvert"}
      </button>
      {speaking && (
        <button className="vc-btn stop" onClick={stopSpeaking} title="Couper la parole (Échap)">
          <I.stop size={11} /> Stop
        </button>
      )}
      <ChaineVoix />
    </div>
  );
}

/**
 * Chaque maillon de la voix avec son état : écoute (Voxtral) et synthèse vivent dans le serveur
 * voix, le cerveau vocal et le rappel par le sens dans Ollama. Vert : prêt ; ambre : en chargement ;
 * gris : arrêté.
 */
function ChaineVoix() {
  const e = useStore((s) => s.voiceEtat);
  const conn = useStore((s) => s.voiceConn);
  const pause = useStore((s) => s.voicePause);
  const etape = useStore((s) => s.voiceEtape);
  const relancer = useStore((s) => s.relancerVoix);
  if (!e) return null;
  type Etat = "ok" | "charge" | "arret";
  const serveur: Etat = !e.serveur ? "arret" : conn === "on" ? "ok" : "charge";
  const cerveau: Etat = e.cerveau_charge ? (serveur === "arret" ? "charge" : "ok") : e.serveur || etape ? "charge" : "arret";
  const maillons: [string, Etat, string][] = [
    ["Écoute", serveur, "Voxtral, dans le serveur voix : transcrit ce que tu dis"],
    ["Cerveau", cerveau, `${e.cerveau}, dans Ollama${e.cerveau_charge ? " : chargé" : " : pas chargé"}`],
    ["Voix", serveur, "Synthèse vocale, dans le serveur voix"],
    ...(e.memoire ? [["Mémoire", e.memoire_chargee ? "ok" : "arret", `${e.memoire} (rappel par le sens), dans Ollama${e.memoire_chargee ? "" : " : pas chargé, le rappel passe par les mots"}`] as [string, Etat, string]] : []),
  ];
  const libelle = { ok: "prêt", charge: "en chargement", arret: "arrêté" };
  return (
    <div className="chaine-voix" title={pause ? `Voix en pause : ${pause}` : undefined}>
      {maillons.map(([nom, etat, detail]) => (
        <span key={nom} className={`cv-maillon ${pause && etat !== "ok" ? "pause" : etat}`} title={`${nom} : ${pause && etat === "arret" ? "en pause" : libelle[etat]} · ${detail}`}>
          <span className="cv-point" />{nom}
        </span>
      ))}
      {serveur === "arret" && !pause && <button className="vc-btn" onClick={relancer} title="Arrête et relance le serveur voix"><I.refresh size={11} /> Relancer la voix</button>}
    </div>
  );
}
