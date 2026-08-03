// Minimal inline SVG icons (stroke-based, inherit currentColor).
const S = {
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const IconDownload = () => (
  <svg {...S}>
    <path d="M12 3v12" />
    <path d="m7 12 5 5 5-5" />
    <path d="M5 21h14" />
  </svg>
);

export const IconVideo = () => (
  <svg {...S}>
    <rect x="2" y="6" width="14" height="12" rx="2" />
    <path d="m22 8-6 4 6 4V8Z" />
  </svg>
);

export const IconAudio = () => (
  <svg {...S}>
    <path d="M9 18V5l12-2v13" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="18" cy="16" r="3" />
  </svg>
);

export const IconPause = () => (
  <svg {...S}>
    <rect x="6" y="5" width="4" height="14" rx="1" />
    <rect x="14" y="5" width="4" height="14" rx="1" />
  </svg>
);

export const IconPlay = () => (
  <svg {...S} fill="currentColor" stroke="none">
    <path d="M7 4v16l13-8L7 4Z" />
  </svg>
);

export const IconX = () => (
  <svg {...S}>
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </svg>
);

export const IconFolder = () => (
  <svg {...S}>
    <path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-7l-2-2H4a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1Z" />
  </svg>
);

export const IconResume = () => (
  <svg {...S}>
    <path d="M3 12a9 9 0 1 0 3-6.7" />
    <path d="M3 4v4h4" />
  </svg>
);

export const IconClear = () => (
  <svg {...S}>
    <path d="M3 6h18" />
    <path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2" />
    <path d="M6 6v14a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6" />
  </svg>
);

export const IconSearch = () => (
  <svg {...S}>
    <circle cx="11" cy="11" r="7" />
    <path d="m21 21-4.3-4.3" />
  </svg>
);

export const IconWarn = () => (
  <svg {...S}>
    <path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" />
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
  </svg>
);

export const IconFile = () => (
  <svg {...S}>
    <path d="M14 3v5h5" />
    <path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8l-5-5Z" />
  </svg>
);

export const IconLink = () => (
  <svg {...S}>
    <path d="M9 15 15 9" />
    <path d="M10.5 6.5 12 5a4 4 0 0 1 5.7 5.7l-1.5 1.5" />
    <path d="M13.5 17.5 12 19a4 4 0 0 1-5.7-5.7l1.5-1.5" />
  </svg>
);

// Brand mark: download arrow + turbo speed streaks (matches the app icon).
export const IconLogo = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
    <g
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeOpacity="0.55"
    >
      <path d="M3 7h4" />
      <path d="M2 12h5" />
      <path d="M3 17h4" />
    </g>
    <g stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M15 4v10" />
      <path d="m11 11 4 4 4-4" />
      <path d="M10 20h10" />
    </g>
  </svg>
);
