import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import { shape, tok } from "../theme";
import type { Dict, Lang } from "../i18n";

export interface LogEntry {
  ts: number;
  level: "error" | "warn" | "info";
  source: string;
  msg: string;
  id: string;
}

const LEVEL_COLOR: Record<LogEntry["level"], string> = {
  error: tok.error,
  warn: tok.caution,
  info: tok.textSecondary,
};

/** The raw diagnostics, in one place.
 *
 *  Cards deliberately show a plain sentence and nothing else. That only works
 *  if the detail is still reachable — this is where it lives, newest first,
 *  copyable in one go for a bug report. */
export function LogDialog({
  open,
  onClose,
  t,
  lang,
  onCopy,
}: {
  open: boolean;
  onClose: () => void;
  t: Dict;
  lang: Lang;
  onCopy: (text: string) => void;
}) {
  const [entries, setEntries] = useState<LogEntry[]>([]);

  const load = useCallback(() => {
    invoke<LogEntry[]>("get_log")
      .then((e) => setEntries([...e].reverse()))
      .catch(() => setEntries([]));
  }, []);

  // The journal is a snapshot, not a stream: read it when the dialog opens.
  useEffect(() => {
    if (open) load();
  }, [open, load]);

  const stamp = (ts: number) =>
    new Date(ts * 1000).toLocaleTimeString(lang === "fr" ? "fr-FR" : "en-US");

  const asText = () =>
    entries
      .map((e) => `${stamp(e.ts)} [${e.level}] ${e.source}: ${e.msg}`)
      .join("\n");

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>{t.logs}</DialogTitle>
      <DialogContent>
        <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
          {t.logHint}
        </Typography>
        <Box
          sx={{
            maxHeight: 360,
            overflow: "auto",
            p: 1.5,
            borderRadius: `${shape.sm}px`,
            bgcolor: tok.surfaceLowest,
            border: `1px solid ${tok.outlineVariant}`,
          }}
        >
          {entries.length === 0 ? (
            <Typography variant="body2" color="text.secondary">
              {t.logEmpty}
            </Typography>
          ) : (
            <Stack spacing={1}>
              {entries.map((e, i) => (
                <Box key={i}>
                  <Typography
                    variant="caption"
                    sx={{ color: LEVEL_COLOR[e.level], fontWeight: 600 }}
                  >
                    {stamp(e.ts)} · {e.source}
                  </Typography>
                  <Typography
                    variant="body2"
                    sx={{
                      // Monospace and wrapped: these lines are URLs, paths and
                      // stack-ish text that must stay copyable verbatim.
                      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                      fontSize: "0.78rem",
                      whiteSpace: "pre-wrap",
                      wordBreak: "break-word",
                      userSelect: "text",
                    }}
                  >
                    {e.msg}
                  </Typography>
                </Box>
              ))}
            </Stack>
          )}
        </Box>
      </DialogContent>
      <DialogActions>
        <Button
          color="error"
          disabled={!entries.length}
          onClick={() => invoke("clear_log").then(load).catch(() => {})}
        >
          {t.clearLog}
        </Button>
        <Box sx={{ flex: 1 }} />
        <Button disabled={!entries.length} onClick={() => onCopy(asText())}>
          {t.copyAll}
        </Button>
        <Button variant="contained" onClick={onClose}>
          {t.close}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
