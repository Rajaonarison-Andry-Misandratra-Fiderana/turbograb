import Box from "@mui/material/Box";
import Typography from "@mui/material/Typography";
import ArchiveOutlined from "@mui/icons-material/ArchiveOutlined";
import AudiotrackOutlined from "@mui/icons-material/AudiotrackOutlined";
import DescriptionOutlined from "@mui/icons-material/DescriptionOutlined";
import ImageOutlined from "@mui/icons-material/ImageOutlined";
import InsertDriveFileOutlined from "@mui/icons-material/InsertDriveFileOutlined";
import MovieOutlined from "@mui/icons-material/MovieOutlined";
import PictureAsPdfOutlined from "@mui/icons-material/PictureAsPdfOutlined";
import TerminalOutlined from "@mui/icons-material/TerminalOutlined";
import type { DownloadInfo } from "../types";
import { shape, tok } from "../theme";

/** File extension, lowercase and without the dot. */
export function extOf(d: DownloadInfo): string {
  const name = d.title || d.url.split(/[?#]/)[0];
  const dot = name.lastIndexOf(".");
  // A dot in the last 8 characters is an extension; a dot in a hostname is not.
  return dot > 0 && name.length - dot <= 9 ? name.slice(dot + 1).toLowerCase() : "";
}

const FAMILIES: [RegExp, typeof MovieOutlined][] = [
  [/^(mp4|mkv|avi|mov|webm|m4v|flv|wmv|mpg|mpeg|ts)$/, MovieOutlined],
  [/^(mp3|flac|wav|ogg|opus|m4a|aac|wma|aiff)$/, AudiotrackOutlined],
  [/^(png|jpe?g|gif|webp|svg|bmp|tiff?|avif|heic)$/, ImageOutlined],
  [/^(zip|rar|7z|tar|gz|bz2|xz|zst|iso|img|deb|rpm|pkg|dmg|apk)$/, ArchiveOutlined],
  [/^pdf$/, PictureAsPdfOutlined],
  [/^(doc|docx|odt|xls|xlsx|ods|ppt|pptx|odp|txt|md|csv|epub)$/, DescriptionOutlined],
  [/^(exe|msi|appimage|sh|bat|bin|run|jar|deb)$/, TerminalOutlined],
];

/** Fixed 16:9 box, always the same size, whatever the file.
 *
 *  There is no thumbnail to fetch for a direct download, so the box carries the
 *  two things that *are* known before a single byte lands: what kind of file it
 *  is, and its extension. The size is reserved up front either way — a card
 *  that resizes as it learns about itself is a card that jumps under the mouse. */
export function Thumb({ d }: { d: DownloadInfo }) {
  const ext = extOf(d);
  const Icon = FAMILIES.find(([re]) => re.test(ext))?.[1] ?? InsertDriveFileOutlined;

  return (
    <Box
      sx={{
        position: "relative",
        flexShrink: 0,
        width: 128,
        height: 72,
        borderRadius: `${shape.sm}px`,
        overflow: "hidden",
        bgcolor: tok.surfaceHighest,
        border: `1px solid ${tok.outlineVariant}`,
        display: "grid",
        placeItems: "center",
        gap: 0.25,
        color: "text.secondary",
        "& svg": { fontSize: 26 },
      }}
    >
      <Icon />
      {!!ext && (
        <Typography
          variant="caption"
          sx={{ fontSize: 10, letterSpacing: 0.6, textTransform: "uppercase", lineHeight: 1 }}
        >
          {ext.slice(0, 6)}
        </Typography>
      )}
    </Box>
  );
}
