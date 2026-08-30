// The brand mark. Every other glyph comes from @mui/icons-material.

/** App icon: a rounded tile with the download glyph knocked out of it.
 *  The tile takes currentColor and the glyph is punched through with a mask,
 *  so it reads on any surface in either colour scheme. */
export const IconLogo = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 48 48" fill="none" aria-hidden>
    <mask id="tg-logo-cut">
      <rect width="48" height="48" rx="11" fill="white" />
      <g
        stroke="black"
        strokeWidth="5"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      >
        <path d="M24 11v19" />
        <path d="m14.5 21.5 9.5 9.5 9.5-9.5" />
        <path d="M13.5 38h21" />
      </g>
    </mask>
    <rect width="48" height="48" rx="11" fill="currentColor" mask="url(#tg-logo-cut)" />
  </svg>
);
