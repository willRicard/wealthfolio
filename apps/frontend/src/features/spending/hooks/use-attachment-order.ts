import { useCallback, useMemo, useRef, useState } from "react";

import type { TransactionAttachment } from "@/adapters";

import type { AttachmentUpload } from "./use-attachment-uploads";

export interface AttachmentUploadItem {
  key: string;
  kind: "upload";
  upload: AttachmentUpload;
}

export interface AttachmentSavedItem {
  key: string;
  kind: "saved";
  attachment: TransactionAttachment;
  localFile?: File;
  uploadId?: string;
}

export type AttachmentItem = AttachmentUploadItem | AttachmentSavedItem;

export interface UseAttachmentOrder {
  items: AttachmentItem[];
  isDirty: boolean;
  move: (fromKey: string, toKey: string) => void;
  moveBy: (key: string, delta: number) => void;
  currentOrder: () => string[];
  reset: () => void;
}

function synthesize(upload: AttachmentUpload, attachmentId: string): TransactionAttachment {
  return {
    id: attachmentId,
    filename: upload.file.name,
    contentType: upload.file.type,
    sizeBytes: upload.file.size,
    createdAt: "",
  };
}

function merge(
  attachments: TransactionAttachment[],
  uploads: AttachmentUpload[],
): AttachmentItem[] {
  const claimed = new Set(
    uploads.map((upload) => upload.attachmentId).filter((id): id is string => !!id),
  );
  const stored: AttachmentItem[] = attachments
    .filter((attachment) => !claimed.has(attachment.id))
    .map((attachment) => ({ key: attachment.id, kind: "saved", attachment }));
  const staged: AttachmentItem[] = uploads.map((upload) => {
    const attachmentId = upload.attachmentId;
    if (upload.status !== "done" || !attachmentId)
      return { key: upload.id, kind: "upload", upload };
    return {
      key: upload.id,
      kind: "saved",
      attachment:
        attachments.find((attachment) => attachment.id === attachmentId) ??
        synthesize(upload, attachmentId),
      localFile: upload.file,
      uploadId: upload.id,
    };
  });
  return [...stored, ...staged];
}

function applyOrder(items: AttachmentItem[], order: string[]): AttachmentItem[] {
  if (!order.length) return items;
  const byKey = new Map(items.map((item) => [item.key, item]));
  const placed = order
    .map((key) => byKey.get(key))
    .filter((item): item is AttachmentItem => !!item);
  const seen = new Set(placed.map((item) => item.key));
  return [...placed, ...items.filter((item) => !seen.has(item.key))];
}

export function useAttachmentOrder(
  attachments: TransactionAttachment[],
  uploads: AttachmentUpload[],
): UseAttachmentOrder {
  const [order, setOrder] = useState<string[]>([]);

  const natural = useMemo(() => merge(attachments, uploads), [attachments, uploads]);
  const items = useMemo(() => applyOrder(natural, order), [natural, order]);
  const isDirty = items.some((item, index) => item.key !== natural[index]?.key);

  const latest = useRef<string[]>([]);
  latest.current = items.map((item) => item.key);

  const reorder = useCallback((from: number, to: number, keys: string[]) => {
    if (from < 0 || to < 0 || from === to || to >= keys.length) return;
    const next = [...keys];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    setOrder(next);
  }, []);

  const move = useCallback(
    (fromKey: string, toKey: string) => {
      const keys = latest.current;
      reorder(keys.indexOf(fromKey), keys.indexOf(toKey), keys);
    },
    [reorder],
  );

  const moveBy = useCallback(
    (key: string, delta: number) => {
      const keys = latest.current;
      const from = keys.indexOf(key);
      reorder(from, from + delta, keys);
    },
    [reorder],
  );

  const currentOrder = useCallback(() => latest.current, []);
  const reset = useCallback(() => setOrder([]), []);

  return { items, isDirty, move, moveBy, currentOrder, reset };
}
