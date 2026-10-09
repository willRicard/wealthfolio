import { invoke } from "./platform";

export interface TransactionAttachment {
  id: string;
  activityId: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
  createdAt: string;
}
export const getTransactionAttachments = (activityId: string) =>
  invoke<TransactionAttachment[]>("get_transaction_attachments", { activityId });
