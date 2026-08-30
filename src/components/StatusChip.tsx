import Chip from "@mui/material/Chip";
import CheckCircleOutlined from "@mui/icons-material/CheckCircleOutlined";
import DownloadingOutlined from "@mui/icons-material/DownloadingOutlined";
import ErrorOutlineOutlined from "@mui/icons-material/ErrorOutlineOutlined";
import HourglassTopOutlined from "@mui/icons-material/HourglassTopOutlined";
import PauseCircleOutlined from "@mui/icons-material/PauseCircleOutlined";
import PlayCircleOutlined from "@mui/icons-material/PlayCircleOutlined";
import WarningAmberOutlined from "@mui/icons-material/WarningAmberOutlined";
import { tok } from "../theme";
import type { Status } from "../types";

/** The card's single status signal.
 *
 *  It used to be said three times — a pill, a coloured rail down the card's
 *  left edge, and a recoloured progress bar. One is enough, and pairing colour
 *  with an icon keeps it readable without colour vision. */
const LOOK: Record<
  Status,
  { Icon: typeof CheckCircleOutlined; bg: string; fg: string }
> = {
  fetching: { Icon: HourglassTopOutlined, bg: tok.surfaceHighest, fg: tok.textSecondary },
  ready: { Icon: PlayCircleOutlined, bg: tok.secondaryContainer, fg: tok.onSecondaryContainer },
  downloading: { Icon: DownloadingOutlined, bg: tok.primaryContainer, fg: tok.onPrimaryContainer },
  paused: { Icon: PauseCircleOutlined, bg: tok.cautionContainer, fg: tok.onCautionContainer },
  interrupted: { Icon: WarningAmberOutlined, bg: tok.cautionContainer, fg: tok.onCautionContainer },
  done: { Icon: CheckCircleOutlined, bg: tok.okContainer, fg: tok.onOkContainer },
  error: { Icon: ErrorOutlineOutlined, bg: tok.errorContainer, fg: tok.onErrorContainer },
};

export function StatusChip({ status, label }: { status: Status; label: string }) {
  const { Icon, bg, fg } = LOOK[status];
  return (
    <Chip
      icon={<Icon />}
      label={label}
      sx={{
        bgcolor: bg,
        color: fg,
        "& .MuiChip-icon": { color: "inherit", fontSize: 18 },
      }}
    />
  );
}
