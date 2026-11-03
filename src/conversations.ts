import { create } from "zustand";
import { useStore, uid, voice } from "./store";
import { api } from "./tauri";
import type { ChatMessage } from "./types";
import { convApi, titreDe, type Conversation, type Dossier, type Fiche, type MessageConv, type Organisation, type Theme } from "./conversationsApi";

/**
 * Conversations, thèmes et dossiers (demande de PB du 03/11).
 * Les messages de la conversation affichée vivent dans le store principal (`useStore.messages`) ;
 * ce module les enregistre dans `Orchestra/conversations/<id>.json` à chaque changement, et
 * remplace les messages quand PB ouvre une autre conversation.
 */

export type Disposition = "ouvert" | "reduit";
export type EtatConscience = "ouverte" | "reduite" | "masquee";

const COULEURS = ["#a5f3fc", "#c084fc", "#5eead4", "#fbbf24", "#f472b6", "#818cf8", "#4ade80", "#fb923c"];

const pref = {
  get<T>(k: string, d: T): T {
    try {
      const v = localStorage.getItem(`orchestra.${k}`);
      return v === null ? d : (JSON.parse(v) as T);
    } catch { return d; }
  },
  set(k: string, v: unknown) {
    try { localStorage.setItem(`orchestra.${k}`, JSON.stringify(v)); } catch { /* sans stockage */ }
  },
};

/** Conversation affichée, sans ses messages (ils sont dans `useStore`). */
type Courante = Omit<Conversation, "messages"> & { surDisque: boolean };

interface ConvState {
  pret: boolean;
  org: Organisation;
  fiches: Fiche[];
  courante: Courante | null;
  erreur: string;
  illisibles: string[];
  /** filtre par thèmes (un des thèmes suffit) et recherche dans les titres */
  filtre: string[];
  recherche: string;
  /** dossiers dépliés */
  deplies: Record<string, boolean>;
  panneau: Disposition;
  conscience: EtatConscience;
  /** aperçu de ce qu'Orchestra lira d'un dossier */
  apercu: { dossier: string; texte: string } | null;

  init: () => Promise<void>;
  nouvelle: (dossier?: string | null, contexteDossier?: string | null, titre?: string) => Promise<void>;
  ouvrir: (id: string) => Promise<void>;
  fermerOnglet: (id: string) => Promise<void>;
  modifierConv: (id: string, p: { titre?: string; themes?: string[]; dossier?: string | null }) => Promise<void>;
  basculerThemeConv: (id: string, theme: string) => Promise<void>;
  supprimerConv: (id: string) => Promise<void>;
  creerTheme: (nom: string) => Promise<Theme | null>;
  renommerTheme: (id: string, nom: string) => Promise<void>;
  supprimerTheme: (id: string) => Promise<void>;
  creerDossier: (nom: string, parent: string | null) => Promise<Dossier | null>;
  renommerDossier: (id: string, nom: string) => Promise<void>;
  deplacerDossier: (id: string, parent: string | null) => Promise<void>;
  basculerThemeDossier: (id: string, theme: string) => Promise<void>;
  lierProjet: (id: string, projet: string | null) => Promise<void>;
  supprimerDossier: (id: string) => Promise<void>;
  interroger: (dossier: string) => Promise<void>;
  voirContexte: (dossier: string | null) => Promise<void>;
  setFiltre: (f: string[]) => void;
  setRecherche: (r: string) => void;
  deplier: (id: string, ouvert?: boolean) => void;
  setPanneau: (d: Disposition) => void;
  setConscience: (c: EtatConscience) => void;
  effacerErreur: () => void;
}

// ---------- outils ----------

const versMessage = (m: MessageConv): ChatMessage => ({ id: uid(), role: m.role, content: m.content, contexte: m.contexte, at: m.at, intent: m.intent, agent: m.agent, voice: m.voice });
const depuisMessage = (m: ChatMessage): MessageConv => {
  const r: MessageConv = { role: m.role, content: m.content, at: m.at };
  if (m.contexte) r.contexte = m.contexte;
  if (m.intent) r.intent = m.intent;
  if (m.agent) r.agent = m.agent;
  if (m.voice) r.voice = true;
  return r;
};
/** Ce qui est gardé : ce que le cerveau reçoit dans l'historique (même filtre que `historyOf`). */
const gardes = (msgs: ChatMessage[]) => msgs.filter((m) => !m.streaming && !m.sim && !m.note);
const idSur = (prefixe: string) => `${prefixe}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/** Le dossier et tous ses parents (sans boucle). */
export function ascendants(org: Organisation, id: string | null): Dossier[] {
  const out: Dossier[] = [];
  const vus = new Set<string>();
  let cur = id;
  while (cur && !vus.has(cur)) {
    vus.add(cur);
    const d = org.dossiers.find((x) => x.id === cur);
    if (!d) break;
    out.push(d);
    cur = d.parent;
  }
  return out;
}
export function descendants(org: Organisation, id: string): Set<string> {
  const vus = new Set<string>();
  const pile = [id];
  while (pile.length) {
    const d = pile.pop()!;
    if (vus.has(d)) continue;
    vus.add(d);
    org.dossiers.filter((x) => x.parent === d).forEach((x) => pile.push(x.id));
  }
  return vus;
}
export const cheminDossier = (org: Organisation, id: string | null) => ascendants(org, id).reverse().map((d) => d.nom).join(" / ");
export const projetDe = (org: Organisation, id: string | null) => ascendants(org, id).find((d) => d.projet?.trim())?.projet ?? null;
/** Thèmes d'une conversation, y compris ceux de ses dossiers. */
export const themesHerites = (org: Organisation, f: { themes: string[]; dossier: string | null }) => new Set([...f.themes, ...ascendants(org, f.dossier).flatMap((d) => d.themes)]);

/** Le bon projet actif pour une conversation rangée dans un dossier lié à un projet (D36 : choisi dans l'interface). */
async function suivreProjet(dossier: string | null) {
  const p = projetDe(useConv.getState().org, dossier);
  const s = useStore.getState();
  if (p && s.projets?.actif !== p) {
    try { await s.choisirProjet(p); } catch { /* le projet reste celui choisi avant */ }
  }
}

/** Le mode voix reçoit l'historique et le résumé de la conversation ouverte (C9). */
async function contexteVoix() {
  if (!voice?.connected) return;
  const h = gardes(useStore.getState().messages).map((m) => ({ role: m.role, content: m.contexte ?? m.content }));
  try {
    const e = await api.etatSession();
    voice.send({ type: "context", messages: h.slice(Math.min(e.couvre, h.length)), resume: e.resume ?? "", profil: e.profil });
  } catch {
    voice.send({ type: "context", messages: h });
  }
}

// ---------- enregistrement ----------

/** vrai pendant que ce module remplace lui-même les messages (ouverture d'une conversation) */
let applique = false;
let minuterie: ReturnType<typeof setTimeout> | null = null;
let minuterieOrg: ReturnType<typeof setTimeout> | null = null;
let enCours: Promise<void> = Promise.resolve();

function planifier(ms = 700) {
  if (minuterie) clearTimeout(minuterie);
  minuterie = setTimeout(() => { minuterie = null; void enregistrerActive(); }, ms);
}

/** Enregistre la conversation affichée (rien tant qu'elle est vide). */
function enregistrerActive(msgs?: ChatMessage[]): Promise<void> {
  if (minuterie) { clearTimeout(minuterie); minuterie = null; }
  const c = useConv.getState().courante;
  const tous = msgs ?? useStore.getState().messages;
  // une réponse en cours d'écriture : on attend qu'elle soit finie
  if (!msgs && tous.some((m) => m.streaming)) { planifier(1200); return enCours; }
  const m = gardes(tous);
  if (!c || m.length === 0) return enCours;
  const conv: Conversation = { ...c, modifie_le: Date.now(), messages: m.map(depuisMessage) };
  delete (conv as Partial<Courante>).surDisque;
  enCours = enCours.then(async () => {
    try {
      const fiche = await convApi.enregistrer(conv, true);
      useConv.setState((s) => ({
        fiches: [fiche, ...s.fiches.filter((f) => f.id !== fiche.id)],
        courante: s.courante?.id === conv.id ? { ...s.courante, modifie_le: conv.modifie_le, surDisque: true } : s.courante,
      }));
    } catch (e) {
      useConv.setState({ erreur: `Conversation non enregistrée : ${String(e)}` });
    }
  });
  return enCours;
}

function enregistrerOrg(immediat = false) {
  if (minuterieOrg) clearTimeout(minuterieOrg);
  const go = () => {
    minuterieOrg = null;
    convApi.enregistrerOrganisation(useConv.getState().org).catch((e) => useConv.setState({ erreur: `Organisation non enregistrée : ${String(e)}` }));
  };
  if (immediat) go(); else minuterieOrg = setTimeout(go, 400);
}

function setOrg(f: (o: Organisation) => Organisation, immediat = false) {
  useConv.setState((s) => ({ org: f(s.org) }));
  enregistrerOrg(immediat);
}

function occupe(): boolean {
  if (useStore.getState().busy) {
    useConv.setState({ erreur: "Orchestra travaille : attends la fin du tour pour changer de conversation." });
    return true;
  }
  return false;
}

/** Affiche la conversation (messages, onglet, conversation active). */
function afficher(conv: Conversation, surDisque: boolean) {
  applique = true;
  useStore.setState({ messages: conv.messages.map(versMessage), entries: [], agents: {}, inspect: null, agentsProposes: [], fileAttente: [] });
  applique = false;
  const { messages: _m, ...meta } = conv;
  useConv.setState({ courante: { ...meta, surDisque } });
  setOrg((o) => ({ ...o, active: conv.id, onglets: o.onglets.includes(conv.id) ? o.onglets : [...o.onglets, conv.id] }));
  // le dossier de la conversation est déplié dans le panneau
  const st = useConv.getState();
  for (const d of ascendants(st.org, conv.dossier)) if (!st.deplies[d.id]) st.deplier(d.id, true);
  void contexteVoix();
  void suivreProjet(conv.dossier);
}

/** Une conversation jamais écrite (vide) disparaît de ses onglets quand on la quitte. */
function oublierSiVide(id: string | undefined) {
  if (!id) return;
  const c = useConv.getState().courante;
  if (c && c.id === id && !c.surDisque && gardes(useStore.getState().messages).length === 0) {
    setOrg((o) => ({ ...o, onglets: o.onglets.filter((x) => x !== id) }));
  }
}

// ---------- store ----------

export const useConv = create<ConvState>((set, get) => ({
  pret: false,
  org: { themes: [], dossiers: [], onglets: [], active: null },
  fiches: [],
  courante: null,
  erreur: "",
  illisibles: [],
  filtre: [],
  recherche: "",
  deplies: pref.get("conv.deplies", {} as Record<string, boolean>),
  panneau: pref.get<Disposition>("conv.panneau", "ouvert"),
  conscience: pref.get<EtatConscience>("conscience", "ouverte"),
  apercu: null,

  init: async () => {
    if (get().pret) return;
    try {
      const e = await convApi.etat();
      const existe = new Set(e.conversations.map((f) => f.id));
      const org = { ...e.organisation, onglets: e.organisation.onglets.filter((id) => existe.has(id)) };
      set({ org, fiches: e.conversations, illisibles: e.illisibles, pret: true });
      // PB a déjà commencé à parler avant le chargement : cette conversation est gardée telle quelle
      if (gardes(useStore.getState().messages).length > 0) {
        const c = await convApi.nouvelle(null, null);
        set({ courante: { ...c, surDisque: false } });
        setOrg((o) => ({ ...o, active: c.id, onglets: [...o.onglets, c.id] }));
        await enregistrerActive();
        return;
      }
      if (org.active && existe.has(org.active)) await get().ouvrir(org.active);
      else await get().nouvelle();
    } catch (e) {
      set({ pret: true, erreur: `Conversations indisponibles : ${String(e)}` });
    }
  },

  nouvelle: async (dossier = null, contexteDossier = null, titre = "") => {
    if (occupe()) return;
    await enregistrerActive();
    const avant = get().courante?.id;
    try {
      const c = await convApi.nouvelle(dossier, contexteDossier);
      oublierSiVide(avant);
      afficher({ ...c, titre }, false);
    } catch (e) {
      set({ erreur: String(e) });
    }
  },

  ouvrir: async (id) => {
    if (get().courante?.id === id) return;
    if (occupe()) return;
    await enregistrerActive();
    const avant = get().courante?.id;
    try {
      const c = await convApi.ouvrir(id);
      oublierSiVide(avant);
      afficher(c, true);
    } catch (e) {
      set({ erreur: String(e) });
    }
  },

  fermerOnglet: async (id) => {
    const { org, courante } = get();
    if (courante?.id === id) {
      if (occupe()) return;
      const i = org.onglets.indexOf(id);
      const voisin = org.onglets[i + 1] ?? org.onglets[i - 1];
      setOrg((o) => ({ ...o, onglets: o.onglets.filter((x) => x !== id) }));
      if (voisin) await get().ouvrir(voisin);
      else await get().nouvelle();
      // l'onglet fermé ne doit pas revenir (ouvrir l'ajoute seulement s'il manque)
      setOrg((o) => ({ ...o, onglets: o.onglets.filter((x) => x !== id) }));
    } else {
      setOrg((o) => ({ ...o, onglets: o.onglets.filter((x) => x !== id) }));
    }
  },

  modifierConv: async (id, p) => {
    const c = get().courante;
    if (c?.id === id) {
      set({ courante: { ...c, ...(p.titre !== undefined ? { titre: p.titre.trim() } : {}), ...(p.themes ? { themes: p.themes } : {}), ...("dossier" in p ? { dossier: p.dossier ?? null } : {}) } });
      if ("dossier" in p) void suivreProjet(p.dossier ?? null);
      if (!c.surDisque) return; // écrite au premier message, avec ces changements
    }
    try {
      const fiche = await convApi.modifier(id, p);
      set((s) => ({ fiches: s.fiches.map((f) => (f.id === id ? fiche : f)) }));
    } catch (e) {
      set({ erreur: String(e) });
    }
  },

  basculerThemeConv: async (id, theme) => {
    const actuels = get().courante?.id === id ? get().courante!.themes : get().fiches.find((f) => f.id === id)?.themes ?? [];
    const themes = actuels.includes(theme) ? actuels.filter((t) => t !== theme) : [...actuels, theme];
    await get().modifierConv(id, { themes });
  },

  supprimerConv: async (id) => {
    if (get().courante?.id === id) {
      if (occupe()) return;
      // on quitte d'abord la conversation (sans la réenregistrer ensuite)
      set((s) => ({ courante: s.courante ? { ...s.courante, surDisque: false } : null }));
      applique = true;
      useStore.setState({ messages: [] });
      applique = false;
      await get().fermerOnglet(id);
    }
    try {
      await convApi.supprimer(id);
      set((s) => ({ fiches: s.fiches.filter((f) => f.id !== id) }));
      setOrg((o) => ({ ...o, onglets: o.onglets.filter((x) => x !== id) }));
    } catch (e) {
      set({ erreur: String(e) });
    }
  },

  creerTheme: async (nom) => {
    const n = nom.trim();
    if (!n) return null;
    const exist = get().org.themes.find((t) => t.nom.toLowerCase() === n.toLowerCase());
    if (exist) return exist;
    const t: Theme = { id: idSur("t"), nom: n, couleur: COULEURS[get().org.themes.length % COULEURS.length] };
    setOrg((o) => ({ ...o, themes: [...o.themes, t] }), true);
    return t;
  },

  renommerTheme: async (id, nom) => {
    if (!nom.trim()) return;
    setOrg((o) => ({ ...o, themes: o.themes.map((t) => (t.id === id ? { ...t, nom: nom.trim() } : t)) }), true);
  },

  supprimerTheme: async (id) => {
    setOrg((o) => ({ ...o, themes: o.themes.filter((t) => t.id !== id), dossiers: o.dossiers.map((d) => ({ ...d, themes: d.themes.filter((t) => t !== id) })) }), true);
    set((s) => ({ filtre: s.filtre.filter((t) => t !== id) }));
    const c = get().courante;
    if (c?.themes.includes(id)) await get().modifierConv(c.id, { themes: c.themes.filter((t) => t !== id) });
    for (const f of get().fiches.filter((f) => f.themes.includes(id) && f.id !== c?.id)) {
      await get().modifierConv(f.id, { themes: f.themes.filter((t) => t !== id) });
    }
  },

  creerDossier: async (nom, parent) => {
    const n = nom.trim();
    if (!n) return null;
    const d: Dossier = { id: idSur("d"), nom: n, parent, themes: [], projet: null };
    setOrg((o) => ({ ...o, dossiers: [...o.dossiers, d] }), true);
    if (parent) get().deplier(parent, true);
    return d;
  },

  renommerDossier: async (id, nom) => {
    if (!nom.trim()) return;
    setOrg((o) => ({ ...o, dossiers: o.dossiers.map((d) => (d.id === id ? { ...d, nom: nom.trim() } : d)) }), true);
  },

  deplacerDossier: async (id, parent) => {
    if (parent && descendants(get().org, id).has(parent)) {
      set({ erreur: "Un dossier ne peut pas aller dans un de ses sous-dossiers." });
      return;
    }
    setOrg((o) => ({ ...o, dossiers: o.dossiers.map((d) => (d.id === id ? { ...d, parent } : d)) }), true);
    if (parent) get().deplier(parent, true);
  },

  basculerThemeDossier: async (id, theme) => {
    setOrg((o) => ({ ...o, dossiers: o.dossiers.map((d) => (d.id === id ? { ...d, themes: d.themes.includes(theme) ? d.themes.filter((t) => t !== theme) : [...d.themes, theme] } : d)) }), true);
  },

  lierProjet: async (id, projet) => {
    const p = projet?.trim() || null;
    setOrg((o) => ({ ...o, dossiers: o.dossiers.map((d) => (d.id === id ? { ...d, projet: p } : d)) }), true);
    const c = get().courante;
    if (p && c && descendants(get().org, id).has(c.dossier ?? "")) void suivreProjet(c.dossier);
  },

  /** Le dossier disparaît ; ses sous-dossiers et ses conversations remontent d'un niveau (rien n'est supprimé). */
  supprimerDossier: async (id) => {
    const d = get().org.dossiers.find((x) => x.id === id);
    if (!d) return;
    setOrg((o) => ({ ...o, dossiers: o.dossiers.filter((x) => x.id !== id).map((x) => (x.parent === id ? { ...x, parent: d.parent } : x)) }), true);
    const c = get().courante;
    if (c?.dossier === id) await get().modifierConv(c.id, { dossier: d.parent });
    for (const f of get().fiches.filter((f) => f.dossier === id && f.id !== c?.id)) await get().modifierConv(f.id, { dossier: d.parent });
  },

  interroger: async (dossier) => {
    const d = get().org.dossiers.find((x) => x.id === dossier);
    if (!d) return;
    await get().nouvelle(dossier, dossier, `Questions sur « ${d.nom} »`);
  },

  voirContexte: async (dossier) => {
    if (!dossier) { set({ apercu: null }); return; }
    try {
      await enregistrerActive();
      set({ apercu: { dossier, texte: await convApi.contexte(dossier) } });
    } catch (e) {
      set({ erreur: String(e) });
    }
  },

  setFiltre: (filtre) => set({ filtre }),
  setRecherche: (recherche) => set({ recherche }),
  deplier: (id, ouvert) => {
    const deplies = { ...get().deplies, [id]: ouvert ?? !get().deplies[id] };
    pref.set("conv.deplies", deplies);
    set({ deplies });
  },
  setPanneau: (panneau) => { pref.set("conv.panneau", panneau); set({ panneau }); },
  setConscience: (conscience) => { pref.set("conscience", conscience); set({ conscience }); },
  effacerErreur: () => set({ erreur: "" }),
}));

// ---------- suivi des messages du store principal ----------

useStore.subscribe((s, p) => {
  if (applique || s.messages === p.messages || !useConv.getState().pret) return;
  // bouton « Nouvelle conversation » de la conversation (corbeille) : l'ancienne est gardée, une nouvelle commence
  if (s.messages.length === 0 && p.messages.length > 0) {
    const avant = p.messages;
    void enregistrerActive(avant).then(() => useConv.getState().nouvelle(useConv.getState().courante?.dossier ?? null));
    return;
  }
  planifier();
});

/** Titre affiché dans un onglet ou une ligne. */
export function titreOnglet(id: string): string {
  const { courante, fiches } = useConv.getState();
  if (courante?.id === id) {
    if (courante.titre.trim()) return courante.titre.trim();
    return titreDe({ titre: "", messages: gardes(useStore.getState().messages).map(depuisMessage) });
  }
  return fiches.find((f) => f.id === id)?.titre ?? "Conversation";
}

// ---------- Conscience réduite ou masquée : elle se rouvre quand PB doit répondre ----------

type Principal = ReturnType<typeof useStore.getState>;
const attentes = (s: Principal) =>
  s.lecturesAConfirmer.length + s.alertesIntegrite.length + s.alertesDonnees.length +
  s.entries.reduce((n, e) => n + (e.kind === "mission" ? e.validations.filter((v) => v.etat === "attente").length : 0), 0);

useStore.subscribe((s, p) => {
  if (s.entries === p.entries && s.lecturesAConfirmer === p.lecturesAConfirmer && s.alertesIntegrite === p.alertesIntegrite && s.alertesDonnees === p.alertesDonnees) return;
  if (useConv.getState().conscience !== "ouverte" && attentes(s) > attentes(p)) useConv.getState().setConscience("ouverte");
});
