import Box from "@mui/material/Box";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { ReactNode } from "react";
import { shape, tok } from "../theme";

/** Fills whatever vertical room is left and centres in it.
 *
 *  `flex: 1` + `justifyContent: center` rather than a fixed `py`: with padding
 *  the block sat high in the window, with more space above it than below. */
export function EmptyState({
  icon,
  title,
  hint,
}: {
  icon: ReactNode;
  title: string;
  hint: string;
}) {
  return (
    <Stack
      spacing={1.5}
      sx={{
        flex: 1,
        minHeight: 200,
        alignItems: "center",
        justifyContent: "center",
        px: 3,
        textAlign: "center",
        color: "text.secondary",
        border: `1px dashed ${tok.outlineVariant}`,
        borderRadius: `${shape.lg}px`,
      }}
    >
      <Box
        sx={{
          width: 64,
          height: 64,
          mb: 1,
          display: "grid",
          placeItems: "center",
          borderRadius: "50%",
          bgcolor: tok.surfaceHighest,
          "& svg": { fontSize: 30 },
        }}
      >
        {icon}
      </Box>
      <Typography variant="subtitle1" color="text.primary">
        {title}
      </Typography>
      <Typography variant="body2">{hint}</Typography>
    </Stack>
  );
}
