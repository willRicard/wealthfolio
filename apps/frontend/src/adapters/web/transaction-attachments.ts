import { profileFetch, profileScope } from "@/features/profiles/session";
import type { TransactionAttachment } from "../shared/transaction-attachments";
import { API_PREFIX } from "./core";
const path = (activityId: string) =>
  `${API_PREFIX}/spending/transactions/${encodeURIComponent(activityId)}/attachments`;
function transactionAttachmentUrl(activityId: string, id: string, thumbnail = false) {
  return `${path(activityId)}/${encodeURIComponent(id)}${thumbnail ? "/thumbnail" : ""}?profileScope=${encodeURIComponent(profileScope())}`;
}
export async function transactionAttachmentThumbnailUrl(activityId: string, id: string) {
  return transactionAttachmentUrl(activityId, id, true);
}
export async function openTransactionAttachment(
  activityId: string,
  attachment: TransactionAttachment,
) {
  window.open(transactionAttachmentUrl(activityId, attachment.id), "_blank", "noopener,noreferrer");
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
export async function uploadTransactionAttachment(activityId: string, file: File) {
  const body = new FormData();
  body.append("file", file);
  const response = await checked(await profileFetch(path(activityId), { method: "POST", body }));
  return (await response.json()) as TransactionAttachment;
}
export async function deleteTransactionAttachment(activityId: string, id: string) {
  await checked(
    await profileFetch(`${path(activityId)}/${encodeURIComponent(id)}`, { method: "DELETE" }),
  );
}
