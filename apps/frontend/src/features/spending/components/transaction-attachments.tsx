import { QueryKeys } from "@/lib/query-keys";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileDropzone, FileTile, useObjectUrl } from "@wealthfolio/ui";
import {
  deleteTransactionAttachment,
  fetchTransactionAttachmentThumbnail,
  openTransactionAttachment,
  SUPPORTS_UPLOAD_PROGRESS,
  type AttachmentTarget,
  type TransactionAttachment,
} from "@/adapters";

import type { AttachmentUpload, UseAttachmentUploads } from "../hooks/use-attachment-uploads";
import { MAX_TRANSACTION_FILES, transactionFileError } from "../lib/transaction-files";

interface TransactionAttachmentsProps {
  target: AttachmentTarget;
  attachments: TransactionAttachment[];
  uploads: UseAttachmentUploads;
  disabled?: boolean;
}

function PendingPreview({ file }: { file: File }) {
  const url = useObjectUrl(file.type.startsWith("image/") ? file : undefined);
  if (!url) return null;
  return <img src={url} alt="" className="bg-muted h-24 w-full rounded object-contain" />;
}

function AttachmentPreview({
  target,
  attachment,
}: {
  target: AttachmentTarget;
  attachment: TransactionAttachment;
}) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const blobQuery = useQuery({
    queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, attachment.id, "thumbnail"],
    queryFn: () => fetchTransactionAttachmentThumbnail(target, attachment.id),
    staleTime: Infinity,
  });
  const blobUrl = useObjectUrl(blobQuery.data);

  if (failed || blobQuery.isError)
    return (
      <p className="text-muted-foreground flex h-24 items-center justify-center text-xs">
        {t("spending:transactionAttachments.thumbnailError")}
      </p>
    );
  return (
    <img
      src={blobUrl}
      alt=""
      className="bg-muted h-24 w-full rounded object-contain"
      loading="lazy"
      onError={() => setFailed(true)}
    />
  );
}

export function TransactionAttachments({
  target,
  attachments,
  uploads,
  disabled,
}: TransactionAttachmentsProps) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const fileInputId = useId();
  const [error, setError] = useState<string>();
  const targetKey = target.kind === "draft" ? target.token : target.id;
  const deletion = useMutation({
    mutationFn: (id: string) => deleteTransactionAttachment(target, id),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, targetKey] }),
  });
  const busy = disabled || deletion.isPending;
  const total = attachments.length + uploads.uploads.length;

  const describe = (upload: AttachmentUpload) => {
    switch (upload.status) {
      case "queued":
        return t("spending:transactionAttachments.queued");
      case "failed":
        return t("spending:transactionAttachments.uploadFailed");
      case "cancelled":
        return t("spending:transactionAttachments.cancelled");
      case "uploading":
        return SUPPORTS_UPLOAD_PROGRESS && upload.progress !== undefined
          ? t("spending:transactionAttachments.uploadProgress", {
              percent: Math.round(upload.progress * 100),
            })
          : t("spending:transactionAttachments.uploading");
    }
  };

  return (
    <section className="space-y-3">
      <label htmlFor={fileInputId} className="text-sm font-medium">
        {t("spending:transactionAttachments.attachments")}
      </label>
      <FileDropzone
        id={fileInputId}
        multiple
        accept="image/jpeg,image/png,image/webp,application/pdf"
        disabled={busy || total >= MAX_TRANSACTION_FILES}
        ariaLabel={t("spending:transactionAttachments.clickToUpload")}
        label={t("spending:transactionAttachments.clickToUpload")}
        hint={t("spending:transactionAttachments.orDrop")}
        description={t("spending:transactionAttachments.limits")}
        onFiles={(picked) => {
          const next = [...uploads.uploads.map((upload) => upload.file), ...picked];
          const issue = transactionFileError(next, attachments.length);
          setError(issue);
          if (!issue) uploads.add(picked);
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
      <div className="grid grid-cols-2 gap-3">
        {uploads.uploads.map((upload) => {
          const inFlight = upload.status === "uploading";
          const stopped = upload.status === "failed" || upload.status === "cancelled";
          return (
            <FileTile
              key={upload.id}
              name={upload.file.name}
              preview={<PendingPreview file={upload.file} />}
              caption={describe(upload)}
              progress={SUPPORTS_UPLOAD_PROGRESS ? upload.progress : undefined}
              indeterminate={inFlight && !SUPPORTS_UPLOAD_PROGRESS}
              error={upload.status === "failed" ? upload.error : undefined}
              onRetry={stopped ? () => uploads.retry(upload.id) : undefined}
              retryLabel={t("common:retry")}
              onCancel={inFlight ? () => uploads.cancel(upload.id) : undefined}
              cancelLabel={t("spending:transactionAttachments.cancel")}
              onRemove={inFlight ? undefined : () => uploads.remove(upload.id)}
              removeLabel={t("spending:transactionAttachments.remove")}
              disabled={busy}
            />
          );
        })}
        {attachments.map((attachment) => (
          <FileTile
            key={attachment.id}
            data-testid="attachment-thumbnail"
            name={attachment.filename}
            preview={<AttachmentPreview target={target} attachment={attachment} />}
            onOpen={() => void openTransactionAttachment(target, attachment)}
            openLabel={t("spending:transactionAttachments.viewOriginal", {
              filename: attachment.filename,
            })}
            onRemove={() => deletion.mutate(attachment.id)}
            removeLabel={t("spending:transactionAttachments.remove")}
            disabled={busy}
          />
        ))}
      </div>
    </section>
  );
}
