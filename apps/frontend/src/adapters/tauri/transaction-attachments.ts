import { invoke } from "@tauri-apps/api/core";
import type { TransactionAttachment } from "../shared/transaction-attachments";

function bytes(value: ArrayBuffer | number[]): Uint8Array<ArrayBuffer> {
  return value instanceof ArrayBuffer ? new Uint8Array(value) : Uint8Array.from(value);
}

export async function transactionAttachmentThumbnailUrl(activityId: string, id: string) {
  const value = await invoke<ArrayBuffer | number[]>("read_transaction_attachment", {
    activityId,
    attachmentId: id,
    preview: true,
  });
  return URL.createObjectURL(new Blob([bytes(value)], { type: "image/webp" }));
}

export async function openTransactionAttachment(
  activityId: string,
  attachment: TransactionAttachment,
) {
  const viewer = window.open("about:blank", "_blank");
  try {
    const value = await invoke<ArrayBuffer | number[]>("read_transaction_attachment", {
      activityId,
      attachmentId: attachment.id,
      preview: false,
    });
    const url = URL.createObjectURL(new Blob([bytes(value)], { type: attachment.contentType }));
    if (viewer) viewer.location.href = url;
    else window.open(url, "_blank", "noopener,noreferrer");
    window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
  } catch (error) {
    viewer?.close();
    throw error;
  }
}

export async function uploadTransactionAttachment(activityId: string, file: File) {
  return invoke<TransactionAttachment>("upload_transaction_attachment", {
    activityId,
    filename: file.name,
    contentType: file.type,
    bytes: Array.from(new Uint8Array(await file.arrayBuffer())),
  });
}

export function deleteTransactionAttachment(activityId: string, id: string): Promise<void> {
  return invoke("delete_transaction_attachment", { activityId, attachmentId: id });
}
