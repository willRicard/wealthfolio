export const PROCUREMENT_TAX_TYPES = ["INCLUDED", "EXCLUDED_8", "EXCLUDED_10", "EXEMPT"] as const;

export type ProcurementTaxType = (typeof PROCUREMENT_TAX_TYPES)[number];

export const DEFAULT_PROCUREMENT_TAX_TYPE: ProcurementTaxType = "INCLUDED";

export const PROCUREMENT_TAX_TYPE_LABELS: Record<ProcurementTaxType, string> = {
  INCLUDED: "taxIncluded",
  EXCLUDED_8: "taxExcluded8",
  EXCLUDED_10: "taxExcluded10",
  EXEMPT: "taxExempt",
};
