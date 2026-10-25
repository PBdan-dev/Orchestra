import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../tauri";
import { Markdown } from "../markdown";
import type { MemoryFile } from "../types";
import { I } from "./Icons";

/** Vue Mémoire : les fichiers .md tels qu'ils sont, lisibles et modifiables. */
export function MemoryView() {
  const [files, setFiles] = useState<MemoryFile[]>([]);
  const focus = useStore((s) => s.memoryFocus);
  const setFocus = useStore((s) => s.setMemoryFocus);
  const entriesCount = useStore((s) => s.entries.length);
  const [sel, setSel] = useState<string>(focus ?? "memoire/orchestra.md");
  const [text, setText] = useState("");
  const [orig, setOrig] = useState("");
  const [mode, setMode] = useState<"read" | "edit">("read");
  const [saved, setSaved] = useState(false);

  const load = async () => {
    const list = await api.memoryTree();
    setFiles(list);
  };
  useEffect(() => { load(); }, [entriesCount]);
  useEffect(() => { if (focus) { setSel(focus); setFocus(null); } }, [focus]);
  useEffect(() => {
    api.readTextFile(sel).then((t) => { setText(t); setOrig(t); }).catch(() => { setText(""); setOrig(""); });
  }, [sel, entriesCount]);

  const save = async () => {
    await api.writeTextFile(sel, text);
    setOrig(text);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
    load();
  };

  const groups: [string, MemoryFile[]][] = [
    ["Mémoire principale", files.filter((f) => f.kind === "principal")],
    ["Agents", files.filter((f) => f.kind === "agent")],
    ["Archives de conversations", files.filter((f) => f.kind === "archive")],
  ];
  const dirty = text !== orig;
  const current = files.find((f) => f.rel === sel);

  return (
    <div className="view memory-view">
      <aside className="file-list">
        <div className="file-list-head">
          <span>Documents/Orchestra/memoire</span>
          <button className="icon-btn" onClick={() => api.openFolder("memoire")} title="Ouvrir le dossier"><I.external size={14} /></button>
        </div>
        {groups.map(([title, list]) => (
          <div key={title} className="file-group">
            <div className="file-group-title">{title}</div>
            {list.length === 0 && <div className="muted small pad">— vide —</div>}
            {list.map((f) => (
              <button key={f.rel} className={`file-row ${sel === f.rel ? "active" : ""}`} onClick={() => setSel(f.rel)}>
                <I.memory size={13} />
                <span className="file-name">{f.name}</span>
                <span className="muted small">{f.modified}</span>
              </button>
            ))}
          </div>
        ))}
        <div className="file-note muted small">
          Une ligne « - (date) … » = un fait. Ce que tu effaces ici est oublié ; ce que tu ajoutes est retenu.
        </div>
      </aside>
      <section className="file-editor">
        <header className="editor-head">
          <div><code>{sel}</code> {current && <span className="muted small">· {current.size} o · {current.modified}</span>}</div>
          <div className="editor-actions">
            <div className="seg">
              <button className={mode === "read" ? "active" : ""} onClick={() => setMode("read")}>Lecture</button>
              <button className={mode === "edit" ? "active" : ""} onClick={() => setMode("edit")}>Édition</button>
            </div>
            <button className="btn primary" onClick={save} disabled={!dirty}>{saved ? <><I.check size={14} /> Enregistré</> : <><I.save size={14} /> Enregistrer</>}</button>
          </div>
        </header>
        {mode === "read" ? (
          <div className="editor-read"><Markdown text={text} /></div>
        ) : (
          <textarea className="editor-area" value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} />
        )}
      </section>
    </div>
  );
}
