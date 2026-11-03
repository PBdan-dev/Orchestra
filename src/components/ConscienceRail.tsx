import { useStore } from "../store";
import { useConv } from "../conversations";
import { PHASE_COLOR } from "../types";
import { I } from "./Icons";

/** Conscience réduite : une bande étroite qui montre qu'Orchestra travaille, un clic la rouvre. */
export function ConscienceRail() {
  const setConscience = useConv((s) => s.setConscience);
  const busy = useStore((s) => s.busy);
  const phase = useStore((s) => s.phase);
  const alertes = useStore((s) => s.alertesIntegrite.length + s.alertesDonnees.length + s.lecturesAConfirmer.length);
  return (
    <aside className="panel conscience-rail" title="Afficher la Conscience" onClick={() => setConscience("ouverte")}>
      <button className="icon-btn" title="Afficher la Conscience"><I.chevron size={15} /></button>
      <I.brain size={16} />
      <span className="conscience-rail-titre">Conscience</span>
      <span className={`live-dot ${busy ? "on" : ""}`} style={{ ["--c" as any]: PHASE_COLOR[phase] }} />
      {alertes > 0 && <span className="conscience-rail-alerte" title="Quelque chose t'attend dans la Conscience"><I.alert size={13} /></span>}
      <button className="icon-btn xs" title="Masquer la Conscience" onClick={(e) => { e.stopPropagation(); setConscience("masquee"); }}><I.x size={11} /></button>
    </aside>
  );
}

/** Conscience masquée : une languette discrète pour la rappeler (alerte visible si besoin). */
export function LanguetteConscience() {
  const setConscience = useConv((s) => s.setConscience);
  const alertes = useStore((s) => s.alertesIntegrite.length + s.alertesDonnees.length + s.lecturesAConfirmer.length);
  return (
    <button className={`conscience-languette ${alertes > 0 ? "alerte" : ""}`} title="Afficher la Conscience" onClick={() => setConscience("ouverte")}>
      <I.brain size={13} /> {alertes > 0 && <I.alert size={12} />}
    </button>
  );
}
