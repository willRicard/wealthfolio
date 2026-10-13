import { invoke, invokeRaw } from "./core";
import {
  uploadAbortError,
  type AttachmentTarget,
  type AttachmentUploadOptions,
  type TransactionAttachment,
} from "../shared/transaction-attachments";

/** A single buffered IPC call has no mid-transfer moment the backend could report. */
export const SUPPORTS_UPLOAD_PROGRESS = false;
/**
 * There is no cancellation token on the Rust side: an `AbortSignal` can stop the call
 * from starting, but once bytes are in flight the upload runs to completion. A caller
 * aborting mid-flight should mark the item cancelled and issue a compensating delete
 * when the promise resolves.
 */
export const SUPPORTS_UPLOAD_CANCEL = false;

function bytes(value: ArrayBuffer | number[]): Uint8Array<ArrayBuffer> {
  return value instanceof ArrayBuffer ? new Uint8Array(value) : Uint8Array.from(value);
}

/** A draft is read by token through its own commands; the payload shape differs too. */
const read = (target: AttachmentTarget, attachmentId: string, preview: boolean) =>
  target.kind === "draft"
    ? invoke<ArrayBuffer | number[]>("read_transaction_attachment_draft", {
        token: target.token,
        attachmentId,
        preview,
      })
    : invoke<ArrayBuffer | number[]>("read_transaction_attachment", {
        activityId: target.id,
        attachmentId,
        preview,
      });

export async function fetchTransactionAttachmentThumbnail(
  target: AttachmentTarget,
  id: string,
): Promise<Blob> {
  return new Blob([bytes(await read(target, id, true))], { type: "image/webp" });
}

export async function openTransactionAttachment(
  target: AttachmentTarget,
  attachment: TransactionAttachment,
) {
  const viewer = window.open("about:blank", "_blank");
  try {
    const value = await read(target, attachment.id, false);
    const url = URL.createObjectURL(new Blob([bytes(value)], { type: attachment.contentType }));
    if (viewer) viewer.location.href = url;
    else window.open(url, "_blank", "noopener,noreferrer");
    window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
  } catch (error) {
    viewer?.close();
    throw error;
  }
}

/**
 * Uploads the file as a raw IPC body. Metadata travels in headers because the body is
 * bytes: header values must be ASCII, so the filename is percent-encoded UTF-8 and
 * decoded on the Rust side — `領収書.png` has to survive the round trip.
 *
 * `onProgress` is never called here (see `SUPPORTS_UPLOAD_PROGRESS`), and `signal` only
 * prevents a call that has not started yet (see `SUPPORTS_UPLOAD_CANCEL`).
 */
export async function uploadTransactionAttachment(
  target: AttachmentTarget,
  file: File,
  options?: AttachmentUploadOptions,
): Promise<TransactionAttachment> {
  if (options?.signal?.aborted) throw uploadAbortError();
  const body = await file.arrayBuffer();
  if (options?.signal?.aborted) throw uploadAbortError();
  return invokeRaw<TransactionAttachment>("upload_transaction_attachment", body, {
    "x-wf-target-kind": target.kind,
    "x-wf-target-id": encodeURIComponent(target.kind === "draft" ? target.token : target.id),
    "x-wf-content-type": file.type,
    "x-wf-filename": encodeURIComponent(file.name),
  });
}

export function deleteTransactionAttachment(target: AttachmentTarget, id: string): Promise<void> {
  return target.kind === "draft"
    ? invoke("delete_transaction_attachment_draft", {
        token: target.token,
        attachmentId: id,
      })
    : invoke("delete_transaction_attachment", { activityId: target.id, attachmentId: id });
}
