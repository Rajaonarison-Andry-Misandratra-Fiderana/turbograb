import { useState } from "react";
import Box from "@mui/material/Box";
import Skeleton from "@mui/material/Skeleton";
import AudiotrackOutlined from "@mui/icons-material/AudiotrackOutlined";
import InsertDriveFileOutlined from "@mui/icons-material/InsertDriveFileOutlined";
import MovieOutlined from "@mui/icons-material/MovieOutlined";
import type { DownloadInfo } from "../types";
import { shape, tok } from "../theme";

/** Fixed 16:9 box, always the same size.
 *
 *  The old thumb was 48x48 with no image and 80x48 with one, so every card
 *  jumped sideways the moment a thumbnail arrived. Here the box is reserved up
 *  front and a skeleton fills it until the image decodes. */
export function Thumb({ d }: { d: DownloadInfo }) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const showImage = !!d.thumbnail && !failed;
  const Fallback =
    d.kind === "audio"
      ? AudiotrackOutlined
      : d.kind === "file"
        ? InsertDriveFileOutlined
        : MovieOutlined;

  return (
    <Box
      sx={{
        position: "relative",
        flexShrink: 0,
        // 16:9, big enough that a video is actually recognisable from it.
        width: 128,
        height: 72,
        borderRadius: `${shape.sm}px`,
        overflow: "hidden",
        bgcolor: tok.surfaceHighest,
        border: `1px solid ${tok.outlineVariant}`,
        display: "grid",
        placeItems: "center",
        color: "text.secondary",
      }}
    >
      {showImage ? (
        <>
          {!loaded && (
            <Skeleton
              variant="rectangular"
              sx={{ position: "absolute", inset: 0 }}
              animation="wave"
            />
          )}
          <Box
            component="img"
            src={d.thumbnail}
            alt=""
            loading="lazy"
            onLoad={() => setLoaded(true)}
            onError={() => setFailed(true)}
            sx={{
              width: "100%",
              height: "100%",
              objectFit: "cover",
              display: "block",
              opacity: loaded ? 1 : 0,
              transition: "opacity 200ms",
            }}
          />
        </>
      ) : (
        <Fallback />
      )}
    </Box>
  );
}
