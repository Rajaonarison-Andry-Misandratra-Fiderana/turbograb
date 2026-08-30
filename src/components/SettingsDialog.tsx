import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import Stack from "@mui/material/Stack";
import ToggleButton from "@mui/material/ToggleButton";
import ToggleButtonGroup from "@mui/material/ToggleButtonGroup";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import BrightnessAutoOutlined from "@mui/icons-material/BrightnessAutoOutlined";
import DarkModeOutlined from "@mui/icons-material/DarkModeOutlined";
import ArticleOutlined from "@mui/icons-material/ArticleOutlined";
import FolderOutlined from "@mui/icons-material/FolderOutlined";
import LightModeOutlined from "@mui/icons-material/LightModeOutlined";
import { useColorScheme } from "@mui/material/styles";
import { DICT, LANGS, type Dict, type Lang } from "../i18n";
import { shape, tok } from "../theme";

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <Stack spacing={1.5}>
      <Typography variant="overline" color="text.secondary">
        {label}
      </Typography>
      {children}
    </Stack>
  );
}

/** Settings used to be a two-item language menu. Everything the app remembers
 *  now lives here, and all of it is persisted by the backend. */
export function SettingsDialog({
  open,
  onClose,
  t,
  lang,
  setLang,
  outDir,
  onPickDir,
  onOpenLogs,
}: {
  open: boolean;
  onClose: () => void;
  t: Dict;
  lang: Lang;
  setLang: (l: Lang) => void;
  outDir: string;
  onPickDir: () => void;
  onOpenLogs: () => void;
}) {
  const { mode, setMode } = useColorScheme();

  return (
    <Dialog open={open} onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{t.settings}</DialogTitle>
      <DialogContent>
        <Stack spacing={3.5} sx={{ pt: 1 }}>
          <Section label={t.theme}>
            <ToggleButtonGroup
              exclusive
              fullWidth
              // `mode` is undefined on the first render; falling back to
              // "system" keeps the group from flashing an unselected state.
              value={mode ?? "system"}
              onChange={(_, v) => v && setMode(v)}
            >
              <ToggleButton value="system">
                <BrightnessAutoOutlined fontSize="small" />
                {t.themeSystem}
              </ToggleButton>
              <ToggleButton value="light">
                <LightModeOutlined fontSize="small" />
                {t.themeLight}
              </ToggleButton>
              <ToggleButton value="dark">
                <DarkModeOutlined fontSize="small" />
                {t.themeDark}
              </ToggleButton>
            </ToggleButtonGroup>
          </Section>

          <Section label={t.language}>
            <ToggleButtonGroup
              exclusive
              fullWidth
              value={lang}
              onChange={(_, v: Lang | null) => v && setLang(v)}
            >
              {LANGS.map((l) => (
                <ToggleButton key={l.id} value={l.id}>
                  {l.label}
                  <Typography variant="caption" sx={{ ml: 0.75, opacity: 0.7 }}>
                    {l.id.toUpperCase()}
                  </Typography>
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
          </Section>

          <Section label={DICT[lang].destination}>
            <Tooltip title={outDir}>
              <Button
                onClick={onPickDir}
                startIcon={<FolderOutlined />}
                color="inherit"
                sx={{
                  justifyContent: "flex-start",
                  border: `1px solid ${tok.outlineVariant}`,
                  borderRadius: `${shape.sm}px`,
                  textTransform: "none",
                }}
              >
                <Box
                  component="span"
                  sx={{
                    flex: 1,
                    minWidth: 0,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    textAlign: "left",
                    direction: "rtl",
                    unicodeBidi: "plaintext",
                  }}
                >
                  {outDir || t.chooseDir}
                </Box>
              </Button>
            </Tooltip>
          </Section>

          <Section label={t.logs}>
            <Button
              onClick={onOpenLogs}
              startIcon={<ArticleOutlined />}
              color="inherit"
              sx={{
                justifyContent: "flex-start",
                border: `1px solid ${tok.outlineVariant}`,
                borderRadius: `${shape.sm}px`,
                textTransform: "none",
              }}
            >
              {t.viewLogs}
            </Button>
          </Section>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t.close}</Button>
      </DialogActions>
    </Dialog>
  );
}
