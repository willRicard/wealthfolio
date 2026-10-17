import type {
  ProcurementItem,
  ProcurementItemWithStats,
  SaveProcurementItem,
  ProcurementItemPrices,
  ProcurementPurchase,
  SaveProcurementPurchase,
} from "@/features/procurement/types";
import { invoke } from "./platform";

export const getProcurementItems = (): Promise<ProcurementItemWithStats[]> =>
  invoke<ProcurementItemWithStats[]>("get_procurement_items");

export const getProcurementItem = (id: string): Promise<ProcurementItem> =>
  invoke<ProcurementItem>("get_procurement_item", { id });

export const createProcurementItem = (input: SaveProcurementItem): Promise<ProcurementItem> =>
  invoke<ProcurementItem>("create_procurement_item", { input });

export const updateProcurementItem = (
  id: string,
  input: SaveProcurementItem,
): Promise<ProcurementItem> => invoke<ProcurementItem>("update_procurement_item", { id, input });

export const cloneProcurementItem = (id: string): Promise<ProcurementItem> =>
  invoke<ProcurementItem>("clone_procurement_item", { id });

export const deleteProcurementItem = (id: string): Promise<void> =>
  invoke<void>("delete_procurement_item", { id });

export const getProcurementItemPrices = (id: string): Promise<ProcurementItemPrices> =>
  invoke<ProcurementItemPrices>("get_procurement_item_prices", { id });

export const getProcurementActivityPurchases = (
  activityId: string,
): Promise<ProcurementPurchase[]> =>
  invoke<ProcurementPurchase[]>("get_procurement_activity_purchases", { activityId });

export const setProcurementActivityPurchases = (
  activityId: string,
  purchases: SaveProcurementPurchase[],
): Promise<ProcurementPurchase[]> =>
  invoke<ProcurementPurchase[]>("set_procurement_activity_purchases", { activityId, purchases });

export const discardProcurementPhotoDraft = (id: string): Promise<void> =>
  invoke<void>("discard_procurement_photo_draft", { id });
