import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import ExtensionOutlined from "@mui/icons-material/ExtensionOutlined";
import type { Dict } from "../i18n";

export interface PairRequest {
  id: string;
  client: string;
  origin: string;
  ts: number;
}

/** The whole security model of the local API, rendered.
 *
 *  Anything on this machine can reach the listening port; only a click here
 *  hands out the token that lets it queue downloads. So the prompt names who is
 *  asking, says plainly what saying yes allows, and defaults to nothing: a
 *  dismissed dialog denies. */
export function PairDialog({
  req,
  t,
  onRespond,
}: {
  req: PairRequest | null;
  t: Dict;
  onRespond: (id: string, allow: boolean) => void;
}) {
  if (!req) return null;
  const who = req.client?.trim() || t.unknownClient;

  return (
    <Dialog open onClose={() => onRespond(req.id, false)} maxWidth="xs" fullWidth>
      <DialogTitle sx={{ display: "flex", alignItems: "center", gap: 1 }}>
        <ExtensionOutlined color="primary" />
        {t.pairTitle}
      </DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          <Typography variant="body2">{t.pairBody(who)}</Typography>
          {!!req.origin && (
            <Typography
              variant="caption"
              color="text.secondary"
              sx={{ fontFamily: "monospace", wordBreak: "break-all" }}
            >
              {req.origin}
            </Typography>
          )}
          <Alert severity="info">{t.pairWarning}</Alert>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={() => onRespond(req.id, false)}>{t.pairDeny}</Button>
        <Button variant="contained" onClick={() => onRespond(req.id, true)}>
          {t.pairAllow}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
