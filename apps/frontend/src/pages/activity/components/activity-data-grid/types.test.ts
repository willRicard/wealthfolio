import { describe, expect, it } from "vitest";

import type { ActivityDetails } from "@/lib/types";

import { toLocalTransaction } from "./types";

function activityWithBoundary(isExternal?: boolean): ActivityDetails {
  return {
    id: "activity-1",
    accountId: "account-1",
    accountName: "Cash",
    accountCurrency: "USD",
    activityType: "CREDIT",
    subtype: "REIMBURSEMENT",
    date: new Date("2026-08-12T12:00:00Z"),
    quantity: null,
    unitPrice: null,
    amount: "100",
    fee: null,
    currency: "USD",
    needsReview: false,
    createdAt: new Date("2026-08-12T12:00:00Z"),
    updatedAt: new Date("2026-08-12T12:00:00Z"),
    assetId: "",
    assetSymbol: "",
    metadata: typeof isExternal === "boolean" ? { flow: { is_external: isExternal } } : undefined,
  };
}

describe("toLocalTransaction", () => {
  it.each([true, false])("preserves an explicit %s boundary", (isExternal) => {
    expect(toLocalTransaction(activityWithBoundary(isExternal)).isExternal).toBe(isExternal);
  });

  it("keeps a missing boundary distinct from explicit false", () => {
    expect(toLocalTransaction(activityWithBoundary()).isExternal).toBeUndefined();
  });
});
