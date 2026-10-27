import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@wealthfolio/ui";
import type { PasskeySummary } from "@/features/passkeys/api";

export function PasskeyRenameDialog({
  passkey,
  onClose,
  onSave,
  isSaving,
}: {
  passkey: PasskeySummary;
  onClose: () => void;
  onSave: (name: string) => Promise<unknown>;
  isSaving: boolean;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(passkey.name);

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed || isSaving) return;
    try {
      await onSave(trimmed);
      onClose();
    } catch {
      // Surfaced as a toast by the hook; leave the dialog open to retry.
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("settings:passkeys.rename_title")}</DialogTitle>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="passkey-rename">{t("settings:passkeys.name_label")}</Label>
          <Input
            id="passkey-rename"
            value={name}
            autoFocus
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void submit();
              }
            }}
          />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={isSaving}>
            {t("common:cancel")}
          </Button>
          <Button onClick={() => void submit()} disabled={!name.trim() || isSaving}>
            {t("common:save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
