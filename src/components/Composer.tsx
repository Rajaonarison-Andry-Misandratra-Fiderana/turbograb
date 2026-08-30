import { useEffect, useMemo, useState } from "react";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Divider from "@mui/material/Divider";
import FormHelperText from "@mui/material/FormHelperText";
import IconButton from "@mui/material/IconButton";
import InputAdornment from "@mui/material/InputAdornment";
import ListItemIcon from "@mui/material/ListItemIcon";
import ListItemText from "@mui/material/ListItemText";
import Menu from "@mui/material/Menu";
import MenuItem from "@mui/material/MenuItem";
import Paper from "@mui/material/Paper";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import ClearOutlined from "@mui/icons-material/ClearOutlined";
import ContentPasteOutlined from "@mui/icons-material/ContentPasteOutlined";
import DownloadOutlined from "@mui/icons-material/DownloadOutlined";
import FolderOpenOutlined from "@mui/icons-material/FolderOpenOutlined";
import FolderOutlined from "@mui/icons-material/FolderOutlined";
import GraphicEqOutlined from "@mui/icons-material/GraphicEqOutlined";
import InsertDriveFileOutlined from "@mui/icons-material/InsertDriveFileOutlined";
import MovieOutlined from "@mui/icons-material/MovieOutlined";
import { shape, tok } from "../theme";
import type { Dict } from "../i18n";
import { defaultSource, detect, type Source } from "../source";

const ICON: Record<Source, typeof MovieOutlined> = {
  video: MovieOutlined,
  audio: GraphicEqOutlined,
  file: InsertDriveFileOutlined,
};

export function Composer({
  t,
  outDir,
  dirWarn,
  onPickDir,
  onSubmit,
}: {
  t: Dict;
  outDir: string;
  dirWarn: boolean;
  onPickDir: () => void;
  onSubmit: (url: string, source: Source) => void;
}) {
  const [url, setUrl] = useState("");
  const [override, setOverride] = useState<Source | null>(null);
  const [menu, setMenu] = useState<HTMLElement | null>(null);

  const kind = useMemo(() => detect(url), [url]);
  // The link decides; the picker only exists to correct a wrong guess, and the
  // correction is dropped as soon as the link it applied to changes.
  const source: Source = override ?? defaultSource(url);
  useEffect(() => setOverride(null), [kind]);

  const submit = () => {
    if (!url.trim()) return;
    onSubmit(url.trim(), source);
    setUrl("");
    setOverride(null);
  };

  // Clipboard reads can be refused (no permission, no secure context); a paste
  // button that silently does nothing beats one that throws.
  const paste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text.trim()) setUrl(text.trim());
    } catch {
      /* ignore — the keyboard still pastes */
    }
  };

  const SourceIcon = ICON[source];
  const OPTIONS: { id: Source; label: string; Icon: typeof MovieOutlined }[] = [
    { id: "video", label: t.video, Icon: MovieOutlined },
    { id: "audio", label: t.audio, Icon: GraphicEqOutlined },
    { id: "file", label: t.directFile, Icon: InsertDriveFileOutlined },
  ];
  const auto = defaultSource(url);

  return (
    <Paper
      sx={{
        p: 2,
        bgcolor: tok.surfaceLow,
        border: `1px solid ${tok.outlineVariant}`,
      }}
    >
      <Stack spacing={2}>
        <Stack direction="row" spacing={1.5} sx={{ alignItems: "center" }}>
          <TextField
            fullWidth
            label={t.urlLabel}
            placeholder={t.urlPlaceholder}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            slotProps={{
              input: {
                // The kind switch lives inside the field, in a slot that is
                // always occupied: nothing appears or disappears, so the layout
                // never gains an empty row or shifts.
                startAdornment: (
                  <InputAdornment position="start">
                    <Tooltip title={t.sourceKind}>
                      <IconButton
                        size="small"
                        aria-label={t.sourceKind}
                        aria-haspopup="menu"
                        onClick={(e) => setMenu(e.currentTarget)}
                        color={source === "file" ? "default" : "primary"}
                      >
                        <SourceIcon fontSize="small" />
                      </IconButton>
                    </Tooltip>
                    <Divider orientation="vertical" flexItem sx={{ ml: 0.5, my: 0.5 }} />
                  </InputAdornment>
                ),
                endAdornment: (
                  <InputAdornment position="end">
                    {url ? (
                      <Tooltip title={t.clearField}>
                        <IconButton
                          size="small"
                          aria-label={t.clearField}
                          onClick={() => setUrl("")}
                        >
                          <ClearOutlined fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    ) : (
                      <Tooltip title={t.paste}>
                        <IconButton size="small" aria-label={t.paste} onClick={paste}>
                          <ContentPasteOutlined fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    )}
                  </InputAdornment>
                ),
              },
            }}
          />
          <Button
            variant="contained"
            startIcon={<DownloadOutlined />}
            disabled={!url.trim()}
            onClick={submit}
            sx={{ flexShrink: 0 }}
          >
            {t.download}
          </Button>
        </Stack>

        <Menu anchorEl={menu} open={!!menu} onClose={() => setMenu(null)}>
          {OPTIONS.map((o) => (
            <MenuItem
              key={o.id}
              selected={o.id === source}
              onClick={() => {
                setOverride(o.id);
                setMenu(null);
              }}
            >
              <ListItemIcon>
                <o.Icon fontSize="small" />
              </ListItemIcon>
              <ListItemText>{o.label}</ListItemText>
              {o.id === auto && (
                <Typography variant="caption" color="text.secondary" sx={{ ml: 2 }}>
                  {t.detected}
                </Typography>
              )}
            </MenuItem>
          ))}
        </Menu>

        <Box>
          {/* One control: it shows where files land, and clicking it changes
              that. The trailing icon is the affordance — it replaces the word
              "Changer", which said the same thing in more room. */}
          <Tooltip title={t.change}>
            <Button
              onClick={onPickDir}
              startIcon={<FolderOutlined />}
              aria-label={`${t.destination} — ${t.change}`}
              color={dirWarn ? "error" : "inherit"}
              sx={{
                width: "100%",
                justifyContent: "flex-start",
                borderRadius: `${shape.sm}px`,
                border: `1px ${dirWarn ? "solid" : "dashed"} ${
                  dirWarn ? tok.error : tok.outlineVariant
                }`,
                color: dirWarn ? "error.main" : "text.secondary",
                textTransform: "none",
              }}
            >
              <Typography
                variant="body2"
                noWrap
                // Clip the head of the path, keep the tail: `direction: rtl`
                // does that, and `plaintext` stops it from also flipping the
                // leading "/" to the end of the string.
                sx={{
                  flex: 1,
                  minWidth: 0,
                  textAlign: "left",
                  direction: "rtl",
                  unicodeBidi: "plaintext",
                }}
                title={outDir}
              >
                {outDir || t.chooseDir}
              </Typography>
              <FolderOpenOutlined
                fontSize="small"
                sx={{ ml: 1.5, flexShrink: 0, color: tok.primary }}
              />
            </Button>
          </Tooltip>
          {dirWarn && <FormHelperText error>{t.dirWarn}</FormHelperText>}
        </Box>
      </Stack>
    </Paper>
  );
}
