import { profileFetch, profileScope } from "@/features/profiles/session";
import type {
  AttachmentTarget,
  AttachmentUploadOptions,
  TransactionAttachment,
} from "../shared/transaction-attachments";
import { API_PREFIX } from "./core";
import { postFileWithProgress } from "./upload";

/** `XMLHttpRequest` measures request-body progress and can abort it. */
export const SUPPORTS_UPLOAD_PROGRESS = true;
export const SUPPORTS_UPLOAD_CANCEL = true;

/**
 * The collection a target's files live in. Everything below composes onto it —
 * `/{id}` and `/{id}/thumbnail` — so the two kinds differ in one place only.
 */
const path = (target: AttachmentTarget) =>
  target.kind === "draft"
    ? `${API_PREFIX}/spending/attachment-drafts/${encodeURIComponent(target.token)}`
    : `${API_PREFIX}/spending/transactions/${encodeURIComponent(target.id)}/attachments`;

function transactionAttachmentUrl(target: AttachmentTarget, id: string, thumbnail = false) {
  // `window.open` and `img` cannot carry headers, so the scope travels in the query.
  return `${path(target)}/${encodeURIComponent(id)}${thumbnail ? "/thumbnail" : ""}?profileScope=${encodeURIComponent(profileScope())}`;
}

export async function fetchTransactionAttachmentThumbnail(
  target: AttachmentTarget,
  id: string,
): Promise<Blob> {
  const response = await checked(await profileFetch(transactionAttachmentUrl(target, id, true)));
  return response.blob();
}

export async function openTransactionAttachment(
  target: AttachmentTarget,
  attachment: TransactionAttachment,
) {
  window.open(transactionAttachmentUrl(target, attachment.id), "_blank", "noopener,noreferrer");
}

async function checked(response: Response) {
  if (!response.ok) {
    const body: unknown = await response.json().catch(() => ({}));
    throw new Error(
      typeof body === "object" &&
        body !== null &&
        "message" in body &&
        typeof body.message === "string"
        ? body.message
        : "Attachment request failed",
    );
  }
  return response;
}

export async function uploadTransactionAttachment(
  target: AttachmentTarget,
  file: File,
  options?: AttachmentUploadOptions,
) {
  return postFileWithProgress<TransactionAttachment>(path(target), file, options);
}

export async function deleteTransactionAttachment(target: AttachmentTarget, id: string) {
  await checked(
    await profileFetch(`${path(target)}/${encodeURIComponent(id)}`, { method: "DELETE" }),
  );
}
