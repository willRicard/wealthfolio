import { invoke } from "./platform";

export interface TransactionAttachment {
  id: string;
  /** Set once the file belongs to a saved transaction. */
  activityId?: string;
  /** Set while the file is still staged under a draft token. */
  token?: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
  createdAt: string;
}

/**
 * What an attachment is stored against: a saved transaction, or a draft token for
 * one the user is still composing. Every attachment call takes a target, so the
 * two cases differ only in the route or command the adapter picks.
 */
export type AttachmentTarget = { kind: "activity"; id: string } | { kind: "draft"; token: string };

export interface AttachmentUploadOptions {
  /** Fraction in 0..1. Only called where the transport can measure progress. */
  onProgress?: (fraction: number) => void;
  signal?: AbortSignal;
}

/**
 * The rejection both upload transports use when a signal aborts, so callers can branch
 * on `error.name === "AbortError"` the way they would for a cancelled `fetch`.
 */
export const uploadAbortError = () =>
  Object.assign(new Error("Upload aborted"), { name: "AbortError" });

export const getTransactionAttachments = (activityId: string) =>
  invoke<TransactionAttachment[]>("get_transaction_attachments", { activityId });

export const listTransactionAttachments = (target: AttachmentTarget) =>
  target.kind === "draft"
    ? invoke<TransactionAttachment[]>("get_transaction_attachment_drafts", { token: target.token })
    : getTransactionAttachments(target.id);

/**
 * Identifies one unsaved transaction's staged files.
 *
 * Generated on the client and validated as a UUID server-side. It is not a
 * capability on its own — the per-profile database and blob root are the boundary —
 * but it does travel in a URL path, so it reaches access logs and browser history.
 */
export const newAttachmentDraftToken = () => crypto.randomUUID();

/**
 * Drops everything staged under a token. Called when a transaction sheet closes
 * unsaved; fire-and-forget, because the sweep is what guarantees cleanup.
 */
export const discardTransactionAttachmentDrafts = (token: string) =>
  invoke<void>("discard_transaction_attachment_drafts", { token });

/** Hands the staged files to a saved transaction. Returns how many moved. */
export const claimTransactionAttachmentDrafts = (token: string, activityId: string) =>
  invoke<number>("claim_transaction_attachment_drafts", { token, activityId });

/**
 * Stores the order the user arranged the tiles in `attachmentIds`.
 */
export const reorderTransactionAttachments = (activityId: string, attachmentIds: string[]) =>
  invoke<void>("reorder_transaction_attachments", { activityId, attachmentIds });
