import React from "react";

type P = { size?: number; className?: string; style?: React.CSSProperties };
const base = (size = 16) => ({ width: size, height: size, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const });

export const I = {
  mic: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/></svg>
  ),
  speaker: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12"/></svg>
  ),
  brain: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M9 4a3 3 0 0 0-3 3v1a3 3 0 0 0-2 3 3 3 0 0 0 2 3v1a3 3 0 0 0 3 3h1V4H9zM15 4a3 3 0 0 1 3 3v1a3 3 0 0 1 2 3 3 3 0 0 1-2 3v1a3 3 0 0 1-3 3h-1V4h1z"/></svg>
  ),
  memory: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 5h11l5 5v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z"/><path d="M15 5v5h5M7 14h10M7 17h6"/></svg>
  ),
  search: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><circle cx="11" cy="11" r="6"/><path d="m20 20-4.5-4.5"/></svg>
  ),
  plug: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M9 3v5M15 3v5M7 8h10v3a5 5 0 0 1-10 0V8zM12 16v5"/></svg>
  ),
  tool: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M14.5 5.5a4 4 0 0 0-5 5L4 16l4 4 5.5-5.5a4 4 0 0 0 5-5l-2.5 2.5-2-2 2.5-2.5z"/></svg>
  ),
  spark: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M5.6 18.4l2.8-2.8M15.6 8.4l2.8-2.8"/></svg>
  ),
  save: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M5 4h11l3 3v13H5z"/><path d="M8 4v5h7V4M8 20v-6h8v6"/></svg>
  ),
  check: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="m5 12 4 4L19 6"/></svg>
  ),
  x: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M6 6l12 12M18 6 6 18"/></svg>
  ),
  alert: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M12 3 2 20h20L12 3zM12 10v4M12 17v.5"/></svg>
  ),
  gear: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>
  ),
  folder: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z"/></svg>
  ),
  pen: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 20h4l10.5-10.5a2 2 0 0 0 0-3L17 5a2 2 0 0 0-3 0L4 15.5V20z"/><path d="m13.5 6.5 4 4"/></svg>
  ),
  journal: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M5 4h12a2 2 0 0 1 2 2v14H7a2 2 0 0 1-2-2V4z"/><path d="M5 4v14a2 2 0 0 0 2 2M9 8h6M9 12h6"/></svg>
  ),
  stage: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><circle cx="12" cy="12" r="4"/><circle cx="12" cy="12" r="9" strokeDasharray="3 4"/></svg>
  ),
  send: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 12 20 4l-4 16-4-7-8-1z"/></svg>
  ),
  external: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M14 4h6v6M20 4l-9 9M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/></svg>
  ),
  chat: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 5h16v11H9l-5 4V5z"/></svg>
  ),
  trash: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/></svg>
  ),
  mail: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/></svg>
  ),
  calendar: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/></svg>
  ),
  globe: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/></svg>
  ),
  plus: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M12 5v14M5 12h14"/></svg>
  ),
  minus: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M5 12h14"/></svg>
  ),
  unplug: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M9 3v4M15 3v4M7 7h10v3a5 5 0 0 1-5 5 5 5 0 0 1-5-5V7zM12 15v6M4 4l16 16"/></svg>
  ),
  play: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M7 5v14l12-7z"/></svg>
  ),
  power: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M12 3v8M6.3 7a8 8 0 1 0 11.4 0"/></svg>
  ),
  refresh: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/></svg>
  ),
  grid: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><rect x="14" y="14" width="6" height="6" rx="1"/></svg>
  ),
  user: ({ size, className, style }: P) => (
    <svg {...base(size)} className={className} style={style}><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></svg>
  ),
};

export function AgentGlyph({ icon, size = 18 }: { icon: string; size?: number }) {
  const C = (I as any)[icon] ?? I.spark;
  return <C size={size} />;
}
