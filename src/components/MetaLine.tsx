import Typography from "@mui/material/Typography";

/** One line of dot-separated facts, in tabular figures so digits don't jitter
 *  as they change. Falsy entries drop out, so every status can feed the same
 *  component and the card keeps one consistent metadata slot. */
export function MetaLine({
  parts,
  color = "text.secondary",
}: {
  parts: (string | false | null | undefined)[];
  color?: string;
}) {
  const shown = parts.filter(Boolean) as string[];
  if (!shown.length) return null;
  return (
    <Typography
      variant="caption"
      color={color}
      noWrap
      sx={{ fontVariantNumeric: "tabular-nums", userSelect: "text" }}
    >
      {shown.join(" · ")}
    </Typography>
  );
}
