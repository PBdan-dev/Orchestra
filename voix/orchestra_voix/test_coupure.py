"""Test du lot D : coupure de parole (D3) et réflexion à la demande (D5).

    python -m orchestra_voix.test_coupure question.wav coupure.wav [--port 8765]

Envoie `question.wav` ; quand la réponse est dite depuis 1,5 s, envoie `coupure.wav` par-dessus,
comme si l'utilisateur coupait la parole. Le test fait comme l'interface (qui tient la conversation,
D1) : il garde le texte complet de la réponse (`bot_text`) et les phrases réellement dites
(`bot_dit`, envoyées quand leur audio part), et à la coupure ne garde que la partie dite, suivie de
« [interrompu] ». Il vérifie aussi qu'aucune trace de l'outil `reflechir` ne reste dans la
conversation du serveur (`contexte_debug`).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import time

import websockets

from .test_dialogue import STEP, load


def partie_dite(contenu: str, dit: str) -> str:
    """Même règle que l'interface (store.ts, partieDite)."""
    d = dit.strip()
    if len(d) >= len(contenu.strip()):
        return contenu
    return f"{d} [interrompu]" if d else "[interrompu]"


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("question")
    ap.add_argument("coupure")
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()

    async with websockets.connect(f"ws://127.0.0.1:{a.port}", max_size=None) as ws:
        e_ = {"son": asyncio.Event(), "dernier_son": 0.0, "contexte": None, "lecture": False}
        reponses: list[dict] = []  # une par bot_start : texte complet et texte dit
        coupees: list[dict] = []

        async def recevoir():
            async for m in ws:
                if isinstance(m, bytes):
                    e_["son"].set()
                    e_["dernier_son"] = time.perf_counter()
                    continue
                e = json.loads(m)
                t = e["type"]
                if t in ("user_final", "decision", "thinking"):
                    print(f"    {e}")
                if t == "bot_start":
                    reponses.append({"complet": "", "dit": ""})
                elif t == "bot_text" and reponses:
                    reponses[-1]["complet"] += e["text"]
                elif t == "bot_dit" and reponses:
                    print(f"    bot_dit : {e['text']!r}")
                    d = e["text"].strip()
                    if d and d in reponses[-1]["complet"]:
                        reponses[-1]["dit"] = f"{reponses[-1]['dit']} {d}".strip()
                elif t == "interrupt":
                    # en lecture : du son est arrivé il y a moins de 0,5 s
                    if reponses and time.perf_counter() - e_["dernier_son"] < 0.5:
                        r = reponses[-1]
                        coupees.append({**r, "garde": partie_dite(r["complet"], r["dit"])})
                        print("    {'type': 'interrupt'} pendant la lecture d'une réponse")
                elif t == "contexte":
                    e_["contexte"] = e["messages"]

        tache = asyncio.create_task(recevoir())
        await asyncio.sleep(0.5)

        async def silence(secondes):
            for _ in range(int(secondes / 0.02)):
                await ws.send(bytes(STEP))
                await asyncio.sleep(0.02)

        async def dire(chemin):
            pcm, _ = load(chemin)
            debut = time.perf_counter()
            for i in range(0, len(pcm), STEP):
                await ws.send(pcm[i : i + STEP])
                attente = debut + (i + STEP) / 32000 - time.perf_counter()
                if attente > 0:
                    await asyncio.sleep(attente)

        await silence(0.5)
        print(f"> {a.question}")
        await dire(a.question)
        limite = time.perf_counter() + 60
        while time.perf_counter() < limite and not e_["son"].is_set():
            await silence(0.1)
        await silence(1.5)
        print(f"> {a.coupure} (pendant que la réponse est dite)")
        await dire(a.coupure)
        # réponse à la coupure : attendre son début puis la fin de sa lecture
        limite = time.perf_counter() + 90
        n = len(reponses)
        while time.perf_counter() < limite and len(reponses) <= n:
            await silence(0.1)
        while time.perf_counter() < limite and time.perf_counter() - e_["dernier_son"] < 2.5:
            await silence(0.1)
        await ws.send(json.dumps({"type": "contexte_debug"}))
        for _ in range(50):
            if e_["contexte"] is not None:
                break
            await asyncio.sleep(0.1)
        tache.cancel()

    print()
    for c in coupees:
        print(f"Réponse complète du modèle ({len(c['complet'])} car.) : {c['complet'][:300]!r}")
        print(f"Gardée dans la conversation : {c['garde'][:300]!r}")
    msgs = e_["contexte"] or []
    traces = [m for m in msgs if "reflechir" in m["outil"] or (m["role"] == "tool" and "reponse_dite" in m["texte"])]
    ok_d3 = bool(coupees) and all(c["garde"].endswith("[interrompu]") and len(c["garde"]) <= len(c["complet"]) + 13 for c in coupees)
    print(f"\nD3 coupure : {'OK, seule la partie dite reste, suivie de [interrompu]' if ok_d3 else 'NON (aucune coupure pendant une lecture)'}")
    print(f"D5 trace de reflechir dans la conversation du serveur : {'aucune (OK)' if not traces else f'{len(traces)} (NON)'}")


if __name__ == "__main__":
    asyncio.run(main())
