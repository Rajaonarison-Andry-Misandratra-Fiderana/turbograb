import Button from "@mui/material/Button";
import Collapse from "@mui/material/Collapse";
import Paper from "@mui/material/Paper";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import RestartAltOutlined from "@mui/icons-material/RestartAltOutlined";
import { tok } from "../theme";
import type { Dict } from "../i18n";

/** Offered only when something is actually interrupted — a download the app
 *  was moving when it was last closed, which nothing but this banner would
 *  otherwise invite you to pick back up. */
export function ResumeBanner({
  count,
  t,
  onDismiss,
  onResumeAll,
}: {
  count: number;
  t: Dict;
  onDismiss: () => void;
  onResumeAll: () => void;
}) {
  return (
    <Collapse in={count > 0} unmountOnExit>
      <Paper
        sx={{
          p: 2,
          mb: 2,
          bgcolor: tok.cautionContainer,
          color: tok.onCautionContainer,
        }}
      >
        <Stack
          direction="row"
          spacing={2}
          useFlexGap
          sx={{ alignItems: "center", flexWrap: "wrap" }}
        >
          <RestartAltOutlined />
          <Stack sx={{ flex: "1 1 200px", minWidth: 0 }}>
            <Typography variant="subtitle2">{t.interruptedTitle(count)}</Typography>
            <Typography variant="caption">{t.resumePrompt}</Typography>
          </Stack>
          <Button color="inherit" onClick={onDismiss}>
            {t.later}
          </Button>
          <Button variant="contained" onClick={onResumeAll}>
            {t.resumeAll}
          </Button>
        </Stack>
      </Paper>
    </Collapse>
  );
}
