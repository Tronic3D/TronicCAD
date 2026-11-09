// TronicCAD's own icon set, drawn for this project (see IP_COMPLIANCE.md).
import { ReactNode } from "react";

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      className="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const icons = {
  logo: (
    <svg className="logo" viewBox="0 0 1024 1024" aria-hidden="true">
      <rect x="64" y="64" width="896" height="896" rx="190" fill="#10242b" />
      <path d="M250 300H774" stroke="#35c6a8" strokeWidth="84" strokeLinecap="round" />
      <path
        d="M512 300V600L700 788"
        stroke="#35c6a8"
        strokeWidth="84"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
      <circle cx="250" cy="300" r="72" fill="#10242b" stroke="#f0a35e" strokeWidth="44" />
      <circle cx="774" cy="300" r="72" fill="#10242b" stroke="#f0a35e" strokeWidth="44" />
      <circle cx="700" cy="788" r="72" fill="#10242b" stroke="#f0a35e" strokeWidth="44" />
    </svg>
  ),
  newFile: (
    <Icon>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4M12 11v6M9 14h6" />
    </Icon>
  ),
  open: (
    <Icon>
      <path d="M3 7V5h6l2 2h8v3" />
      <path d="M3 19 5.5 10H22l-2.5 9z" />
    </Icon>
  ),
  save: (
    <Icon>
      <path d="M4 4h13l3 3v13H4z" />
      <path d="M8 4v5h7V4M7 20v-7h10v7" />
    </Icon>
  ),
  saveAs: (
    <Icon>
      <path d="M12 20H4V4h13l3 3v4" />
      <path d="M8 4v5h7V4M14 21l1-3.5 5-5 2.5 2.5-5 5z" />
    </Icon>
  ),
  close: (
    <Icon>
      <path d="m6 6 12 12M18 6 6 18" />
    </Icon>
  ),
  project: (
    <Icon>
      <path d="M4 5h16v14H4zM4 9h16M9 9v10" />
    </Icon>
  ),
  schematic: (
    <Icon>
      <path d="M2 12h5l1.5-3 3 6 3-6L16 12h6" />
    </Icon>
  ),
  pcb: (
    <Icon>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M7 16v-4l3-3h4" />
      <circle cx="16" cy="9" r="1.6" />
      <circle cx="7" cy="17" r="0.6" />
      <path d="M12 16h5" />
    </Icon>
  ),
  library: (
    <Icon>
      <path d="M6 7h12v10H6zM9 7V3M15 7V3M9 21v-4M15 21v-4" />
    </Icon>
  ),
  view3d: (
    <Icon>
      <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9" />
    </Icon>
  ),
  simulation: (
    <Icon>
      <path d="M3 20V4M3 20h18M6 15c2-8 3.5-8 5.5-3s3.5 5 6.5-4" />
    </Icon>
  ),
  ai: (
    <Icon>
      <path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z" />
    </Icon>
  ),
  pointer: (
    <Icon>
      <path d="M6 3v15l4-4 3 7 3-1-3-7h6z" />
    </Icon>
  ),
  wire: (
    <Icon>
      <path d="M3 18h7V6h11" />
      <circle cx="3" cy="18" r="1.3" />
      <circle cx="21" cy="6" r="1.3" />
    </Icon>
  ),
  bus: (
    <Icon>
      <path d="M3 9h18M3 15h18M8 6l-2 12M18 6l-2 12" />
    </Icon>
  ),
  label: (
    <Icon>
      <path d="M3 7h12l6 5-6 5H3z" />
      <circle cx="7" cy="12" r="1" />
    </Icon>
  ),
  noConnect: (
    <Icon>
      <path d="M2 12h8M7 7l10 10M17 7 7 17" />
    </Icon>
  ),
  junction: (
    <Icon>
      <path d="M12 3v18M3 12h18" />
      <circle cx="12" cy="12" r="2.4" fill="currentColor" />
    </Icon>
  ),
  block: (
    <Icon>
      <rect x="6" y="4" width="14" height="16" rx="1" />
      <path d="M2 9h4M2 15h4M10 9h6M10 13h6" />
    </Icon>
  ),
  undo: (
    <Icon>
      <path d="M8 5 3 10l5 5" />
      <path d="M3 10h11a6 6 0 0 1 0 12h-3" />
    </Icon>
  ),
  redo: (
    <Icon>
      <path d="m16 5 5 5-5 5" />
      <path d="M21 10H10a6 6 0 0 0 0 12h3" />
    </Icon>
  ),
  rotate: (
    <Icon>
      <path d="M20 12a8 8 0 1 1-2.6-5.9" />
      <path d="M20 3v5h-5" />
    </Icon>
  ),
  mirror: (
    <Icon>
      <path d="M12 3v18" strokeDasharray="2 2.5" />
      <path d="M9 7 3 17h6zM15 7l6 10h-6z" />
    </Icon>
  ),
  trash: (
    <Icon>
      <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v6M14 11v6" />
    </Icon>
  ),
  annotate: (
    <Icon>
      <path d="M9 4 7 20M17 4l-2 16M4 9h16M3 15h16" />
    </Icon>
  ),
  fit: (
    <Icon>
      <path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5" />
    </Icon>
  ),
  plus: (
    <Icon>
      <path d="M12 5v14M5 12h14" />
    </Icon>
  ),
  cloud: (
    <Icon>
      <path d="M7 18a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 9.5a4.3 4.3 0 0 1-.5 8.5z" />
    </Icon>
  ),
  extensions: (
    <Icon>
      <path d="M10 4h4v3h3a1 1 0 0 1 1 1v3h3v4h-3v3a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-3H3v-4h3V8a1 1 0 0 1 1-1h3z" />
    </Icon>
  ),
  help: (
    <Icon>
      <circle cx="12" cy="12" r="9" />
      <path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7M12 17h.01" />
    </Icon>
  ),
  update: (
    <Icon>
      <path d="M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4" />
    </Icon>
  ),
  license: (
    <Icon>
      <circle cx="8" cy="15" r="4" />
      <path d="M11 12l8-8M16 7l3 3M13.5 9.5l2 2" />
    </Icon>
  ),
  team: (
    <Icon>
      <path d="M7 18a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 9.5a4.3 4.3 0 0 1-.5 8.5z" />
    </Icon>
  ),
};
