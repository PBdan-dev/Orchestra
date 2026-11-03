import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion } from "framer-motion";
import { useStore } from "../store";
import { cheminDossier, descendants, projetDe, themesHerites, titreOnglet, useConv } from "../conversations";
import type { Dossier, Fiche, Organisation, Theme } from "../conversationsApi";
import { I } from "./Icons";

/** Panneau latéral : historique des conversations, dossiers (imbriqués) et thèmes. */
export function ConversationsPanel() {
  const panneau = useConv((s) => s.panneau);
  const setPanneau = useConv((s) => s.setPanneau);
  const nouvelle = useConv((s) => s.nouvelle);
  const init = useConv((s) => s.init);
  const bootDone = useStore((s) => s.bootDone);

  useEffect(() => {
    if (bootDone) void init();
  }, [bootDone]);

  if (panneau === "reduit") {
    return (
      <aside className="panel conv-rail" aria-label="Conversations (réduit)">
        <button className="icon-btn" title="Afficher les conversations" onClick={() => setPanneau("ouvert")}><I.chevron size={15} /></button>
        <button className="icon-btn" title="Nouvelle conversation" onClick={() => nouvelle()}><I.plus size={15} /></button>
        <OngletsRail />
      </aside>
    );
  }
  return <PanneauOuvert />;
}

function OngletsRail() {
  const onglets = useConv((s) => s.org.onglets);
  const active = useConv((s) => s.courante?.id);
  const ouvrir = useConv((s) => s.ouvrir);
  useStore((s) => s.messages);
  useConv((s) => s.fiches);
  return (
    <div className="conv-rail-onglets">
      {onglets.map((id) => {
        const t = titreOnglet(id);
        return <button key={id} className={`conv-rail-pastille ${id === active ? "active" : ""}`} title={t} onClick={() => ouvrir(id)}>{t.slice(0, 1).toUpperCase()}</button>;
      })}
    </div>
  );
}

type Glisse = { type: "conv" | "dossier"; id: string; nom: string; x: number; y: number; sx: number; sy: number; actif: boolean };

function PanneauOuvert() {
  const org = useConv((s) => s.org);
  const fiches = useConv((s) => s.fiches);
  const courante = useConv((s) => s.courante);
  const filtre = useConv((s) => s.filtre);
  const recherche = useConv((s) => s.recherche);
  const setRecherche = useConv((s) => s.setRecherche);
  const erreur = useConv((s) => s.erreur);
  const effacerErreur = useConv((s) => s.effacerErreur);
  const illisibles = useConv((s) => s.illisibles);
  const setPanneau = useConv((s) => s.setPanneau);
  const nouvelle = useConv((s) => s.nouvelle);
  const creerDossier = useConv((s) => s.creerDossier);
  const modifierConv = useConv((s) => s.modifierConv);
  const deplacerDossier = useConv((s) => s.deplacerDossier);
  const apercu = useConv((s) => s.apercu);
  useStore((s) => s.messages); // titre de la conversation affichée
  const [nouveauDossier, setNouveauDossier] = useState<string | null>(null);
  const [glisse, setGlisse] = useState<Glisse | null>(null);
  const [cible, setCible] = useState<string | null>(null);
  const glisseRef = useRef<Glisse | null>(null);

  // la conversation affichée apparaît dans la liste, même avant son premier enregistrement
  const toutes: Fiche[] = useMemo(() => {
    if (!courante) return fiches;
    const f: Fiche = { id: courante.id, titre: titreOnglet(courante.id), cree_le: courante.cree_le, modifie_le: courante.modifie_le, themes: courante.themes, dossier: courante.dossier, contexte_dossier: courante.contexte_dossier, nb_messages: 0, apercu: fiches.find((x) => x.id === courante.id)?.apercu ?? "" };
    return [f, ...fiches.filter((x) => x.id !== courante.id)].sort((a, b) => b.modifie_le - a.modifie_le);
  }, [fiches, courante, useStore.getState().messages]);

  const q = recherche.trim().toLowerCase();
  const visible = (f: Fiche) => {
    if (q && !f.titre.toLowerCase().includes(q) && !f.apercu.toLowerCase().includes(q)) return false;
    if (filtre.length === 0) return true;
    const th = themesHerites(org, f);
    return filtre.some((t) => th.has(t));
  };
  const visibles = toutes.filter(visible);
  const existe = new Set(org.dossiers.map((d) => d.id));
  const filtreActif = filtre.length > 0 || q.length > 0;

  // ---------- glisser-déposer (souris) : conversation ou dossier vers un dossier ----------
  const cibleRef = useRef<string | null>(null);
  const vientDeGlisser = useRef(false);
  useEffect(() => {
    if (!glisse) return;
    const bouge = (e: PointerEvent) => {
      const g = glisseRef.current;
      if (!g) return;
      const actif = g.actif || Math.hypot(e.clientX - g.sx, e.clientY - g.sy) > 6;
      const n = { ...g, x: e.clientX, y: e.clientY, actif };
      glisseRef.current = n;
      setGlisse(n);
      const el = actif ? (document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-depot]") as HTMLElement | null) : null;
      cibleRef.current = el?.dataset.depot ?? null;
      setCible(cibleRef.current);
    };
    const lache = () => {
      const g = glisseRef.current;
      const c = cibleRef.current;
      glisseRef.current = null;
      cibleRef.current = null;
      setGlisse(null);
      setCible(null);
      if (!g?.actif) return;
      // le clic qui suit le lâcher n'ouvre rien
      vientDeGlisser.current = true;
      setTimeout(() => { vientDeGlisser.current = false; }, 0);
      if (!c) return;
      const dest = c === "racine" ? null : c;
      if (g.type === "conv") void modifierConv(g.id, { dossier: dest });
      else if (g.id !== dest) void deplacerDossier(g.id, dest);
    };
    window.addEventListener("pointermove", bouge);
    window.addEventListener("pointerup", lache);
    return () => { window.removeEventListener("pointermove", bouge); window.removeEventListener("pointerup", lache); };
  }, [glisse !== null]);

  const commencer = (type: Glisse["type"], id: string, nom: string) => (e: React.PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, input, .conv-menu")) return;
    const g: Glisse = { type, id, nom, x: e.clientX, y: e.clientY, sx: e.clientX, sy: e.clientY, actif: false };
    glisseRef.current = g;
    setGlisse(g);
  };
  const ctx: Ctx = { org, visibles, filtreActif, cible, commencer, cliquable: () => !vientDeGlisser.current, courante: courante?.id ?? null };

  return (
    <aside className="panel conv-panel">
      <header className="panel-head">
        <div className="panel-title"><I.chat size={15} /> Conversations</div>
        <div className="panel-meta">
          <button className="icon-btn" title="Nouvelle conversation" onClick={() => nouvelle()}><I.plus size={14} /></button>
          <button className="icon-btn" title="Nouveau dossier" onClick={() => setNouveauDossier("")}><I.folder size={14} /></button>
          <button className="icon-btn" title="Réduire le panneau" onClick={() => setPanneau("reduit")}><I.chevron size={14} style={{ transform: "rotate(180deg)" }} /></button>
        </div>
      </header>
      <div className="conv-outils">
        <div className="conv-recherche">
          <I.search size={12} />
          <input value={recherche} onChange={(e) => setRecherche(e.target.value)} placeholder="Chercher une conversation" />
          {recherche && <button className="icon-btn xs" title="Effacer" onClick={() => setRecherche("")}><I.x size={11} /></button>}
        </div>
        <BarreThemes />
      </div>
      {erreur && <div className="conv-erreur small"><I.alert size={12} /> {erreur} <button className="link-btn" onClick={effacerErreur}>ok</button></div>}
      {illisibles.length > 0 && <div className="conv-erreur small"><I.alert size={12} /> Fichier(s) illisible(s), laissé(s) en place : {illisibles.join(", ")}</div>}
      <div className="panel-body conv-arbre">
        {nouveauDossier !== null && (
          <SaisieNom placeholder="Nom du dossier" initial="" onValider={(n) => { void creerDossier(n, null); setNouveauDossier(null); }} onAnnuler={() => setNouveauDossier(null)} />
        )}
        {org.dossiers.filter((d) => !d.parent || !existe.has(d.parent)).sort(parNom).map((d) => <LigneDossier key={d.id} d={d} niveau={0} ctx={ctx} />)}
        <div className={`conv-section ${cible === "racine" ? "cible" : ""}`} data-depot="racine">
          <span>Historique</span><span className="muted">sans dossier</span>
        </div>
        {visibles.filter((f) => !f.dossier || !existe.has(f.dossier)).map((f) => <LigneConv key={f.id} f={f} niveau={0} ctx={ctx} />)}
        {toutes.length === 0 && <div className="muted small conv-vide">Aucune conversation pour l'instant.</div>}
        {toutes.length > 0 && visibles.length === 0 && <div className="muted small conv-vide">Aucune conversation ne correspond au filtre.</div>}
      </div>
      {glisse?.actif && <div className="conv-fantome" style={{ left: glisse.x + 12, top: glisse.y + 8 }}>{glisse.type === "dossier" ? <I.folder size={12} /> : <I.chat size={12} />} {glisse.nom}</div>}
      {createPortal(<AnimatePresence>{apercu && <ApercuContexte key={apercu.dossier} apercu={apercu} />}</AnimatePresence>, document.body)}
    </aside>
  );
}

const parNom = (a: Dossier, b: Dossier) => a.nom.localeCompare(b.nom, "fr");

type Ctx = {
  org: Organisation;
  visibles: Fiche[];
  filtreActif: boolean;
  cible: string | null;
  cliquable: () => boolean;
  courante: string | null;
  commencer: (type: "conv" | "dossier", id: string, nom: string) => (e: React.PointerEvent) => void;
};

/** Le dossier a-t-il quelque chose à montrer avec le filtre actuel ? */
function contientVisible(ctx: Ctx, d: Dossier): boolean {
  const ids = descendants(ctx.org, d.id);
  return ctx.visibles.some((f) => f.dossier && ids.has(f.dossier));
}

function LigneDossier({ d, niveau, ctx }: { d: Dossier; niveau: number; ctx: Ctx }) {
  const deplies = useConv((s) => s.deplies);
  const deplier = useConv((s) => s.deplier);
  const renommer = useConv((s) => s.renommerDossier);
  const creerDossier = useConv((s) => s.creerDossier);
  const [edition, setEdition] = useState<"nom" | "sous" | null>(null);
  const [menu, setMenu] = useState(false);
  if (ctx.filtreActif && !contientVisible(ctx, d)) return null;
  const ouvert = ctx.filtreActif || !!deplies[d.id];
  const enfants = ctx.org.dossiers.filter((x) => x.parent === d.id).sort(parNom);
  const convs = ctx.visibles.filter((f) => f.dossier === d.id);
  const themes = ctx.org.themes.filter((t) => d.themes.includes(t.id));
  return (
    <div className="conv-groupe">
      <div
        className={`conv-ligne dossier ${ctx.cible === d.id ? "cible" : ""}`}
        style={{ paddingLeft: 6 + niveau * 14 }}
        data-depot={d.id}
        onPointerDown={ctx.commencer("dossier", d.id, d.nom)}
        onClick={() => ctx.cliquable() && edition !== "nom" && deplier(d.id)}
        onDoubleClick={() => setEdition("nom")}
      >
        <I.chevron size={11} className={`conv-chevron ${ouvert ? "ouvert" : ""}`} />
        <I.folder size={13} />
        {edition === "nom" ? (
          <SaisieNom initial={d.nom} onValider={(n) => { void renommer(d.id, n); setEdition(null); }} onAnnuler={() => setEdition(null)} />
        ) : (
          <span className="conv-nom">{d.nom}</span>
        )}
        {d.projet && <span className="conv-projet" title={`Projet : ${d.projet}`}>{d.projet}</span>}
        <Pastilles themes={themes} />
        <button className="icon-btn xs conv-plus" title="Actions du dossier" onClick={(e) => { e.stopPropagation(); setMenu(!menu); }}>⋯</button>
        {menu && <MenuDossier d={d} fermer={() => setMenu(false)} renommer={() => setEdition("nom")} sousDossier={() => { deplier(d.id, true); setEdition("sous"); }} />}
      </div>
      {edition === "sous" && (
        <div style={{ paddingLeft: 20 + niveau * 14 }}>
          <SaisieNom placeholder="Nom du sous-dossier" initial="" onValider={(n) => { void creerDossier(n, d.id); setEdition(null); }} onAnnuler={() => setEdition(null)} />
        </div>
      )}
      {ouvert && (
        <>
          {enfants.map((x) => <LigneDossier key={x.id} d={x} niveau={niveau + 1} ctx={ctx} />)}
          {convs.map((f) => <LigneConv key={f.id} f={f} niveau={niveau + 1} ctx={ctx} />)}
          {enfants.length === 0 && convs.length === 0 && <div className="muted small conv-vide" style={{ paddingLeft: 26 + niveau * 14 }}>vide</div>}
        </>
      )}
    </div>
  );
}

function LigneConv({ f, niveau, ctx }: { f: Fiche; niveau: number; ctx: Ctx }) {
  const ouvrir = useConv((s) => s.ouvrir);
  const modifier = useConv((s) => s.modifierConv);
  const busy = useStore((s) => s.busy);
  const [menu, setMenu] = useState(false);
  const [edition, setEdition] = useState(false);
  const themes = ctx.org.themes.filter((t) => f.themes.includes(t.id));
  const active = ctx.courante === f.id;
  return (
    <div
      className={`conv-ligne conv ${active ? "active" : ""}`}
      style={{ paddingLeft: 10 + niveau * 14 }}
      onPointerDown={ctx.commencer("conv", f.id, f.titre)}
      onClick={() => ctx.cliquable() && !edition && ouvrir(f.id)}
      onDoubleClick={() => setEdition(true)}
      title={busy && !active ? "Orchestra travaille : tu pourras changer de conversation à la fin du tour" : f.apercu || f.titre}
    >
      {f.contexte_dossier ? <I.search size={12} /> : <I.chat size={12} />}
      {edition ? (
        <SaisieNom initial={f.titre} onValider={(n) => { void modifier(f.id, { titre: n }); setEdition(false); }} onAnnuler={() => setEdition(false)} />
      ) : (
        <span className="conv-nom">{f.titre}</span>
      )}
      <Pastilles themes={themes} />
      <span className="conv-date">{dateCourte(f.modifie_le)}</span>
      <button className="icon-btn xs conv-plus" title="Actions de la conversation" onClick={(e) => { e.stopPropagation(); setMenu(!menu); }}>⋯</button>
      {menu && <MenuConv f={f} fermer={() => setMenu(false)} renommer={() => setEdition(true)} />}
    </div>
  );
}

function dateCourte(ms: number): string {
  const d = new Date(ms);
  const auj = new Date();
  if (d.toDateString() === auj.toDateString()) return d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  return d.toLocaleDateString("fr-FR", { day: "2-digit", month: "2-digit" });
}

function Pastilles({ themes }: { themes: Theme[] }) {
  if (themes.length === 0) return null;
  return (
    <span className="conv-pastilles" title={themes.map((t) => t.nom).join(", ")}>
      {themes.slice(0, 4).map((t) => <i key={t.id} style={{ background: t.couleur }} />)}
    </span>
  );
}

/** Champ de saisie d'un nom : Entrée valide, Échap annule. */
function SaisieNom({ initial, placeholder, onValider, onAnnuler }: { initial: string; placeholder?: string; onValider: (n: string) => void; onAnnuler: () => void }) {
  const [v, setV] = useState(initial);
  return (
    <input
      className="conv-saisie"
      autoFocus
      value={v}
      placeholder={placeholder}
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      onChange={(e) => setV(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") { if (v.trim()) onValider(v.trim()); else onAnnuler(); }
        if (e.key === "Escape") onAnnuler();
      }}
      onBlur={() => (v.trim() && v.trim() !== initial ? onValider(v.trim()) : onAnnuler())}
    />
  );
}

/** Petit menu flottant (au-dessus de tout, placé près de sa ligne) qui se ferme au clic ailleurs. */
function Menu({ fermer, children }: { fermer: () => void; children: React.ReactNode }) {
  const ancre = useRef<HTMLSpanElement>(null);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  useLayoutEffect(() => {
    const r = ancre.current?.parentElement?.getBoundingClientRect();
    if (!r) return;
    const h = Math.min(340, ref.current?.offsetHeight ?? 240);
    const top = r.bottom + 2 + h > window.innerHeight - 8 ? Math.max(8, r.top - 2 - h) : r.bottom + 2;
    setPos({ top, left: Math.max(8, Math.min(window.innerWidth - 268, r.right - 230)) });
  }, [children]);
  useEffect(() => {
    const f = (e: PointerEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) fermer(); };
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") fermer(); };
    const t = setTimeout(() => window.addEventListener("pointerdown", f), 0);
    window.addEventListener("keydown", k);
    return () => { clearTimeout(t); window.removeEventListener("pointerdown", f); window.removeEventListener("keydown", k); };
  }, []);
  return (
    <span ref={ancre} className="conv-menu-ancre">
      {createPortal(
        <motion.div ref={ref} className="conv-menu glass" style={{ top: pos?.top ?? -9999, left: pos?.left ?? -9999 }} initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} onClick={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
          {children}
        </motion.div>,
        document.body,
      )}
    </span>
  );
}

function MenuConv({ f, fermer, renommer }: { f: Fiche; fermer: () => void; renommer: () => void }) {
  const [vue, setVue] = useState<"menu" | "themes" | "deplacer" | "supprimer">("menu");
  const org = useConv((s) => s.org);
  const basculer = useConv((s) => s.basculerThemeConv);
  const modifier = useConv((s) => s.modifierConv);
  const supprimer = useConv((s) => s.supprimerConv);
  const themesConv = useConv((s) => (s.courante?.id === f.id ? s.courante.themes : s.fiches.find((x) => x.id === f.id)?.themes ?? []));
  return (
    <Menu fermer={fermer}>
      {vue === "menu" && (
        <>
          <button onClick={() => { fermer(); renommer(); }}><I.pen size={12} /> Renommer</button>
          <button onClick={() => setVue("themes")}><I.spark size={12} /> Thèmes…</button>
          <button onClick={() => setVue("deplacer")}><I.folder size={12} /> Déplacer vers…</button>
          <button className="danger" onClick={() => setVue("supprimer")}><I.trash size={12} /> Supprimer</button>
        </>
      )}
      {vue === "themes" && <ChoixThemes actifs={themesConv} basculer={(t) => basculer(f.id, t)} />}
      {vue === "deplacer" && <ChoixDossier org={org} actuel={f.dossier} choisir={(d) => { void modifier(f.id, { dossier: d }); fermer(); }} />}
      {vue === "supprimer" && (
        <>
          <div className="small conv-menu-texte">Mettre « {f.titre} » à la corbeille ? Le fichier reste dans <code>conversations/corbeille/</code>.</div>
          <button className="danger" onClick={() => { void supprimer(f.id); fermer(); }}><I.trash size={12} /> Oui, supprimer</button>
          <button onClick={() => setVue("menu")}>Annuler</button>
        </>
      )}
    </Menu>
  );
}

function MenuDossier({ d, fermer, renommer, sousDossier }: { d: Dossier; fermer: () => void; renommer: () => void; sousDossier: () => void }) {
  const [vue, setVue] = useState<"menu" | "themes" | "projet" | "deplacer" | "supprimer">("menu");
  const org = useConv((s) => s.org);
  const nouvelle = useConv((s) => s.nouvelle);
  const interroger = useConv((s) => s.interroger);
  const voirContexte = useConv((s) => s.voirContexte);
  const basculer = useConv((s) => s.basculerThemeDossier);
  const lierProjet = useConv((s) => s.lierProjet);
  const deplacer = useConv((s) => s.deplacerDossier);
  const supprimer = useConv((s) => s.supprimerDossier);
  const actifs = org.dossiers.find((x) => x.id === d.id)?.themes ?? [];
  return (
    <Menu fermer={fermer}>
      {vue === "menu" && (
        <>
          <button className="fort" onClick={() => { fermer(); void interroger(d.id); }}><I.search size={12} /> Interroger ce dossier</button>
          <button onClick={() => { fermer(); void voirContexte(d.id); }}><I.memory size={12} /> Voir ce qu'Orchestra lira</button>
          <button onClick={() => { fermer(); void nouvelle(d.id); }}><I.plus size={12} /> Nouvelle conversation ici</button>
          <button onClick={() => { fermer(); sousDossier(); }}><I.folder size={12} /> Nouveau sous-dossier</button>
          <button onClick={() => setVue("projet")}><I.grid size={12} /> Projet… {d.projet ? <span className="muted">({d.projet})</span> : null}</button>
          <button onClick={() => setVue("themes")}><I.spark size={12} /> Thèmes…</button>
          <button onClick={() => { fermer(); renommer(); }}><I.pen size={12} /> Renommer</button>
          <button onClick={() => setVue("deplacer")}><I.folder size={12} /> Déplacer vers…</button>
          <button className="danger" onClick={() => setVue("supprimer")}><I.trash size={12} /> Supprimer le dossier</button>
        </>
      )}
      {vue === "themes" && <ChoixThemes actifs={actifs} basculer={(t) => basculer(d.id, t)} />}
      {vue === "projet" && <ChoixProjet d={d} choisir={(p) => { void lierProjet(d.id, p); fermer(); }} />}
      {vue === "deplacer" && <ChoixDossier org={org} actuel={d.parent} interdits={descendants(org, d.id)} choisir={(p) => { void deplacer(d.id, p); fermer(); }} />}
      {vue === "supprimer" && (
        <>
          <div className="small conv-menu-texte">Supprimer le dossier « {d.nom} » ? Ses sous-dossiers et ses conversations remontent dans {d.parent ? `« ${cheminDossier(org, d.parent)} »` : "l'historique"} ; aucune conversation n'est supprimée.</div>
          <button className="danger" onClick={() => { void supprimer(d.id); fermer(); }}><I.trash size={12} /> Oui, supprimer le dossier</button>
          <button onClick={() => setVue("menu")}>Annuler</button>
        </>
      )}
    </Menu>
  );
}

function ChoixThemes({ actifs, basculer }: { actifs: string[]; basculer: (id: string) => void }) {
  const themes = useConv((s) => s.org.themes);
  const creer = useConv((s) => s.creerTheme);
  const [nom, setNom] = useState("");
  return (
    <>
      <div className="conv-menu-titre">Thèmes</div>
      {themes.length === 0 && <div className="small muted conv-menu-texte">Aucun thème : crée le premier ci-dessous.</div>}
      {themes.map((t) => (
        <button key={t.id} onClick={() => basculer(t.id)}>
          <i className="conv-pastille" style={{ background: t.couleur }} /> {t.nom} {actifs.includes(t.id) && <I.check size={12} className="conv-coche" />}
        </button>
      ))}
      <input
        className="conv-saisie"
        placeholder="Nouveau thème + Entrée"
        value={nom}
        onChange={(e) => setNom(e.target.value)}
        onKeyDown={async (e) => {
          if (e.key === "Enter" && nom.trim()) {
            const t = await creer(nom);
            if (t && !actifs.includes(t.id)) basculer(t.id);
            setNom("");
          }
        }}
      />
    </>
  );
}

function ChoixDossier({ org, actuel, interdits, choisir }: { org: Organisation; actuel: string | null; interdits?: Set<string>; choisir: (id: string | null) => void }) {
  const lignes: { d: Dossier; niveau: number }[] = [];
  const existe = new Set(org.dossiers.map((d) => d.id));
  const parcourir = (parent: string | null, niveau: number, vus: Set<string>) => {
    org.dossiers.filter((d) => (parent ? d.parent === parent : !d.parent || !existe.has(d.parent))).sort(parNom).forEach((d) => {
      if (vus.has(d.id)) return;
      vus.add(d.id);
      lignes.push({ d, niveau });
      parcourir(d.id, niveau + 1, vus);
    });
  };
  parcourir(null, 0, new Set());
  return (
    <>
      <div className="conv-menu-titre">Déplacer vers</div>
      <button disabled={actuel === null} onClick={() => choisir(null)}><I.chat size={12} /> Historique (sans dossier)</button>
      {lignes.map(({ d, niveau }) => (
        <button key={d.id} disabled={d.id === actuel || interdits?.has(d.id)} style={{ paddingLeft: 10 + niveau * 12 }} onClick={() => choisir(d.id)}>
          <I.folder size={12} /> {d.nom}
        </button>
      ))}
      {lignes.length === 0 && <div className="small muted conv-menu-texte">Aucun dossier : crée-en un avec le bouton dossier en haut du panneau.</div>}
    </>
  );
}

function ChoixProjet({ d, choisir }: { d: Dossier; choisir: (p: string | null) => void }) {
  const projets = useStore((s) => s.projets);
  const refresh = useStore((s) => s.refreshProjets);
  const org = useConv((s) => s.org);
  const [nom, setNom] = useState("");
  useEffect(() => { void refresh().catch(() => {}); }, []);
  const herite = d.parent ? projetDe(org, d.parent) : null;
  return (
    <>
      <div className="conv-menu-titre">Projet du dossier</div>
      <div className="small muted conv-menu-texte">Les conversations de ce dossier forment ce projet : Orchestra s'en sert comme contexte, et les agents y écrivent leurs livrables quand une de ces conversations est ouverte.</div>
      <button disabled={!d.projet} onClick={() => choisir(null)}>Aucun{herite ? ` (hérité : ${herite})` : ""}</button>
      {(projets?.liste ?? []).map((p) => (
        <button key={p} disabled={p === d.projet} onClick={() => choisir(p)}><I.grid size={12} /> {p}</button>
      ))}
      <input className="conv-saisie" placeholder="Nouveau projet + Entrée" value={nom} onChange={(e) => setNom(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && nom.trim()) choisir(nom.trim()); }} />
    </>
  );
}

/** Barre des thèmes : cliquer filtre, double-cliquer renomme, ⨯ supprime. */
function BarreThemes() {
  const themes = useConv((s) => s.org.themes);
  const filtre = useConv((s) => s.filtre);
  const setFiltre = useConv((s) => s.setFiltre);
  const creer = useConv((s) => s.creerTheme);
  const renommer = useConv((s) => s.renommerTheme);
  const supprimer = useConv((s) => s.supprimerTheme);
  const [ajout, setAjout] = useState(false);
  const [edition, setEdition] = useState<string | null>(null);
  const [aSupprimer, setASupprimer] = useState<string | null>(null);
  return (
    <div className="conv-themes">
      {themes.map((t) =>
        edition === t.id ? (
          <SaisieNom key={t.id} initial={t.nom} onValider={(n) => { void renommer(t.id, n); setEdition(null); }} onAnnuler={() => setEdition(null)} />
        ) : aSupprimer === t.id ? (
          <span key={t.id} className="conv-theme confirmer">
            Supprimer « {t.nom} » ?
            <button className="link-btn danger" onClick={() => { void supprimer(t.id); setASupprimer(null); }}>oui</button>
            <button className="link-btn" onClick={() => setASupprimer(null)}>non</button>
          </span>
        ) : (
          <span
            key={t.id}
            className={`conv-theme ${filtre.includes(t.id) ? "actif" : ""}`}
            style={{ ["--c" as any]: t.couleur }}
            title="Cliquer : filtrer · double-cliquer : renommer"
            onClick={() => setFiltre(filtre.includes(t.id) ? filtre.filter((x) => x !== t.id) : [...filtre, t.id])}
            onDoubleClick={() => setEdition(t.id)}
          >
            <i /> {t.nom}
            <button className="conv-theme-x" title="Supprimer ce thème" onClick={(e) => { e.stopPropagation(); setASupprimer(t.id); }}><I.x size={9} /></button>
          </span>
        ),
      )}
      {ajout ? (
        <SaisieNom initial="" placeholder="Nom du thème" onValider={(n) => { void creer(n); setAjout(false); }} onAnnuler={() => setAjout(false)} />
      ) : (
        <button className="conv-theme ajout" onClick={() => setAjout(true)}><I.plus size={10} /> thème</button>
      )}
      {filtre.length > 0 && <button className="link-btn" onClick={() => setFiltre([])}>tout afficher</button>}
    </div>
  );
}

function ApercuContexte({ apercu }: { apercu: { dossier: string; texte: string } }) {
  const voir = useConv((s) => s.voirContexte);
  const interroger = useConv((s) => s.interroger);
  const org = useConv((s) => s.org);
  return (
    <motion.div className="conv-apercu-fond" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={() => voir(null)}>
      <motion.div className="conv-apercu glass" initial={{ y: 12 }} animate={{ y: 0 }} onClick={(e) => e.stopPropagation()}>
        <div className="panel-head">
          <div className="panel-title"><I.memory size={14} /> Ce qu'Orchestra lira de « {cheminDossier(org, apercu.dossier)} »</div>
          <button className="icon-btn" onClick={() => voir(null)}><I.x size={14} /></button>
        </div>
        <pre className="conv-apercu-texte">{apercu.texte}</pre>
        <div className="conv-apercu-actions">
          <span className="small muted">Coupes signalées dans le texte. Les conversations restent lisibles dans <code>Orchestra\conversations\</code>.</span>
          <button className="glass-btn sm active" onClick={() => { const d = apercu.dossier; void voir(null); void interroger(d); }}><I.search size={11} /> Interroger ce dossier</button>
        </div>
      </motion.div>
    </motion.div>
  );
}
