import React from "react";

/** Rendu Markdown minimal et sûr (texte échappé, pas de HTML brut). */
export function Markdown({ text }: { text: string }) {
  const blocks = splitBlocks(text);
  return <div className="md">{blocks.map((b, i) => <Block key={i} b={b} />)}</div>;
}

type B =
  | { t: "code"; lang: string; body: string }
  | { t: "h"; level: number; body: string }
  | { t: "ul"; items: string[] }
  | { t: "ol"; items: string[] }
  | { t: "quote"; body: string }
  | { t: "p"; body: string };

function splitBlocks(text: string): B[] {
  const lines = text.replace(/\r/g, "").split("\n");
  const out: B[] = [];
  let i = 0;
  while (i < lines.length) {
    const l = lines[i];
    if (l.startsWith("```")) {
      const lang = l.slice(3).trim();
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) body.push(lines[i++]);
      i++;
      out.push({ t: "code", lang, body: body.join("\n") });
      continue;
    }
    const h = /^(#{1,4})\s+(.*)$/.exec(l);
    if (h) { out.push({ t: "h", level: h[1].length, body: h[2] }); i++; continue; }
    if (/^\s*[-*•]\s+/.test(l)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*•]\s+/.test(lines[i])) items.push(lines[i++].replace(/^\s*[-*•]\s+/, ""));
      out.push({ t: "ul", items });
      continue;
    }
    if (/^\s*\d+[.)]\s+/.test(l)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+[.)]\s+/.test(lines[i])) items.push(lines[i++].replace(/^\s*\d+[.)]\s+/, ""));
      out.push({ t: "ol", items });
      continue;
    }
    if (l.startsWith(">")) {
      const q: string[] = [];
      while (i < lines.length && lines[i].startsWith(">")) q.push(lines[i++].replace(/^>\s?/, ""));
      out.push({ t: "quote", body: q.join("\n") });
      continue;
    }
    if (l.trim() === "") { i++; continue; }
    const p: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^(```|#{1,4}\s|\s*[-*•]\s|\s*\d+[.)]\s|>)/.test(lines[i])) p.push(lines[i++]);
    out.push({ t: "p", body: p.join("\n") });
  }
  return out;
}

function Block({ b }: { b: B }) {
  switch (b.t) {
    case "code": return <pre className="md-code"><code>{b.body}</code></pre>;
    case "h": return React.createElement(`h${Math.min(4, b.level + 2)}`, { className: "md-h" }, inline(b.body));
    case "ul": return <ul>{b.items.map((it, i) => <li key={i}>{inline(it)}</li>)}</ul>;
    case "ol": return <ol>{b.items.map((it, i) => <li key={i}>{inline(it)}</li>)}</ol>;
    case "quote": return <blockquote>{inline(b.body)}</blockquote>;
    case "p": return <p>{inline(b.body)}</p>;
  }
}

function inline(s: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*\n]+\*|_[^_\n]+_|https?:\/\/[^\s)]+)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push(withBreaks(s.slice(last, m.index), k++));
    const tok = m[0];
    if (tok.startsWith("**")) out.push(<strong key={k++}>{tok.slice(2, -2)}</strong>);
    else if (tok.startsWith("`")) out.push(<code key={k++}>{tok.slice(1, -1)}</code>);
    else if (tok.startsWith("http")) out.push(<span key={k++} className="md-link">{tok}</span>);
    else out.push(<em key={k++}>{tok.slice(1, -1)}</em>);
    last = m.index + tok.length;
  }
  if (last < s.length) out.push(withBreaks(s.slice(last), k++));
  return out;
}

function withBreaks(s: string, key: number) {
  const parts = s.split("\n");
  return (
    <React.Fragment key={key}>
      {parts.map((p, i) => (
        <React.Fragment key={i}>{p}{i < parts.length - 1 && <br />}</React.Fragment>
      ))}
    </React.Fragment>
  );
}
