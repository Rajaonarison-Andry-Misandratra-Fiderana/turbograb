import { useState } from "react";
import Button from "@mui/material/Button";
import ListItemIcon from "@mui/material/ListItemIcon";
import ListItemText from "@mui/material/ListItemText";
import Menu from "@mui/material/Menu";
import MenuItem from "@mui/material/MenuItem";
import Typography from "@mui/material/Typography";
import CheckOutlined from "@mui/icons-material/CheckOutlined";
import ExpandMoreOutlined from "@mui/icons-material/ExpandMoreOutlined";
import GraphicEqOutlined from "@mui/icons-material/GraphicEqOutlined";
import HighQualityOutlined from "@mui/icons-material/HighQualityOutlined";
import { bytes } from "../format";
import { shape, tok } from "../theme";
import type { Dict, Lang } from "../i18n";
import type { DownloadInfo } from "../types";

/** Quality chooser: label on the left, estimated size right-aligned.
 *
 *  A `Menu` rather than the old absolutely-positioned popover, which was laid
 *  out inside the scrolling list and got clipped whenever the card sat near the
 *  bottom of the window. A portalled menu flips instead of clipping, and brings
 *  arrow-key navigation and a focus trap with it. */
export function QualityPicker({
  d,
  value,
  onChange,
  t,
  lang,
}: {
  d: DownloadInfo;
  value: string;
  onChange: (v: string) => void;
  t: Dict;
  lang: Lang;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const labelOf = (v: string, label: string) => (v === "" ? t.best : label);
  const cur = d.qualities.find((q) => q.value === value) ?? d.qualities[0];
  const KindIcon = d.kind === "audio" ? GraphicEqOutlined : HighQualityOutlined;

  return (
    <>
      <Button
        variant="outlined"
        color="inherit"
        onClick={(e) => setAnchor(e.currentTarget)}
        aria-haspopup="listbox"
        aria-expanded={!!anchor}
        startIcon={<KindIcon />}
        endIcon={<ExpandMoreOutlined />}
        sx={{
          flex: 1,
          minWidth: 0,
          justifyContent: "flex-start",
          // A dropdown reads as a field, not as a button: it takes the input
          // corner (8) rather than the pill every real button uses.
          borderRadius: `${shape.sm}px`,
          borderColor: tok.outlineVariant,
          textAlign: "left",
        }}
      >
        <Typography variant="body2" noWrap sx={{ flex: 1, minWidth: 0 }}>
          {cur ? labelOf(cur.value, cur.label) : t.quality}
        </Typography>
        {/* Tabular figures so the sizes line up between the button and the menu. */}
        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ ml: 1, fontVariantNumeric: "tabular-nums" }}
        >
          {cur ? bytes(cur.size_bytes, lang) : ""}
        </Typography>
      </Button>

      <Menu
        open={!!anchor}
        anchorEl={anchor}
        onClose={() => setAnchor(null)}
        slotProps={{ list: { role: "listbox", dense: true } }}
      >
        {d.qualities.map((q) => (
          <MenuItem
            key={q.value || "best"}
            role="option"
            aria-selected={q.value === value}
            selected={q.value === value}
            onClick={() => {
              onChange(q.value);
              setAnchor(null);
            }}
          >
            <ListItemIcon sx={{ minWidth: 32 }}>
              {q.value === value && <CheckOutlined fontSize="small" color="primary" />}
            </ListItemIcon>
            <ListItemText slotProps={{ primary: { variant: "body2" } }}>
              {labelOf(q.value, q.label)}
            </ListItemText>
            <Typography
              variant="caption"
              color="text.secondary"
              sx={{ ml: 3, fontVariantNumeric: "tabular-nums" }}
            >
              {bytes(q.size_bytes, lang) || t.unknownSize}
            </Typography>
          </MenuItem>
        ))}
      </Menu>
    </>
  );
}
