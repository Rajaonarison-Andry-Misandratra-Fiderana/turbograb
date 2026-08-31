import { useMemo, useState } from "react";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import FormHelperText from "@mui/material/FormHelperText";
import IconButton from "@mui/material/IconButton";
import InputAdornment from "@mui/material/InputAdornment";
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
import LinkOutlined from "@mui/icons-material/LinkOutlined";
import { shape, tok } from "../theme";
import type { Dict } from "../i18n";
import { parseLinks } from "../types";

/** The outlined input's own horizontal padding. Every control in the composer
 *  reuses it, so leading icons and text sit in one column down the left edge
 *  and the trailing icons in one down the right. */
const EDGE = 14;
/** Height of a medium outlined TextField — the download button matches it. */
const FIELD_H = 56;

/** One field, any number of links.
 *
 *  A block of links copied out of a page is one paste and one click: the field
 *  splits on whitespace, so ten URLs become ten cards. The count under the
 *  field is the confirmation — it says how many links were actually recognised
 *  before anything is queued. */
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
  onSubmit: (urls: string[]) => void;
}) {
  const [text, setText] = useState("");
  const links = useMemo(() => parseLinks(text), [text]);
  const multiline = text.includes("\n");
  const hint = links.length > 1 ? t.linkCount(links.length) : text.trim() && !links.length ? t.noLink : "";

  const submit = () => {
    if (!links.length) return;
    onSubmit(links);
    setText("");
  };

  // Clipboard reads can be refused (no permission, no secure context); a paste
  // button that silently does nothing beats one that throws.
  const paste = async () => {
    try {
      const clip = await navigator.clipboard.readText();
      if (clip.trim()) setText((cur) => (cur.trim() ? `${cur.trim()}\n${clip.trim()}` : clip.trim()));
    } catch {
      /* ignore — the keyboard still pastes */
    }
  };

  return (
    <Paper
      sx={{
        p: 2,
        bgcolor: tok.surfaceLow,
        border: `1px solid ${tok.outlineVariant}`,
      }}
    >
      <Stack spacing={2}>
        <Stack direction="row" spacing={1.5} sx={{ alignItems: "flex-start" }}>
          <TextField
            sx={{ flex: 1, minWidth: 0 }}
            multiline={multiline}
            maxRows={6}
            label={t.urlLabel}
            placeholder={t.urlPlaceholder}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              // Enter sends; Shift+Enter is how you get a second link in.
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit();
              }
            }}
            slotProps={{
              input: {
                startAdornment: (
                  <InputAdornment position="start">
                    <LinkOutlined fontSize="small" sx={{ color: tok.primary }} />
                  </InputAdornment>
                ),
                endAdornment: (
                  <InputAdornment position="end">
                    {text ? (
                      <Tooltip title={t.clearField}>
                        <IconButton
                          size="small"
                          aria-label={t.clearField}
                          onClick={() => setText("")}
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
            disabled={!links.length}
            onClick={submit}
            sx={{ flexShrink: 0, height: FIELD_H }}
          >
            {t.download}
          </Button>
        </Stack>

        {hint && (
          // Under the row, not inside it: a helper that only exists when it has
          // something to say keeps every gap in the composer the same 16px.
          <FormHelperText sx={{ mt: "-8px !important", mx: `${EDGE}px` }}>{hint}</FormHelperText>
        )}

        <Box>
          {/* One control: it shows where files land, and clicking it changes
              that. The trailing icon is the affordance — it replaces the word
              "Change", which said the same thing in more room. */}
          <Tooltip title={t.change}>
            <Button
              onClick={onPickDir}
              startIcon={<FolderOutlined />}
              aria-label={`${t.destination} — ${t.change}`}
              color={dirWarn ? "error" : "inherit"}
              sx={{
                width: "100%",
                justifyContent: "flex-start",
                px: `${EDGE}px`,
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
                sx={{ ml: 1.5, mr: "7px", flexShrink: 0, color: tok.primary }}
              />
            </Button>
          </Tooltip>
          {dirWarn && (
            <FormHelperText error sx={{ mx: `${EDGE}px` }}>
              {t.dirWarn}
            </FormHelperText>
          )}
        </Box>
      </Stack>
    </Paper>
  );
}
