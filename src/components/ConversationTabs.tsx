import { useState } from "react";
import { useStore } from "../store";
import { cheminDossier, projetDe, titreOnglet, useConv } from "../conversations";
import { I } from "./Icons";

/** Onglets des conversations ouvertes, au-dessus de la conversation. */
export function ConversationTabs() {
  const onglets = useConv((s) => s.org.onglets);
  const org = useConv((s) => s.org);
  const courante = useConv((s) => s.courante);
  const ouvrir = useConv((s) => s.ouvrir);
  const fermer = useConv((s) => s.fermerOnglet);
  const nouvelle = useConv((s) => s.nouvelle);
  const modifier = useConv((s) => s.modifierConv);
  const busy = useStore((s) => s.busy);
  useStore((s) => s.messages);
  useConv((s) => s.fiches);
  const [edition, setEdition] = useState<string | null>(null);
  const [valeur, setValeur] = useState("");
  const contexte = courante?.contexte_dossier;
  const projet = courante ? projetDe(org, courante.dossier) : null;

  return (
    <div className="conv-onglets-zone">
      <div className="conv-onglets" role="tablist">
        {onglets.map((id) => {
          const t = titreOnglet(id);
          const actif = courante?.id === id;
          return (
            <div key={id} role="tab" aria-selected={actif} className={`conv-onglet ${actif ? "actif" : ""}`} title={busy && !actif ? "Orchestra travaille : attends la fin du tour" : t} onClick={() => ouvrir(id)} onDoubleClick={() => { setEdition(id); setValeur(t); }}>
              {edition === id ? (
                <input
                  className="conv-saisie"
                  autoFocus
                  value={valeur}
                  onChange={(e) => setValeur(e.target.value)}
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") { void modifier(id, { titre: valeur }); setEdition(null); }
                    if (e.key === "Escape") setEdition(null);
                  }}
                  onBlur={() => { if (valeur.trim() && valeur.trim() !== t) void modifier(id, { titre: valeur }); setEdition(null); }}
                />
              ) : (
                <span className="conv-onglet-titre">{t}</span>
              )}
              <button className="conv-onglet-x" title="Fermer l'onglet (la conversation reste dans l'historique)" onClick={(e) => { e.stopPropagation(); void fermer(id); }}><I.x size={10} /></button>
            </div>
          );
        })}
        <button className="conv-onglet nouveau" title="Nouvelle conversation" disabled={busy} onClick={() => nouvelle(courante?.dossier ?? null)}><I.plus size={12} /></button>
      </div>
      {courante && (courante.dossier || contexte) && (
        <div className="conv-fil small">
          {courante.dossier && <span><I.folder size={11} /> {cheminDossier(org, courante.dossier)}</span>}
          {projet && <span className="conv-projet">projet {projet}</span>}
          {contexte && <span className="conv-contexte" title="Orchestra a le contenu de ce dossier en contexte"><I.search size={11} /> lit le dossier « {cheminDossier(org, contexte)} »</span>}
        </div>
      )}
    </div>
  );
}
