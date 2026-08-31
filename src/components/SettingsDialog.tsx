import { useState } from "react";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import Divider from "@mui/material/Divider";
import FormControlLabel from "@mui/material/FormControlLabel";
import IconButton from "@mui/material/IconButton";
import Slider from "@mui/material/Slider";
import Stack from "@mui/material/Stack";
import Switch from "@mui/material/Switch";
import TextField from "@mui/material/TextField";
import ToggleButton from "@mui/material/ToggleButton";
import ToggleButtonGroup from "@mui/material/ToggleButtonGroup";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import ArticleOutlined from "@mui/icons-material/ArticleOutlined";
import BrightnessAutoOutlined from "@mui/icons-material/BrightnessAutoOutlined";
import ContentCopyOutlined from "@mui/icons-material/ContentCopyOutlined";
import DarkModeOutlined from "@mui/icons-material/DarkModeOutlined";
import FolderOutlined from "@mui/icons-material/FolderOutlined";
import LightModeOutlined from "@mui/icons-material/LightModeOutlined";
import { useColorScheme } from "@mui/material/styles";
import { DICT, LANGS, type Dict, type Lang } from "../i18n";
import { shape, tok } from "../theme";
import type { Settings } from "../types";

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

/** A switch with a sentence under it: every one of these changes what the app
 *  does when nobody is looking, so none of them ships as a bare label. */
function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <Box>
      <FormControlLabel
        sx={{ ml: 0, width: "100%", justifyContent: "space-between" }}
        labelPlacement="start"
        label={<Typography variant="body2">{label}</Typography>}
        control={<Switch checked={checked} onChange={(e) => onChange(e.target.checked)} />}
      />
      <Typography variant="caption" color="text.secondary" sx={{ display: "block" }}>
        {hint}
      </Typography>
    </Box>
  );
}

/** Everything the app remembers, in one place, all of it persisted by the
 *  backend — including the parts that only matter while the window is closed
 *  (launch on boot, start in the tray, the browser listener). */
export function SettingsDialog({
  open,
  onClose,
  t,
  lang,
  settings,
  patch,
  onPickDir,
  onOpenLogs,
  onCopy,
  onRevokeToken,
  onMakeToken,
}: {
  open: boolean;
  onClose: () => void;
  t: Dict;
  lang: Lang;
  settings: Settings;
  patch: (next: Partial<Settings>) => void;
  onPickDir: () => void;
  onOpenLogs: () => void;
  onCopy: (text: string) => void;
  onRevokeToken: () => void;
  onMakeToken: () => void;
}) {
  const { mode, setMode } = useColorScheme();
  const [port, setPort] = useState(String(settings.server_port));
  const [showToken, setShowToken] = useState(false);

  const commitPort = () => {
    const n = Number(port);
    if (Number.isInteger(n) && n >= 1024 && n <= 65535) patch({ server_port: n });
    else setPort(String(settings.server_port));
  };

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
              onChange={(_, v: Lang | null) => v && patch({ lang: v })}
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
            <Tooltip title={settings.out_dir}>
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
                  {settings.out_dir || t.chooseDir}
                </Box>
              </Button>
            </Tooltip>
          </Section>

          <Section label={t.transfers}>
            <Box>
              <Typography variant="body2">{t.connectionsSetting}</Typography>
              <Typography variant="caption" color="text.secondary">
                {t.connectionsHint}
              </Typography>
              <Slider
                value={settings.connections}
                min={1}
                max={16}
                step={1}
                marks={[
                  { value: 1, label: "1" },
                  { value: 8, label: "8" },
                  { value: 16, label: "16" },
                ]}
                valueLabelDisplay="auto"
                onChange={(_, v) => patch({ connections: v as number })}
              />
            </Box>
            <Box>
              <Typography variant="body2">{t.maxActiveSetting}</Typography>
              <Typography variant="caption" color="text.secondary">
                {t.maxActiveHint}
              </Typography>
              <Slider
                value={settings.max_active}
                min={1}
                max={10}
                step={1}
                marks={[
                  { value: 1, label: "1" },
                  { value: 5, label: "5" },
                  { value: 10, label: "10" },
                ]}
                valueLabelDisplay="auto"
                onChange={(_, v) => patch({ max_active: v as number })}
              />
            </Box>
          </Section>

          <Section label={t.startup}>
            <Toggle
              label={t.autostart}
              hint={t.autostartHint}
              checked={settings.autostart}
              onChange={(v) => patch({ autostart: v })}
            />
            <Toggle
              label={t.startHidden}
              hint={t.startHiddenHint}
              checked={settings.start_hidden}
              onChange={(v) => patch({ start_hidden: v })}
            />
          </Section>

          <Section label={t.browserSection}>
            <Toggle
              label={t.serverEnabled}
              hint={t.serverEnabledHint}
              checked={settings.server_enabled}
              onChange={(v) => patch({ server_enabled: v })}
            />
            <TextField
              label={t.serverPort}
              value={port}
              disabled={!settings.server_enabled}
              onChange={(e) => setPort(e.target.value.replace(/\D/g, "").slice(0, 5))}
              onBlur={commitPort}
              onKeyDown={(e) => e.key === "Enter" && commitPort()}
              helperText={t.serverPortHint}
              size="small"
            />
            <Divider />
            <Box>
              <Typography variant="body2">
                {settings.token ? t.paired : t.notPaired}
              </Typography>
              <Typography variant="caption" color="text.secondary" sx={{ display: "block" }}>
                {settings.token ? t.pairedHint : t.notPairedHint}
              </Typography>
            </Box>
            {settings.token ? (
              <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
                <TextField
                  size="small"
                  fullWidth
                  value={showToken ? settings.token : "•".repeat(32)}
                  slotProps={{ input: { readOnly: true, sx: { fontFamily: "monospace" } } }}
                  onClick={() => setShowToken(true)}
                />
                <Tooltip title={t.copyToken}>
                  <IconButton aria-label={t.copyToken} onClick={() => onCopy(settings.token)}>
                    <ContentCopyOutlined fontSize="small" />
                  </IconButton>
                </Tooltip>
                <Button color="error" onClick={onRevokeToken} sx={{ flexShrink: 0 }}>
                  {t.revoke}
                </Button>
              </Stack>
            ) : (
              <Button
                onClick={onMakeToken}
                color="inherit"
                sx={{
                  justifyContent: "flex-start",
                  border: `1px solid ${tok.outlineVariant}`,
                  borderRadius: `${shape.sm}px`,
                  textTransform: "none",
                }}
              >
                {t.makeToken}
              </Button>
            )}
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
