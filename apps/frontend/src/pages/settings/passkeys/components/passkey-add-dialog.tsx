import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@wealthfolio/ui";
import { supportsPasskeys } from "@/features/passkeys/api";

const DEVICE_HINTS: readonly [string, string][] = [
  ["iPhone", "iPhone"],
  ["iPad", "iPad"],
  ["Android", "Android phone"],
  ["Macintosh", "Mac"],
  ["Windows", "Windows PC"],
];

function suggestName(): string {
  const agent = navigator.userAgent;
  return DEVICE_HINTS.find(([needle]) => agent.includes(needle))?.[1] ?? "";
}

export function PasskeyAddDialog({
  open,
  onOpenChange,
  onAdd,
  isAdding,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (name: string) => Promise<unknown>;
  isAdding: boolean;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const supported = supportsPasskeys();

  useEffect(() => {
    if (open) setName(suggestName());
  }, [open]);

  const submit = async () => {
    if (!name.trim() || isAdding) return;
    try {
      await onAdd(name.trim());
      onOpenChange(false);
    } catch {
      // The hook already raised a toast; keep the dialog open to retry.
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("settings:passkeys.add_title")}</DialogTitle>
          <DialogDescription>
            {supported
              ? t("settings:passkeys.add_description")
              : t("settings:passkeys.unsupported")}
          </DialogDescription>
        </DialogHeader>

        {supported ? (
          <div className="space-y-2">
            <Label htmlFor="passkey-name">{t("settings:passkeys.name_label")}</Label>
            <Input
              id="passkey-name"
              value={name}
              autoFocus
              maxLength={64}
              placeholder={t("settings:passkeys.name_placeholder")}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void submit();
                }
              }}
            />
          </div>
        ) : null}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isAdding}>
            {t("common:cancel")}
          </Button>
          <Button onClick={() => void submit()} disabled={!supported || !name.trim() || isAdding}>
            {isAdding ? t("settings:passkeys.adding") : t("settings:passkeys.add")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
