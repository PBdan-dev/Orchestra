import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

/** Conversations, thèmes et dossiers : fichiers lisibles dans `Orchestra/conversations/`. */
export interface Theme { id: string; nom: string; couleur: string }
export interface Dossier { id: string; nom: string; parent: string | null; themes: string[]; projet: string | null }
export interface Organisation { themes: Theme[]; dossiers: Dossier[]; onglets: string[]; active: string | null }
export interface MessageConv { role: "user" | "assistant"; content: string; contexte?: string; at: number; intent?: string; agent?: string; voice?: boolean }
export interface SessionConv { couvre: number; resume: string | null; debut_le: string | null }
export interface Conversation {
  id: string; titre: string; cree_le: number; modifie_le: number; themes: string[];
  dossier: string | null; contexte_dossier: string | null; session: SessionConv; messages: MessageConv[];
}
export interface Fiche {
  id: string; titre: string; cree_le: number; modifie_le: number; themes: string[];
  dossier: string | null; contexte_dossier: string | null; nb_messages: number; apercu: string;
}
export interface EtatConversations { organisation: Organisation; conversations: Fiche[]; illisibles: string[] }

const real = {
  etat: () => invoke<EtatConversations>("conv_etat"),
  enregistrerOrganisation: (organisation: Organisation) => invoke<void>("conv_enregistrer_organisation", { organisation }),
  nouvelle: (dossier: string | null, contexteDossier: string | null) => invoke<Conversation>("conv_nouvelle", { dossier, contexteDossier }),
  ouvrir: (id: string) => invoke<Conversation>("conv_ouvrir", { id }),
  enregistrer: (conversation: Conversation, active: boolean) => invoke<Fiche>("conv_enregistrer", { conversation, active }),
  modifier: (id: string, p: { titre?: string; themes?: string[]; dossier?: string | null }) =>
    invoke<Fiche>("conv_modifier", { id, titre: p.titre ?? null, themes: p.themes ?? null, dossier: p.dossier ?? null, changerDossier: "dossier" in p }),
  supprimer: (id: string) => invoke<void>("conv_supprimer", { id }),
  contexte: (dossier: string) => invoke<string>("conv_contexte", { dossier }),
};

// ---------- démo navigateur : même comportement, rangé dans le stockage du navigateur ----------

const CLE = "orchestra.demo.conversations";
type Demo = { org: Organisation; convs: Record<string, Conversation> };
const lireDemo = (): Demo => {
  try {
    const v = localStorage.getItem(CLE);
    if (v) return JSON.parse(v) as Demo;
  } catch { /* sans stockage */ }
  return { org: { themes: [], dossiers: [], onglets: [], active: null }, convs: {} };
};
let demo = lireDemo();
const garder = () => { try { localStorage.setItem(CLE, JSON.stringify(demo)); } catch { /* sans stockage */ } };
const idDemo = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}_${Math.random().toString(16).slice(2, 6)}`;
};
export function titreDe(c: { titre: string; messages?: MessageConv[] }): string {
  if (c.titre.trim()) return c.titre.trim();
  const m = c.messages?.find((x) => x.role === "user" && x.content.trim());
  if (!m) return "Nouvelle conversation";
  const t = m.content.trim().replace(/\n/g, " ");
  return t.length > 60 ? t.slice(0, 60) + "…" : t;
}
const ficheDe = (c: Conversation): Fiche => {
  const dernier = [...c.messages].reverse().find((m) => m.content.trim());
  const a = dernier ? dernier.content.trim().replace(/\n/g, " ") : "";
  return { id: c.id, titre: titreDe(c), cree_le: c.cree_le, modifie_le: c.modifie_le, themes: c.themes, dossier: c.dossier, contexte_dossier: c.contexte_dossier, nb_messages: c.messages.length, apercu: a.length > 90 ? a.slice(0, 90) + "…" : a };
};

const mock: typeof real = {
  etat: async () => ({ organisation: demo.org, conversations: Object.values(demo.convs).sort((a, b) => b.modifie_le - a.modifie_le).map(ficheDe), illisibles: [] }),
  enregistrerOrganisation: async (o) => { demo.org = o; garder(); },
  nouvelle: async (dossier, contexte_dossier) => ({ id: idDemo(), titre: "", cree_le: Date.now(), modifie_le: Date.now(), themes: [], dossier, contexte_dossier, session: { couvre: 0, resume: null, debut_le: null }, messages: [] }),
  ouvrir: async (id) => {
    const c = demo.convs[id];
    if (!c) throw new Error(`conversation ${id} introuvable`);
    return c;
  },
  enregistrer: async (c, active) => {
    const ancien = demo.convs[c.id];
    demo.convs[c.id] = active || !ancien ? c : { ...c, messages: ancien.messages, session: ancien.session };
    garder();
    return ficheDe(demo.convs[c.id]);
  },
  modifier: async (id, p) => {
    const c = demo.convs[id];
    if (!c) throw new Error(`conversation ${id} introuvable`);
    if (p.titre !== undefined) c.titre = p.titre.trim();
    if (p.themes !== undefined) c.themes = p.themes;
    if ("dossier" in p) c.dossier = p.dossier ?? null;
    garder();
    return ficheDe(c);
  },
  supprimer: async (id) => { delete demo.convs[id]; garder(); },
  contexte: async (dossier) => {
    const d = demo.org.dossiers.find((x) => x.id === dossier);
    if (!d) throw new Error("dossier introuvable");
    const ids = new Set<string>([dossier]);
    for (let k = 0; k < 20; k++) demo.org.dossiers.forEach((x) => { if (x.parent && ids.has(x.parent)) ids.add(x.id); });
    const cs = Object.values(demo.convs).filter((c) => c.dossier && ids.has(c.dossier) && c.messages.length);
    return `## Dossier « ${d.nom} » (contexte de projet)\n${cs.length} conversation(s)${d.projet ? `, projet « ${d.projet} »` : ""}.\n\n` +
      cs.map((c) => `### ${titreDe(c)}\n` + c.messages.map((m) => `${m.role === "user" ? "PB" : "Orchestra"} : ${m.content}`).join("\n")).join("\n\n");
  },
};

export const convApi: typeof real = isTauri ? real : mock;
