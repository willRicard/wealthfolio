export interface ItemPhoto {
  backend: string;
  key: string;
  previewKey: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}
export interface ProcurementItem {
  id: string;
  name: string;
  brand: string | null;
  memo: string | null;
  categoryId: string;
  subcategoryId: string | null;
  rating: number | null;
  unit: string;
  taxType: string;
  aliases: string[];
  photo: ItemPhoto | null;
  createdAt: string;
  updatedAt: string;
}
export interface SaveProcurementItem {
  name: string;
  brand: string | null;
  memo: string | null;
  categoryId: string;
  subcategoryId: string | null;
  rating: number | null;
  unit: string;
  taxType: string;
  aliases: string[];
  draftId?: string | null;
}
export interface ProcurementPriceStats {
  baseCurrency: string;
  minPrice: string | null;
  avgPrice: string | null;
  latestPrice: string | null;
  latestDate: string | null;
  purchaseCount: number;
  unconvertedCount: number;
}
export interface ProcurementItemWithStats extends ProcurementItem, ProcurementPriceStats {}
export interface ProcurementPrice {
  itemId: string;
  activityId: string;
  date: string;
  shop: string | null;
  quantity: string;
  unitPrice: string;
  currency: string;
  unitPriceBase: string | null;
  baseCurrency: string;
}
export interface ProcurementItemPrices {
  prices: ProcurementPrice[];
  stats: ProcurementPriceStats;
}
export interface SaveProcurementPurchase {
  itemId: string;
  activityId: string;
  /** Expressed in the item's unit. */
  quantity: string;
  subtotal: string;
}
export interface ProcurementPurchase extends SaveProcurementPurchase {
  createdAt: string;
  updatedAt: string;
}
export interface ProcurementPhotoDraft {
  id: string;
  photo: ItemPhoto;
  createdAt: string;
}
