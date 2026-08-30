import { useState } from "react";
import Alert from "@mui/material/Alert";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import IconButton from "@mui/material/IconButton";
import LinearProgress from "@mui/material/LinearProgress";
import ListItemIcon from "@mui/material/ListItemIcon";
import ListItemText from "@mui/material/ListItemText";
import Menu from "@mui/material/Menu";
import MenuItem from "@mui/material/MenuItem";
import Paper from "@mui/material/Paper";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import BugReportOutlined from "@mui/icons-material/BugReportOutlined";
import ContentCopyOutlined from "@mui/icons-material/ContentCopyOutlined";
import DeleteOutlineOutlined from "@mui/icons-material/DeleteOutlineOutlined";
import DownloadOutlined from "@mui/icons-material/DownloadOutlined";
import FolderOpenOutlined from "@mui/icons-material/FolderOpenOutlined";
import LinkOutlined from "@mui/icons-material/LinkOutlined";
import MoreVertOutlined from "@mui/icons-material/MoreVertOutlined";
import OpenInNewOutlined from "@mui/icons-material/OpenInNewOutlined";
import PauseOutlined from "@mui/icons-material/PauseOutlined";
import PlayArrowOutlined from "@mui/icons-material/PlayArrowOutlined";
import RefreshOutlined from "@mui/icons-material/RefreshOutlined";

import { errorText } from "../errors";
import { MetaLine } from "./MetaLine";
import { QualityPicker } from "./QualityPicker";
import { SegmentBar } from "./SegmentBar";
import { Thumb } from "./Thumb";
import * as fmt from "../format";
import { tok } from "../theme";
import type { Dict, Lang } from "../i18n";
import { isExpired, type DownloadInfo } from "../types";

export interface CardActions {
  start: (d: DownloadInfo, quality: string) => void;
  pause: (d: DownloadInfo) => void;
  resume: (d: DownloadInfo) => void;
  retry: (d: DownloadInfo) => void;
  remove: (d: DownloadInfo) => void;
  relink: (d: DownloadInfo, url: string) => void;
  openFile: (d: DownloadInfo) => void;
  revealFile: (d: DownloadInfo) => void;
  copy: (text: string) => void;
}

/** Every status renders the same four slots — thumbnail, title + status,
 *  metadata, progress — so the eye lands in the same place on every card and
 *  the action column never changes width. */
export function DownloadCard({
  d,
  t,
  lang,
  filePresent,
  actions,
}: {
  d: DownloadInfo;
  t: Dict;
  lang: Lang;
  /** undefined = not checked yet; false = the file is gone from disk. */
  filePresent?: boolean;
  actions: CardActions;
}) {
  const [quality, setQuality] = useState(d.quality);
  const [newUrl, setNewUrl] = useState("");
  const [menu, setMenu] = useState<HTMLElement | null>(null);

  const expired = isExpired(d);
  const hasFile = d.status === "done" && filePresent !== false;

  const showsProgress =
    d.status === "downloading" || d.status === "paused" || d.status === "interrupted";

  // Identity of the item, same slot whatever the status.
  const meta =
    d.kind === "file"
      ? [fmt.host(d.url), d.resumable === false && t.notResumable]
      : [
          d.uploader,
          fmt.duration(d.duration),
          d.quality && (d.kind === "audio" ? `${d.quality} kbps` : `${d.quality}p`),
        ];

  // Numbers, same slot whatever the status.
  const stats = showsProgress
    ? [
        fmt.percent(d.percent, lang),
        fmt.progressBytes(d.downloaded_bytes, d.total_bytes, lang),
        d.status === "downloading" && fmt.speed(d.speed_bps, lang),
        d.status === "downloading" && fmt.eta(d.eta_secs),
        d.status === "downloading" && d.connections > 1 && t.connections(d.connections),
      ]
    : d.status === "done"
      ? [
          fmt.bytes(d.total_bytes || d.downloaded_bytes, lang),
          fmt.since(d.finished_at, lang),
        ]
      : [];

  const primary =
    d.status === "downloading"
      ? { icon: <PauseOutlined />, label: t.pause, run: () => actions.pause(d) }
      : d.status === "paused" || d.status === "interrupted"
        ? { icon: <PlayArrowOutlined />, label: t.resume, run: () => actions.resume(d) }
        : d.status === "error" && !expired
          ? { icon: <RefreshOutlined />, label: t.retry, run: () => actions.retry(d) }
          : hasFile
            ? { icon: <OpenInNewOutlined />, label: t.openFile, run: () => actions.openFile(d) }
            : null;

  return (
    <Paper
      component="li"
      sx={{
        p: 2,
        listStyle: "none",
        bgcolor: tok.surfaceLow,
        border: `1px solid ${tok.outlineVariant}`,
        transition: (th) => th.transitions.create(["background-color", "border-color"]),
        "&:hover": { bgcolor: tok.surface },
      }}
    >
      <Stack spacing={1.25}>
        {/* Header: only the identity of the item sits beside the thumbnail.
            Everything below runs the card's full width, so the quality picker
            and the progress bar line up with the thumbnail's left edge instead
            of being indented under it. */}
        <Stack direction="row" spacing={2} sx={{ alignItems: "center" }}>
          <Thumb d={d} />
          <Stack spacing={0.25} sx={{ flex: 1, minWidth: 0 }}>
            <Tooltip title={d.title || d.url} enterDelay={700}>
              <Typography variant="subtitle2" noWrap sx={{ userSelect: "text" }}>
                {d.title || d.url}
              </Typography>
            </Tooltip>
            <MetaLine parts={meta} />
          </Stack>
          <Stack direction="row" spacing={0.5} sx={{ flexShrink: 0 }}>
            {primary ? (
              <Tooltip title={primary.label}>
                <IconButton
                  aria-label={primary.label}
                  onClick={primary.run}
                  color={d.status === "done" ? "primary" : "default"}
                >
                  {primary.icon}
                </IconButton>
              </Tooltip>
            ) : (
              // Reserved, so the row keeps one shape across every status.
              <Box sx={{ width: 42, height: 42 }} aria-hidden />
            )}
            <Tooltip title={t.more}>
              <IconButton aria-label={t.more} onClick={(e) => setMenu(e.currentTarget)}>
                <MoreVertOutlined />
              </IconButton>
            </Tooltip>
          </Stack>
        </Stack>

        {d.status === "fetching" && <LinearProgress aria-label={t.status.fetching} />}

        {d.status === "ready" && (
          <Stack direction="row" spacing={1}>
            <QualityPicker d={d} value={quality} onChange={setQuality} t={t} lang={lang} />
            <Button
              variant="contained"
              startIcon={<DownloadOutlined />}
              onClick={() => actions.start(d, quality)}
              sx={{ flexShrink: 0 }}
            >
              {t.download}
            </Button>
          </Stack>
        )}

        {showsProgress && (
          <Box>
            <LinearProgress
              variant="determinate"
              value={Math.min(d.percent, 100)}
              aria-label={t.status[d.status]}
            />
            {d.status === "downloading" && (
              <SegmentBar segments={d.segments} label={t.connections(d.connections)} />
            )}
          </Box>
        )}

        {(showsProgress || d.status === "done") && <MetaLine parts={stats} />}

        {d.retry > 0 && (
          <Typography variant="caption" sx={{ color: tok.caution }}>
            {t.retrying(d.retry, d.retry_max)}
          </Typography>
        )}

        {d.status === "error" && (
          // `severity` carries the meaning; the text stays selectable so a raw
          // yt-dlp message can be copied into a bug report.
          <Alert
            severity={expired ? "warning" : "error"}
            sx={{ "& .MuiAlert-message": { userSelect: "text" } }}
          >
            <Typography variant="body2">{errorText(d.error_msg, t)}</Typography>
          </Alert>
        )}

        {expired && (
          // Retrying a dead address is pointless, so ask for a live one. The
          // bytes on disk are kept; the If-Range validator decides the rest.
          <Box>
            <Typography variant="caption" color="text.secondary">
              {t.relinkHint}
            </Typography>
            <Stack direction="row" spacing={1} sx={{ mt: 1 }}>
              <TextField
                fullWidth
                placeholder={t.relinkPlaceholder}
                value={newUrl}
                onChange={(e) => setNewUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && newUrl.trim()) {
                    actions.relink(d, newUrl.trim());
                    setNewUrl("");
                  }
                }}
              />
              <Button
                variant="contained"
                disabled={!newUrl.trim()}
                onClick={() => {
                  actions.relink(d, newUrl.trim());
                  setNewUrl("");
                }}
                sx={{ flexShrink: 0 }}
              >
                {t.relinkBtn}
              </Button>
            </Stack>
          </Box>
        )}
      </Stack>

      <Menu anchorEl={menu} open={!!menu} onClose={() => setMenu(null)}>
        {hasFile && (
          <MenuItem
            onClick={() => {
              actions.openFile(d);
              setMenu(null);
            }}
          >
            <ListItemIcon>
              <OpenInNewOutlined fontSize="small" />
            </ListItemIcon>
            <ListItemText>{t.openFile}</ListItemText>
          </MenuItem>
        )}
        {hasFile && (
          <MenuItem
            onClick={() => {
              actions.revealFile(d);
              setMenu(null);
            }}
          >
            <ListItemIcon>
              <FolderOpenOutlined fontSize="small" />
            </ListItemIcon>
            <ListItemText>{t.revealFile}</ListItemText>
          </MenuItem>
        )}
        {d.status === "done" && filePresent === false && (
          <MenuItem disabled>
            <ListItemText>{t.fileGone}</ListItemText>
          </MenuItem>
        )}
        {!!d.error_detail && (
          <MenuItem
            onClick={() => {
              actions.copy(d.error_detail);
              setMenu(null);
            }}
          >
            <ListItemIcon>
              <BugReportOutlined fontSize="small" />
            </ListItemIcon>
            <ListItemText>{t.copyError}</ListItemText>
          </MenuItem>
        )}
        <MenuItem
          onClick={() => {
            actions.copy(d.url);
            setMenu(null);
          }}
        >
          <ListItemIcon>
            <LinkOutlined fontSize="small" />
          </ListItemIcon>
          <ListItemText>{t.copyLink}</ListItemText>
        </MenuItem>
        {!!d.file_path && (
          <MenuItem
            onClick={() => {
              actions.copy(d.file_path);
              setMenu(null);
            }}
          >
            <ListItemIcon>
              <ContentCopyOutlined fontSize="small" />
            </ListItemIcon>
            <ListItemText>{t.copyPath}</ListItemText>
          </MenuItem>
        )}
        <MenuItem
          onClick={() => {
            actions.remove(d);
            setMenu(null);
          }}
          sx={{ color: "error.main" }}
        >
          <ListItemIcon>
            <DeleteOutlineOutlined fontSize="small" color="error" />
          </ListItemIcon>
          <ListItemText>{t.delete}</ListItemText>
        </MenuItem>
      </Menu>
    </Paper>
  );
}
