import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { formatDistanceToNowStrict, type Locale } from "date-fns";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  dateFnsLocaleFor,
  Icons,
  Skeleton,
  useDateFormatting,
  useLocalizationSettings,
} from "@wealthfolio/ui";
import type { PasskeySummary } from "@/features/passkeys/api";
import { usePasskeys } from "../hooks/use-passkeys";
import { PasskeyAddDialog } from "./passkey-add-dialog";
import { PasskeyRenameDialog } from "./passkey-rename-dialog";

function formatLastUsed(t: TFunction, lastUsedAt: string | null, locale: Locale) {
  return lastUsedAt
    ? t("settings:passkeys.last_used", {
        time: formatDistanceToNowStrict(new Date(lastUsedAt), { addSuffix: true, locale }),
      })
    : t("settings:passkeys.never_used");
}

export function PasskeyTable() {
  const { t } = useTranslation();
  const formatting = useDateFormatting();
  const { uiLocale } = useLocalizationSettings();
  const dateFnsLocale = dateFnsLocaleFor(uiLocale);
  const { configured, passkeys, isLoading, createMutation, renameMutation, deleteMutation } =
    usePasskeys();

  const [addOpen, setAddOpen] = useState(false);
  const [renaming, setRenaming] = useState<PasskeySummary | null>(null);
  const [removing, setRemoving] = useState<PasskeySummary | null>(null);

  if (!isLoading && !configured) {
    return (
      <Card className="rounded-lg">
        <CardHeader className="p-6 pb-4">
          <CardTitle className="text-base font-semibold tracking-tight">
            {t("settings:passkeys.title")}
          </CardTitle>
          <CardDescription className="text-xs">
            {t("settings:passkeys.unconfigured")}
          </CardDescription>
        </CardHeader>
        <CardContent className="p-6 pt-0">
          <pre className="bg-muted/40 text-muted-foreground overflow-x-auto rounded-md border p-4 text-xs">
            WF_WEBAUTHN_RP_ID=your.domain
          </pre>
        </CardContent>
      </Card>
    );
  }

  const isLast = passkeys.length === 1;

  return (
    <Card className="rounded-lg">
      <CardHeader className="flex flex-row items-start justify-between gap-3 space-y-0 p-6 pb-4">
        <div className="min-w-0 space-y-1">
          <CardTitle className="text-base font-semibold tracking-tight">
            {t("settings:passkeys.title")}
          </CardTitle>
          <CardDescription className="text-xs">
            {t("settings:passkeys.description")}
          </CardDescription>
        </div>
        <Button size="sm" onClick={() => setAddOpen(true)} disabled={createMutation.isPending}>
          <Icons.Plus className="me-2 h-4 w-4" />
          {t("settings:passkeys.add")}
        </Button>
      </CardHeader>

      <CardContent className="p-6 pt-0">
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-16 rounded-md" />
            <Skeleton className="h-16 rounded-md" />
          </div>
        ) : passkeys.length === 0 ? (
          <div className="text-muted-foreground rounded-md border border-dashed py-8 text-center text-xs">
            {t("settings:passkeys.empty")}
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {passkeys.map((passkey) => (
              <div
                key={passkey.id}
                className="border-border bg-muted/30 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-4 rounded-md border px-4 py-3.5 sm:grid-cols-[auto_minmax(0,1fr)_auto]"
              >
                <div className="bg-background border-border text-foreground flex h-8 w-8 items-center justify-center rounded-md border">
                  <Icons.ShieldCheck size={18} weight="duotone" />
                </div>
                <div className="min-w-0 space-y-1">
                  <span className="truncate text-sm font-medium">{passkey.name}</span>
                  <p className="text-muted-foreground text-xs">
                    {t("settings:passkeys.added", {
                      date: formatting.formatDate(new Date(passkey.createdAt)),
                    })}{" "}
                    · {formatLastUsed(t, passkey.lastUsedAt, dateFnsLocale)}
                  </p>
                </div>
                <div className="col-span-2 flex flex-wrap items-center justify-end gap-3 sm:col-span-1">
                  <Button variant="outline" size="sm" onClick={() => setRenaming(passkey)}>
                    {t("settings:passkeys.rename")}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="text-destructive hover:text-destructive hover:border-destructive/40 rounded-full"
                    onClick={() => setRemoving(passkey)}
                  >
                    {t("settings:passkeys.remove")}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>

      <PasskeyAddDialog
        open={addOpen}
        onOpenChange={(open) => {
          setAddOpen(open);
          if (!open) createMutation.reset();
        }}
        onAdd={(name) => createMutation.mutateAsync(name)}
        isAdding={createMutation.isPending}
      />

      {renaming && (
        <PasskeyRenameDialog
          key={renaming.id}
          passkey={renaming}
          onClose={() => setRenaming(null)}
          onSave={(name) => renameMutation.mutateAsync({ id: renaming.id, name })}
          isSaving={renameMutation.isPending}
        />
      )}

      <AlertDialog open={removing !== null} onOpenChange={(value) => !value && setRemoving(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings:passkeys.remove_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings:passkeys.remove_desc", { name: removing?.name ?? "" })}
              {isLast ? ` ${t("settings:passkeys.remove_last_warning")}` : ""}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>
              {t("common:cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={deleteMutation.isPending}
              onClick={() => {
                if (!removing) return;
                deleteMutation.mutate(removing.id, { onSuccess: () => setRemoving(null) });
              }}
            >
              {t("settings:passkeys.remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
