import Box from "@mui/material/Box";
import Tooltip from "@mui/material/Tooltip";
import { tok } from "../theme";

/** The shape of a segmented transfer: one tick per bucket, filled by how much
 *  of that slice is on disk.
 *
 *  The backend already tracked this per 4 MiB chunk to make resume work; it
 *  just never left the process. Buckets are capped at 48 so a 100 GB file
 *  costs the same to send and draw as a 100 MB one. */
export function SegmentBar({ segments, label }: { segments: number[]; label: string }) {
  if (segments.length < 2) return null;
  return (
    <Tooltip title={label}>
      <Box
        aria-hidden
        sx={{ display: "flex", gap: "2px", height: 6, mt: 0.25 }}
      >
        {segments.map((pct, i) => (
          <Box
            key={i}
            sx={(t) => ({
              flex: 1,
              borderRadius: "2px",
              bgcolor: tok.surfaceHighest,
              overflow: "hidden",
              position: "relative",
              "&::after": {
                content: '""',
                position: "absolute",
                inset: 0,
                width: `${pct}%`,
                bgcolor: tok.primary,
                transition: t.transitions.create("width", { duration: 250 }),
              },
            })}
          />
        ))}
      </Box>
    </Tooltip>
  );
}
