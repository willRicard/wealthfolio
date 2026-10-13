import { useCallback, useEffect, useRef, useState } from "react";

import {
  deleteTransactionAttachment,
  SUPPORTS_UPLOAD_CANCEL,
  SUPPORTS_UPLOAD_PROGRESS,
  uploadTransactionAttachment,
  type AttachmentTarget,
  type TransactionAttachment,
} from "@/adapters";

export type AttachmentUploadStatus = "queued" | "uploading" | "failed" | "cancelled";

export interface AttachmentUpload {
  id: string;
  file: File;
  status: AttachmentUploadStatus;
  /** Fraction in 0..1, only on a transport that can measure it. */
  progress?: number;
  error?: string;
}

export interface UseAttachmentUploadsOptions {
  target: AttachmentTarget;
  onUploaded?: () => void;
}

export interface UseAttachmentUploads {
  uploads: AttachmentUpload[];
  isUploading: boolean;
  add: (files: File[]) => void;
  retry: (id: string) => void;
  cancel: (id: string) => void;
  remove: (id: string) => void;
  /** Run the queue to completion, retrying what failed. Throws if any still fails. */
  flush: () => Promise<void>;
  reset: () => void;
}

const localId = () => crypto.randomUUID();

const message = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : "Upload failed";

/**
 * Sequential upload queue for one transaction form.
 */
export function useAttachmentUploads({
  target: targetProp,
  onUploaded,
}: UseAttachmentUploadsOptions): UseAttachmentUploads {
  const [uploads, setUploads] = useState<AttachmentUpload[]>([]);
  const list = useRef<AttachmentUpload[]>([]);
  const target = useRef(targetProp);
  const controllers = useRef(new Map<string, AbortController>());
  const cancelled = useRef(new Set<string>());
  const running = useRef<Promise<void> | null>(null);
  const uploaded = useRef(onUploaded);
  useEffect(() => {
    uploaded.current = onUploaded;
  }, [onUploaded]);

  const write = useCallback((next: AttachmentUpload[]) => {
    list.current = next;
    setUploads(next);
  }, []);

  const patch = useCallback(
    (id: string, changes: Partial<AttachmentUpload>) =>
      write(list.current.map((item) => (item.id === id ? { ...item, ...changes } : item))),
    [write],
  );

  const send = useCallback(
    async (item: AttachmentUpload, to: AttachmentTarget) => {
      const controller = new AbortController();
      controllers.current.set(item.id, controller);
      patch(item.id, {
        status: "uploading",
        error: undefined,
        progress: SUPPORTS_UPLOAD_PROGRESS ? 0 : undefined,
      });
      try {
        const attachment = await uploadTransactionAttachment(to, item.file, {
          onProgress: (fraction) => patch(item.id, { progress: fraction }),
          signal: controller.signal,
        });
        if (cancelled.current.has(item.id)) {
          await discard(to, attachment);
          patch(item.id, { status: "cancelled", progress: undefined });
          return;
        }
        write(list.current.filter((entry) => entry.id !== item.id));
        uploaded.current?.();
      } catch (error) {
        const aborted = error instanceof Error && error.name === "AbortError";
        patch(item.id, {
          status: aborted ? "cancelled" : "failed",
          progress: undefined,
          error: aborted ? undefined : message(error),
        });
      } finally {
        cancelled.current.delete(item.id);
        controllers.current.delete(item.id);
      }
    },
    [patch, write],
  );

  const pump = useCallback(async () => {
    for (;;) {
      const next = list.current.find((item) => item.status === "queued");
      if (!next) return;
      await send(next, target.current);
    }
  }, [send]);

  const drain = useCallback(() => {
    const pass = (running.current ?? Promise.resolve()).then(pump);
    running.current = pass;
    void pass.finally(() => {
      if (running.current === pass) running.current = null;
    });
    return pass;
  }, [pump]);

  useEffect(() => {
    target.current = targetProp;
  }, [targetProp]);

  const add = useCallback(
    (files: File[]) => {
      write([
        ...list.current,
        ...files.map((file) => ({ id: localId(), file, status: "queued" as const })),
      ]);
      void drain();
    },
    [drain, write],
  );

  const retry = useCallback(
    (id: string) => {
      cancelled.current.delete(id);
      patch(id, { status: "queued", error: undefined, progress: undefined });
      void drain();
    },
    [drain, patch],
  );

  const cancel = useCallback(
    (id: string) => {
      cancelled.current.add(id);
      controllers.current.get(id)?.abort();
      const item = list.current.find((entry) => entry.id === id);
      if (item?.status === "queued") patch(id, { status: "cancelled" });
    },
    [patch],
  );

  const remove = useCallback(
    (id: string) => {
      cancelled.current.add(id);
      controllers.current.get(id)?.abort();
      write(list.current.filter((item) => item.id !== id));
    },
    [write],
  );

  const flush = useCallback(async () => {
    write(
      list.current.map((item) => {
        if (item.status !== "failed") return item;
        cancelled.current.delete(item.id);
        return { ...item, status: "queued" as const, error: undefined };
      }),
    );
    await drain();
    const failed = list.current.find((item) => item.status === "failed");
    if (failed) throw new Error(failed.error ?? "Upload failed");
  }, [drain, write]);

  const reset = useCallback(() => {
    for (const controller of controllers.current.values()) controller.abort();
    controllers.current.clear();
    cancelled.current.clear();
    write([]);
  }, [write]);

  return {
    uploads,
    isUploading: uploads.some((item) => item.status === "queued" || item.status === "uploading"),
    add,
    retry,
    cancel,
    remove,
    flush,
    reset,
  };
}

async function discard(target: AttachmentTarget, attachment: TransactionAttachment) {
  if (SUPPORTS_UPLOAD_CANCEL) return;
  try {
    await deleteTransactionAttachment(target, attachment.id);
  } catch {
    /* the attachment stays; the user can remove it from the list */
  }
}
