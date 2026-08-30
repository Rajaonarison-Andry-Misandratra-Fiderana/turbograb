/* =====================================================================
   TurboGrab — Material Design 3 theme for MUI.

   MUI's palette has one slot per role name (primary.main, primary.light…),
   M3 has a wider set (containers, surface tiers, outline). The bridge:
   MUI's own slots keep their meaning, and every extra M3 role is added as
   a typed field on the same palette object (see the augmentation below).
   `cssVariables` publishes all of them as CSS custom properties, so plain
   CSS (the logo mark, scrollbars) can read the same tokens as the sx prop.

   Colours come from one tonal source — the app's cyan #4CC2FF — expanded
   to the M3 light and dark schemes.
   ===================================================================== */
import { createTheme } from "@mui/material/styles";

declare module "@mui/material/styles" {
  interface Palette {
    /** M3 surface tiers: elevation expressed as tint, not shadow. */
    surfaceContainer: {
      lowest: string;
      low: string;
      main: string;
      high: string;
      highest: string;
    };
    outlineVariant: string;
    /** Roles M3 has no equivalent for in MUI's default palette. */
    ok: PaletteColor;
    caution: PaletteColor;
  }
  interface PaletteOptions {
    surfaceContainer?: Palette["surfaceContainer"];
    outlineVariant?: string;
    ok?: SimplePaletteColorOptions;
    caution?: SimplePaletteColorOptions;
  }
  interface PaletteColor {
    /** M3 `*Container` / `on*Container` pair, carried alongside main/contrastText. */
    container?: string;
    onContainer?: string;
  }
  interface SimplePaletteColorOptions {
    container?: string;
    onContainer?: string;
  }
}

// Inter: tall x-height and open apertures, which is what keeps 12-13px
// metadata legible. The stack degrades for titles in scripts the latin subset
// doesn't cover — a YouTube title can be in anything.
const FONT =
  '"Inter Variable", Inter, system-ui, -apple-system, "Segoe UI", "Noto Sans", sans-serif';

/** M3 state-layer opacities. Left to MUI's M2 defaults, hover and disabled
 *  states drift between the two schemes — a disabled button in particular
 *  became almost invisible on the dark ground. */
const action = {
  hoverOpacity: 0.08,
  focusOpacity: 0.12,
  selectedOpacity: 0.12,
  activatedOpacity: 0.12,
  disabledOpacity: 0.38,
};

const light = {
  primary: {
    main: "#00658B",
    contrastText: "#FFFFFF",
    container: "#C2E8FF",
    onContainer: "#001E2C",
  },
  secondary: {
    main: "#4E616D",
    contrastText: "#FFFFFF",
    container: "#D1E5F4",
    onContainer: "#0A1E28",
  },
  error: {
    main: "#BA1A1A",
    contrastText: "#FFFFFF",
    container: "#FFDAD6",
    onContainer: "#410002",
  },
  ok: {
    main: "#006D43",
    contrastText: "#FFFFFF",
    container: "#8FF7BD",
    onContainer: "#002114",
  },
  caution: {
    main: "#7C5800",
    contrastText: "#FFFFFF",
    container: "#FFDF9E",
    onContainer: "#271900",
  },
  background: { default: "#F6FAFE", paper: "#FFFFFF" },
  text: {
    primary: "#171C1F",
    secondary: "#40484D",
    disabled: "#70787E",
  },
  divider: "#C0C8CE",
  outlineVariant: "#C0C8CE",
  action: { ...action, disabled: "#70787E", disabledBackground: "rgba(23,28,31,0.12)" },
  surfaceContainer: {
    lowest: "#FFFFFF",
    low: "#F0F4F8",
    main: "#EAEEF2",
    high: "#E4E9EC",
    highest: "#DFE3E7",
  },
} as const;

const dark = {
  primary: {
    main: "#86CFFF",
    contrastText: "#003548",
    container: "#004C67",
    onContainer: "#C2E8FF",
  },
  secondary: {
    main: "#B5C9D7",
    contrastText: "#20333D",
    container: "#364A54",
    onContainer: "#D1E5F4",
  },
  error: {
    main: "#FFB4AB",
    contrastText: "#690005",
    container: "#93000A",
    onContainer: "#FFDAD6",
  },
  ok: {
    main: "#7FD8A0",
    contrastText: "#00391F",
    container: "#00522F",
    onContainer: "#9BF5C0",
  },
  caution: {
    main: "#F3C048",
    contrastText: "#412D00",
    container: "#5C4200",
    onContainer: "#FFDF9E",
  },
  background: { default: "#0F1417", paper: "#1B2023" },
  text: {
    primary: "#DFE3E7",
    secondary: "#BFC8CE",
    disabled: "#89929A",
  },
  divider: "#40484D",
  outlineVariant: "#40484D",
  action: { ...action, disabled: "#89929A", disabledBackground: "rgba(223,227,231,0.12)" },
  surfaceContainer: {
    lowest: "#0A0F12",
    low: "#171C1F",
    main: "#1B2023",
    high: "#262B2E",
    highest: "#303538",
  },
} as const;

/** M3 type scale, mapped onto the MUI variants the app actually renders. */
const typography = {
  fontFamily: FONT,
  // Roboto Flex is a variable face (wght 100–1000); these land on real weights.
  fontWeightRegular: 400,
  fontWeightMedium: 500,
  fontWeightBold: 700,
  h1: { fontSize: "2rem", lineHeight: 1.25, fontWeight: 600, letterSpacing: "-0.022em" },
  h2: { fontSize: "1.75rem", lineHeight: 1.29, fontWeight: 600, letterSpacing: "-0.021em" },
  h3: { fontSize: "1.5rem", lineHeight: 1.33, fontWeight: 600, letterSpacing: "-0.02em" },
  // title-large / title-medium / title-small
  h4: { fontSize: "1.375rem", lineHeight: 1.27, fontWeight: 600, letterSpacing: "-0.018em" },
  h5: { fontSize: "1rem", lineHeight: 1.5, fontWeight: 600, letterSpacing: "-0.011em" },
  h6: { fontSize: "0.875rem", lineHeight: 1.43, fontWeight: 600, letterSpacing: "-0.006em" },
  subtitle1: { fontSize: "1rem", lineHeight: 1.5, fontWeight: 600, letterSpacing: "-0.011em" },
  subtitle2: { fontSize: "0.9375rem", lineHeight: 1.4, fontWeight: 600, letterSpacing: "-0.009em" },
  body1: { fontSize: "1rem", lineHeight: 1.55, letterSpacing: "-0.006em" },
  body2: { fontSize: "0.875rem", lineHeight: 1.5, letterSpacing: "-0.003em" },
  caption: { fontSize: "0.8125rem", lineHeight: 1.45, letterSpacing: "0" },
  overline: {
    fontSize: "0.6875rem",
    lineHeight: 1.45,
    fontWeight: 500,
    letterSpacing: "0.08em",
    textTransform: "uppercase" as const,
  },
  button: {
    fontSize: "0.875rem",
    lineHeight: 1.43,
    fontWeight: 500,
    letterSpacing: "-0.003em",
    textTransform: "none" as const, // M3 buttons are sentence case, not ALL CAPS
  },
};

/** M3 roles as raw CSS variables, for use in component `sx`.
 *
 *  Deliberately not `theme.palette.*`: with `cssVariables` enabled that object
 *  is frozen to the *default* colour scheme, so reading it inside an `sx` or
 *  `styleOverrides` callback paints light values onto a dark window. Only the
 *  variables follow the active scheme. */
export const tok = {
  surfaceLowest: "var(--mui-palette-surfaceContainer-lowest)",
  surfaceLow: "var(--mui-palette-surfaceContainer-low)",
  surface: "var(--mui-palette-surfaceContainer-main)",
  surfaceHigh: "var(--mui-palette-surfaceContainer-high)",
  surfaceHighest: "var(--mui-palette-surfaceContainer-highest)",
  outlineVariant: "var(--mui-palette-outlineVariant)",
  divider: "var(--mui-palette-divider)",
  text: "var(--mui-palette-text-primary)",
  textSecondary: "var(--mui-palette-text-secondary)",
  primary: "var(--mui-palette-primary-main)",
  primaryContainer: "var(--mui-palette-primary-container)",
  onPrimaryContainer: "var(--mui-palette-primary-onContainer)",
  secondaryContainer: "var(--mui-palette-secondary-container)",
  onSecondaryContainer: "var(--mui-palette-secondary-onContainer)",
  error: "var(--mui-palette-error-main)",
  onError: "var(--mui-palette-error-contrastText)",
  errorContainer: "var(--mui-palette-error-container)",
  onErrorContainer: "var(--mui-palette-error-onContainer)",
  okContainer: "var(--mui-palette-ok-container)",
  onOkContainer: "var(--mui-palette-ok-onContainer)",
  caution: "var(--mui-palette-caution-main)",
  cautionContainer: "var(--mui-palette-caution-container)",
  onCautionContainer: "var(--mui-palette-caution-onContainer)",
} as const;

/** M3 corner scale, in pixels. One rule, applied everywhere:
 *
 *    full  buttons, chips, toggles, progress bars   (pills)
 *    sm    inputs and small controls                 (8)
 *    md    cards, menus, alerts                      (12)
 *    lg    containers that hold cards or inputs      (16)
 *    xl    dialogs                                   (28)
 *
 *  An inner radius is always smaller than the one it sits inside.
 *
 *  Always spell these as `${shape.md}px` in `sx`: a bare number there is a
 *  *multiplier* on `theme.shape.borderRadius`, so `borderRadius: 3` silently
 *  meant 36px — which is where the mismatched corners came from.
 */
export const shape = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 28,
  full: 999,
};

/** M3 emphasised easing + durations, reused by every custom transition. */
export const motion = {
  emphasized: "cubic-bezier(0.2, 0, 0, 1)",
  standard: "cubic-bezier(0.2, 0, 0, 1)",
  short: 150,
  medium: 250,
  long: 400,
};

export const theme = createTheme({
  // NOT the `media` default: that follows the OS only and silently ignores a
  // manual choice. `data` publishes `[data-mui-color-scheme="dark"]`, which the
  // pre-paint script in index.html also sets to avoid a flash of the wrong theme.
  cssVariables: { colorSchemeSelector: "data" },
  colorSchemes: {
    light: { palette: light },
    dark: { palette: dark },
  },
  typography,
  shape: { borderRadius: shape.md },
  components: {
    MuiCssBaseline: {
      styleOverrides: (t) => ({
        "html, body, #root": { height: "100%", margin: 0 },
        html: {
          // Greyscale antialiasing: on a dark ground subpixel rendering makes
          // Inter's thin strokes look coloured and furry.
          WebkitFontSmoothing: "antialiased",
          MozOsxFontSmoothing: "grayscale",
          textRendering: "optimizeLegibility",
          // cv05 gives the lowercase l a tail, so it stops reading as a 1 or an
          // uppercase I in a filename.
          fontFeatureSettings: '"cv05" 1, "calt" 1',
        },
        body: {
          // Chrome-only app (Tauri webview), so this is the whole story.
          // `alpha()` can't operate on a CSS variable; the *Channel token is
          // the raw "r g b" triplet MUI publishes for exactly this case.
          scrollbarColor: `rgba(${t.vars.palette.text.primaryChannel} / 0.28) transparent`,
          scrollbarWidth: "thin",
          overflow: "hidden",
        },
        // Text is selectable everywhere by default — an error message the user
        // can't copy is a dead end. Only chrome opts out, per component.
        "::selection": {
          background: t.vars.palette.primary.container,
          color: t.vars.palette.primary.onContainer,
        },
        "@media (prefers-reduced-motion: reduce)": {
          "*": {
            animationDuration: "0.01ms !important",
            transitionDuration: "0.01ms !important",
          },
        },
      }),
    },
    MuiButton: {
      defaultProps: { disableElevation: true },
      styleOverrides: {
        root: ({ theme: t }) => ({
          borderRadius: shape.full,
          paddingInline: 24,
          minHeight: 44,
          // M3 disabled: the surface's own ink at 12% / 38%, so it reads the
          // same way on a light and a dark ground.
          "&.Mui-disabled": {
            backgroundColor: "transparent",
            color: t.vars.palette.action.disabled,
          },
          "&.MuiButton-contained.Mui-disabled": {
            backgroundColor: t.vars.palette.action.disabledBackground,
            color: t.vars.palette.action.disabled,
          },
        }),
        sizeSmall: { minHeight: 36, paddingInline: 16 },
      },
    },
    MuiIconButton: {
      styleOverrides: {
        root: { borderRadius: shape.full, padding: 9 },
        sizeSmall: { padding: 7 },
      },
    },
    MuiToggleButton: {
      styleOverrides: {
        root: ({ theme: t }) => ({
          border: `1px solid ${t.vars.palette.outlineVariant}`,
          textTransform: "none",
          gap: 8,
          paddingBlock: 10,
          paddingInline: 18,
          color: t.vars.palette.text.secondary,
          "&.Mui-selected": {
            backgroundColor: t.vars.palette.secondary.container,
            color: t.vars.palette.secondary.onContainer,
            "&:hover": { backgroundColor: t.vars.palette.secondary.container },
          },
        }),
      },
    },
    MuiToggleButtonGroup: {
      styleOverrides: {
        root: { borderRadius: shape.full },
        grouped: {
          "&:first-of-type": { borderRadius: `${shape.full}px 0 0 ${shape.full}px` },
          "&:last-of-type": { borderRadius: `0 ${shape.full}px ${shape.full}px 0` },
        },
      },
    },
    MuiTextField: { defaultProps: { variant: "outlined" } },
    MuiPaper: {
      defaultProps: { elevation: 0 },
      styleOverrides: {
        // MUI still paints an M2 white-gradient overlay on elevated Paper in
        // dark mode. M3 expresses elevation as a surface tone, so the overlay
        // only washes menus and dialogs out. Off, everywhere.
        root: { backgroundImage: "none" },
        rounded: { borderRadius: shape.md },
      },
    },
    MuiMenu: {
      defaultProps: { elevation: 3 },
      styleOverrides: {
        paper: ({ theme: t }) => ({
          borderRadius: shape.md,
          minWidth: 224,
          paddingBlock: 8,
          backgroundColor: t.vars.palette.surfaceContainer.high,
          backgroundImage: "none",
        }),
      },
    },
    MuiPopover: {
      styleOverrides: {
        paper: ({ theme: t }) => ({
          backgroundColor: t.vars.palette.surfaceContainer.high,
          backgroundImage: "none",
        }),
      },
    },
    MuiOutlinedInput: {
      styleOverrides: {
        root: { borderRadius: shape.sm },
        notchedOutline: ({ theme: t }) => ({ borderColor: t.vars.palette.outlineVariant }),
      },
    },
    MuiSnackbarContent: {
      styleOverrides: {
        // M3 inverse surface: deliberately the opposite scheme, so a message
        // over a dark list is unmistakably a message and not another card.
        root: ({ theme: t }) => ({
          borderRadius: shape.xs,
          backgroundColor: t.vars.palette.text.primary,
          color: t.vars.palette.background.default,
          backgroundImage: "none",
        }),
      },
    },
    MuiSkeleton: {
      styleOverrides: {
        root: ({ theme: t }) => ({ backgroundColor: t.vars.palette.surfaceContainer.highest }),
      },
    },
    MuiLinearProgress: {
      styleOverrides: {
        root: ({ theme: t }) => ({
          borderRadius: shape.full,
          height: 6,
          backgroundColor: t.vars.palette.surfaceContainer.highest,
        }),
        bar: { borderRadius: shape.full },
      },
    },
    MuiMenuItem: {
      styleOverrides: {
        root: { borderRadius: shape.full, margin: "3px 8px", minHeight: 44, paddingBlock: 8 },
      },
    },
    MuiTooltip: {
      defaultProps: { enterDelay: 400 },
      styleOverrides: {
        tooltip: ({ theme: t }) => ({
          borderRadius: shape.xs,
          backgroundColor: t.vars.palette.text.primary,
          color: t.vars.palette.background.default,
          fontSize: "0.8125rem",
        }),
      },
    },
    MuiChip: {
      styleOverrides: {
        root: { fontWeight: 500 },
        sizeSmall: { height: 26 },
      },
    },
    MuiTab: {
      styleOverrides: { root: { textTransform: "none", minHeight: 48, gap: 8 } },
    },
    MuiDialog: {
      styleOverrides: {
        paper: ({ theme: t }) => ({
          borderRadius: shape.xl,
          padding: 12,
          backgroundColor: t.vars.palette.surfaceContainer.high,
          backgroundImage: "none",
        }),
      },
    },
    MuiAlert: {
      // M3 has no outlined alert: the tinted container *is* the signal, and it
      // holds a readable contrast ratio in both schemes, which a bare outline
      // drawn in `error.main` does not.
      defaultProps: { variant: "standard" },
      styleOverrides: {
        root: ({ theme: t, ownerState }) => {
          const tone = {
            error: t.vars.palette.error,
            warning: t.vars.palette.caution,
            success: t.vars.palette.ok,
            info: t.vars.palette.primary,
          }[ownerState.severity ?? "success"];
          return {
            borderRadius: shape.md,
            ...(ownerState.variant === "standard" &&
              tone && {
                backgroundColor: tone.container,
                color: tone.onContainer,
                "& .MuiAlert-icon": { color: tone.onContainer },
              }),
          };
        },
      },
    },
  },
});
