import type { ReactNode } from "react";
import Box from "@mui/material/Box";
import IconButton from "@mui/material/IconButton";
import Stack from "@mui/material/Stack";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import CloseOutlined from "@mui/icons-material/CloseOutlined";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconLogo } from "../icons";
import { tok } from "../theme";

/** The window's own title bar.
 *
 *  The app runs with `decorations: false` so tiling compositors (Hyprland,
 *  Sway, river…) get the window itself and not a server-side frame they have
 *  to work around. That makes this the *only* chrome, not a second copy of a
 *  native one.
 *
 *  Close is the single button: minimise and maximise are the compositor's job
 *  everywhere this matters, and the drag region still double-clicks to
 *  maximise for anyone on a stacking WM. */
export function TitleBar({
  actions,
  closeLabel,
}: {
  actions?: ReactNode;
  closeLabel: string;
}) {
  return (
    <Stack
      direction="row"
      // Tauri turns any element carrying this attribute into a drag handle,
      // double-click included. Children with their own handlers opt out.
      data-tauri-drag-region
      sx={{
        alignItems: "center",
        gap: 1,
        // The close button carries 9px of its own padding, so a smaller right
        // inset lands on the same optical margin as the left.
        pl: 2,
        pr: 1,
        height: 44,
        flexShrink: 0,
        userSelect: "none",
        borderBottom: `1px solid ${tok.divider}`,
        bgcolor: tok.surfaceLow,
      }}
    >
      <Box
        data-tauri-drag-region
        sx={{ display: "grid", placeItems: "center", color: tok.primary }}
      >
        <IconLogo size={18} />
      </Box>
      <Typography
        data-tauri-drag-region
        variant="subtitle2"
        sx={{ flex: 1, letterSpacing: "0.02em" }}
      >
        TurboGrab
      </Typography>

      {actions}

      {/* Closing hides to the tray — the backend intercepts CloseRequested. */}
      <Tooltip title={closeLabel}>
        <IconButton
          size="small"
          aria-label={closeLabel}
          onClick={() => getCurrentWindow().close()}
          sx={{
            ml: 0.5,
            "&:hover": { bgcolor: tok.error, color: tok.onError },
          }}
        >
          <CloseOutlined sx={{ fontSize: 18 }} />
        </IconButton>
      </Tooltip>
    </Stack>
  );
}
