import { QueryKeys } from "@/lib/query-keys";
import { useCallback, useId, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  closestCenter,
  DndContext,
  MouseSensor,
  TouchSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type Modifier,
} from "@dnd-kit/core";
import { rectSortingStrategy, SortableContext, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  cn,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  FileDropzone,
  FileTile,
  Icons,
  useDateFormatting,
  useObjectUrl,
} from "@wealthfolio/ui";
import {
  deleteTransactionAttachment,
  fetchTransactionAttachmentThumbnail,
  openTransactionAttachment,
  SUPPORTS_UPLOAD_PROGRESS,
  type AttachmentTarget,
  type TransactionAttachment,
} from "@/adapters";

import type { AttachmentSavedItem, UseAttachmentOrder } from "../hooks/use-attachment-order";
import type { AttachmentUpload, UseAttachmentUploads } from "../hooks/use-attachment-uploads";
import { MAX_TRANSACTION_FILES, transactionFileError } from "../lib/transaction-files";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function AttachmentInfoDialog({
  target,
  attachment,
}: {
  target: AttachmentTarget;
  attachment: TransactionAttachment;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const formatting = useDateFormatting();

  const blobQuery = useQuery({
    queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, attachment.id, "thumbnail"],
    queryFn: () => fetchTransactionAttachmentThumbnail(target, attachment.id),
    staleTime: Infinity,
  });
  const thumbnailUrl = useObjectUrl(blobQuery.data);

  return (
    <>
      <button
        type="button"
        aria-label={t("spending:transactionAttachments.fileInfoButton", {
          filename: attachment.filename,
        })}
        className="bg-popover pointer-fine:group-hover:flex pointer-fine:group-focus-within:flex absolute right-1 top-1 z-10 hidden size-6 items-center justify-center rounded-full border shadow-sm"
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => setOpen(true)}
      >
        <Icons.Info className="size-3.5" aria-hidden />
      </button>
      <Dialog open={open} onOpenChange={setOpen} useIsMobile={() => false}>
        <DialogContent className="w-[calc(100%-2rem)]">
          <DialogHeader>
            <DialogTitle>{t("spending:transactionAttachments.fileInfo")}</DialogTitle>
          </DialogHeader>
          {thumbnailUrl && (
            <img
              src={thumbnailUrl}
              alt=""
              className="bg-muted max-h-64 w-full rounded-md object-contain"
            />
          )}
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
            <dt className="text-muted-foreground">
              {t("spending:transactionAttachments.fileName")}
            </dt>
            <dd className="min-w-0 break-all">{attachment.filename}</dd>
            <dt className="text-muted-foreground">
              {t("spending:transactionAttachments.fileSize")}
            </dt>
            <dd>{formatBytes(attachment.sizeBytes)}</dd>
            <dt className="text-muted-foreground">
              {t("spending:transactionAttachments.fileType")}
            </dt>
            <dd>{attachment.contentType}</dd>
            <dt className="text-muted-foreground">
              {t("spending:transactionAttachments.uploadDate")}
            </dt>
            <dd>{formatting.formatDate(attachment.createdAt)}</dd>
          </dl>
        </DialogContent>
      </Dialog>
    </>
  );
}

interface TransactionAttachmentsProps {
  target: AttachmentTarget;
  uploads: UseAttachmentUploads;
  order: UseAttachmentOrder;
  disabled?: boolean;
}

const PLACEHOLDER = "bg-muted flex h-24 w-full items-center justify-center rounded";

function PendingPreview({ file }: { file: File }) {
  const url = useObjectUrl(file.type.startsWith("image/") ? file : undefined);
  if (!url) return <span className={PLACEHOLDER} aria-hidden />;
  return <img src={url} alt="" className="bg-muted h-24 w-full rounded object-contain" />;
}

function AttachmentPreview({
  target,
  attachment,
  localFile,
}: {
  target: AttachmentTarget;
  attachment: TransactionAttachment;
  localFile?: File;
}) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const [thumbnailReady, setThumbnailReady] = useState(false);
  const blobQuery = useQuery({
    queryKey: [QueryKeys.TRANSACTION_ATTACHMENTS, attachment.id, "thumbnail"],
    queryFn: () => fetchTransactionAttachmentThumbnail(target, attachment.id),
    staleTime: Infinity,
  });
  const blobUrl = useObjectUrl(blobQuery.data);
  const localUrl = useObjectUrl(localFile?.type.startsWith("image/") ? localFile : undefined);

  if (failed || blobQuery.isError)
    return (
      <p className="text-muted-foreground flex h-24 items-center justify-center text-xs">
        {t("spending:transactionAttachments.thumbnailError")}
      </p>
    );

  const showLocal = !!localUrl && !thumbnailReady;
  const src = showLocal ? localUrl : blobUrl;
  return (
    <>
      {showLocal && blobUrl && (
        <img
          src={blobUrl}
          alt=""
          aria-hidden
          className="hidden"
          onLoad={() => setThumbnailReady(true)}
          onError={() => setThumbnailReady(true)}
        />
      )}
      {src ? (
        <img
          src={src}
          alt=""
          className="bg-muted h-24 w-full rounded object-contain"
          loading="lazy"
          onError={() => setFailed(true)}
        />
      ) : (
        <span className={PLACEHOLDER} aria-hidden />
      )}
    </>
  );
}

function SortableTile({
  id,
  index,
  count,
  filename,
  reorderable,
  onMoveBy,
  children,
}: {
  id: string;
  index: number;
  count: number;
  filename: string;
  reorderable: boolean;
  onMoveBy: (delta: number) => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
    disabled: !reorderable,
  });

  const nudge = (delta: number, label: string, icon: ReactNode, disabled: boolean) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={() => onMoveBy(delta)}
      className="hover:bg-muted focus-visible:ring-ring flex size-6 items-center justify-center rounded-full focus-visible:outline-none focus-visible:ring-2 disabled:opacity-40"
    >
      {icon}
    </button>
  );

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(
        "group relative",
        reorderable && "cursor-grab",
        isDragging && "z-10 opacity-80",
      )}
      {...listeners}
    >
      {children}
      {reorderable && count > 1 && (
        <div className="bg-popover pointer-fine:group-hover:flex pointer-fine:group-focus-within:flex absolute left-1 top-1 z-10 hidden gap-0.5 rounded-full border p-0.5 shadow-sm">
          {nudge(
            -1,
            t("spending:transactionAttachments.moveLeft", { filename }),
            <Icons.ChevronLeft className="size-4" aria-hidden />,
            index === 0,
          )}
          {nudge(
            1,
            t("spending:transactionAttachments.moveRight", { filename }),
            <Icons.ChevronRight className="size-4" aria-hidden />,
            index === count - 1,
          )}
        </div>
      )}
    </div>
  );
}

export function TransactionAttachments({
  target,
  uploads,
  order,
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
  const items = order.items;
  const pending = items.flatMap((item) => (item.kind === "upload" ? [item.upload.file] : []));

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 8 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 200, tolerance: 8 } }),
  );

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
      case "done":
        return undefined;
    }
  };

  const removeSaved = (item: AttachmentSavedItem) =>
    deletion.mutate(item.attachment.id, {
      onSuccess: () => {
        if (item.uploadId) uploads.remove(item.uploadId);
      },
    });

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (over && active.id !== over.id) order.move(String(active.id), String(over.id));
  };

  const gridRef = useRef<HTMLDivElement>(null);

  const restrictToGridBounds = useCallback<Modifier>(({ transform, draggingNodeRect }) => {
    if (!gridRef.current || !draggingNodeRect) return transform;
    const grid = gridRef.current.getBoundingClientRect();
    return {
      ...transform,
      x: Math.min(
        Math.max(transform.x, grid.left - draggingNodeRect.left),
        grid.right - draggingNodeRect.right,
      ),
      y: Math.min(
        Math.max(transform.y, grid.top - draggingNodeRect.top),
        grid.bottom - draggingNodeRect.bottom,
      ),
    };
  }, []);

  return (
    <section className="space-y-3">
      <label htmlFor={fileInputId} className="text-sm font-medium">
        {t("spending:transactionAttachments.attachments")}
      </label>
      <FileDropzone
        id={fileInputId}
        multiple
        accept="image/jpeg,image/png,image/webp,application/pdf"
        disabled={busy || items.length >= MAX_TRANSACTION_FILES}
        ariaLabel={t("spending:transactionAttachments.clickToUpload")}
        label={t("spending:transactionAttachments.clickToUpload")}
        hint={t("spending:transactionAttachments.orDrop")}
        description={t("spending:transactionAttachments.limits")}
        onFiles={(picked) => {
          const issue = transactionFileError(
            [...pending, ...picked],
            items.length - pending.length,
          );
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
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={onDragEnd}
        modifiers={[restrictToGridBounds]}
      >
        <SortableContext items={items.map((item) => item.key)} strategy={rectSortingStrategy}>
          <div ref={gridRef} className="grid grid-cols-2 gap-3">
            {items.map((item, index) => {
              const filename =
                item.kind === "upload" ? item.upload.file.name : item.attachment.filename;
              return (
                <SortableTile
                  key={item.key}
                  id={item.key}
                  index={index}
                  count={items.length}
                  filename={filename}
                  reorderable={!busy}
                  onMoveBy={(delta) => order.moveBy(item.key, delta)}
                >
                  {item.kind === "upload" ? (
                    <UploadTile
                      upload={item.upload}
                      caption={describe(item.upload)}
                      uploads={uploads}
                      disabled={busy}
                    />
                  ) : (
                    <>
                      <FileTile
                        data-testid="attachment-thumbnail"
                        name={item.attachment.filename}
                        preview={
                          <AttachmentPreview
                            target={target}
                            attachment={item.attachment}
                            localFile={item.localFile}
                          />
                        }
                        onOpen={() => void openTransactionAttachment(target, item.attachment)}
                        openLabel={t("spending:transactionAttachments.viewOriginal", {
                          filename: item.attachment.filename,
                        })}
                        onRemove={() => removeSaved(item)}
                        removeLabel={t("spending:transactionAttachments.remove")}
                        disabled={busy}
                      />
                      <AttachmentInfoDialog target={target} attachment={item.attachment} />
                    </>
                  )}
                </SortableTile>
              );
            })}
          </div>
        </SortableContext>
      </DndContext>
    </section>
  );
}

function UploadTile({
  upload,
  caption,
  uploads,
  disabled,
}: {
  upload: AttachmentUpload;
  caption?: string;
  uploads: UseAttachmentUploads;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const inFlight = upload.status === "uploading";
  const stopped = upload.status === "failed" || upload.status === "cancelled";
  return (
    <FileTile
      name={upload.file.name}
      preview={<PendingPreview file={upload.file} />}
      caption={caption}
      progress={SUPPORTS_UPLOAD_PROGRESS ? upload.progress : undefined}
      indeterminate={inFlight && !SUPPORTS_UPLOAD_PROGRESS}
      error={upload.status === "failed" ? upload.error : undefined}
      onRetry={stopped ? () => uploads.retry(upload.id) : undefined}
      retryLabel={t("common:retry")}
      onCancel={inFlight ? () => uploads.cancel(upload.id) : undefined}
      cancelLabel={t("spending:transactionAttachments.cancel")}
      onRemove={inFlight ? undefined : () => uploads.remove(upload.id)}
      removeLabel={t("spending:transactionAttachments.remove")}
      disabled={disabled}
    />
  );
}
