import Box from "@mui/material/Box";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Invisible grips along the window edges.
 *
 *  With `decorations: false` there is no server-side frame to grab, and on
 *  Wayland GTK draws no resize border of its own, so the window would be stuck
 *  at its start size for anyone not using a compositor keybinding. */
const EDGES = [
  { dir: "North", sx: { top: 0, left: 8, right: 8, height: 4, cursor: "ns-resize" } },
  { dir: "South", sx: { bottom: 0, left: 8, right: 8, height: 4, cursor: "ns-resize" } },
  { dir: "West", sx: { left: 0, top: 8, bottom: 8, width: 4, cursor: "ew-resize" } },
  { dir: "East", sx: { right: 0, top: 8, bottom: 8, width: 4, cursor: "ew-resize" } },
  { dir: "NorthWest", sx: { top: 0, left: 0, width: 8, height: 8, cursor: "nwse-resize" } },
  { dir: "NorthEast", sx: { top: 0, right: 0, width: 8, height: 8, cursor: "nesw-resize" } },
  { dir: "SouthWest", sx: { bottom: 0, left: 0, width: 8, height: 8, cursor: "nesw-resize" } },
  { dir: "SouthEast", sx: { bottom: 0, right: 0, width: 8, height: 8, cursor: "nwse-resize" } },
] as const;

export function ResizeHandles() {
  return (
    <>
      {EDGES.map((e) => (
        <Box
          key={e.dir}
          onMouseDown={(ev) => {
            if (ev.button !== 0) return;
            ev.preventDefault();
            getCurrentWindow().startResizeDragging(e.dir).catch(() => {});
          }}
          sx={{ position: "fixed", zIndex: (t) => t.zIndex.tooltip + 1, ...e.sx }}
        />
      ))}
    </>
  );
}
