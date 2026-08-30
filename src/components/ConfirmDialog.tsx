import { useEffect, useState } from "react";
import Button from "@mui/material/Button";
import Checkbox from "@mui/material/Checkbox";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogContentText from "@mui/material/DialogContentText";
import DialogTitle from "@mui/material/DialogTitle";
import FormControlLabel from "@mui/material/FormControlLabel";

export interface ConfirmSpec {
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel: string;
  /** When set, the dialog offers a checkbox and hands its value to `onConfirm`. */
  checkboxLabel?: string;
  destructive?: boolean;
  onConfirm: (checked: boolean) => void;
}

/** One in-app confirmation for every destructive action.
 *
 *  Replaces the native `ask()` dialogs, which only appeared for a *finished*
 *  item — cancelling a download in flight destroyed it with no prompt at all. */
export function ConfirmDialog({
  spec,
  onClose,
}: {
  spec: ConfirmSpec | null;
  onClose: () => void;
}) {
  const [checked, setChecked] = useState(false);
  // Each new prompt starts from "keep the file": the safe answer should never
  // be inherited from whatever the user picked last time.
  useEffect(() => {
    if (spec) setChecked(false);
  }, [spec]);

  return (
    <Dialog open={!!spec} onClose={onClose} maxWidth="xs" fullWidth>
      {spec && (
        <>
          <DialogTitle>{spec.title}</DialogTitle>
          <DialogContent>
            <DialogContentText>{spec.body}</DialogContentText>
            {spec.checkboxLabel && (
              <FormControlLabel
                sx={{ mt: 1 }}
                control={
                  <Checkbox
                    checked={checked}
                    onChange={(e) => setChecked(e.target.checked)}
                  />
                }
                label={spec.checkboxLabel}
              />
            )}
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{spec.cancelLabel}</Button>
            <Button
              variant="contained"
              color={spec.destructive || checked ? "error" : "primary"}
              onClick={() => {
                spec.onConfirm(checked);
                onClose();
              }}
            >
              {spec.confirmLabel}
            </Button>
          </DialogActions>
        </>
      )}
    </Dialog>
  );
}
