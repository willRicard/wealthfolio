import { QueryKeys } from "@/lib/query-keys";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Input } from "@wealthfolio/ui";
import {
  deleteTransactionAttachment,
  openTransactionAttachment,
  transactionAttachmentThumbnailUrl,
  type TransactionAttachment,
} from "@/adapters";

import { MAX_TRANSACTION_FILES, transactionFileError } from "../lib/transaction-files";

interface TransactionAttachmentsProps {
  activityId?: string;
  attachments: TransactionAttachment[];
  pendingFiles: File[];
  onFilesChange: (files: File[]) => void;
  disabled?: boolean;
}

function AttachmentThumbnail({ attachment }: { attachment: TransactionAttachment }) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const thumbnail = useQuery({
    queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, attachment.id, "thumbnail"],
    queryFn: () => transactionAttachmentThumbnailUrl(attachment.activityId, attachment.id),
    staleTime: Infinity,
  });
  useEffect(() => {
    const url = thumbnail.data;
    return () => {
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    };
  }, [thumbnail.data]);
  return (
    <button
      type="button"
      onClick={() => void openTransactionAttachment(attachment.activityId, attachment)}
      className="min-w-0 flex-1 rounded-md border p-2 focus-visible:outline focus-visible:outline-2"
      aria-label={t("spending:transactionAttachments.viewOriginal", {
        filename: attachment.filename,
      })}
    >
      {failed || thumbnail.isError ? (
        <p className="text-muted-foreground flex h-24 items-center justify-center text-xs">
          {t("spending:transactionAttachments.thumbnailError")}
        </p>
      ) : (
        <img
          src={thumbnail.data}
          alt=""
          className="bg-muted h-24 w-full rounded object-contain"
          loading="lazy"
          onError={() => setFailed(true)}
        />
      )}
      <p className="mt-1 truncate text-sm" title={attachment.filename}>
        {attachment.filename}
      </p>
    </button>
  );
}

export function TransactionAttachments({
  activityId,
  attachments,
  pendingFiles,
  onFilesChange,
  disabled,
}: TransactionAttachmentsProps) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const fileInputId = useId();
  const limitsId = `${fileInputId}-limits`;
  const [error, setError] = useState<string>();
  const deletion = useMutation({
    mutationFn: (id: string) => deleteTransactionAttachment(activityId!, id),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, activityId] }),
  });
  const busy = disabled || deletion.isPending;
  return (
    <section className="space-y-3">
      <label htmlFor={fileInputId} className="text-sm font-medium">
        {t("spending:transactionAttachments.attachments")}
      </label>
      <p id={limitsId} className="text-muted-foreground text-xs">
        {t("spending:transactionAttachments.limits")}
      </p>
      <Input
        id={fileInputId}
        type="file"
        multiple
        accept="image/jpeg,image/png,image/webp,application/pdf"
        disabled={busy || pendingFiles.length + attachments.length >= MAX_TRANSACTION_FILES}
        aria-describedby={limitsId}
        onChange={(event) => {
          const next = [...pendingFiles, ...Array.from(event.target.files ?? [])];
          const issue = transactionFileError(next, attachments.length);
          setError(issue);
          if (!issue) onFilesChange(next);
          event.target.value = "";
        }}
      />
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {t(`spending:transactionAttachments.${error}`)}
        </p>
      )}
      {deletion.isError && (
        <p role="alert" className="text-destructive text-sm">
          {deletion.error.message}
        </p>
      )}
      {pendingFiles.length > 0 && (
        <p className="text-muted-foreground text-xs">
          {t("spending:transactionAttachments.pendingHint")}
        </p>
      )}
      {pendingFiles.map((file, index) => (
        <div key={`${file.name}-${index}`} className="flex items-center gap-2 text-sm">
          <span className="min-w-0 flex-1 truncate">{file.name}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => onFilesChange(pendingFiles.filter((_, i) => i !== index))}
          >
            {t("spending:transactionAttachments.remove")}
          </Button>
        </div>
      ))}
      <div className="grid grid-cols-2 gap-3">
        {attachments.map((attachment) => (
          <div key={attachment.id} className="flex min-w-0 flex-col gap-1">
            <AttachmentThumbnail attachment={attachment} />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => deletion.mutate(attachment.id)}
            >
              {t("spending:transactionAttachments.remove")}
            </Button>
          </div>
        ))}
      </div>
    </section>
  );
}
