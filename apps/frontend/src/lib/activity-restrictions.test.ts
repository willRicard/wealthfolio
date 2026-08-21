import { describe, expect, it } from "vitest";

import type { Account } from "@/lib/types";
import { canAddHoldings } from "./activity-restrictions";

function holdingsAccount(providerAccountId?: string): Account {
  return {
    id: "account",
    name: "Account",
    accountType: "SECURITIES",
    balance: 0,
    currency: "USD",
    isDefault: false,
    isActive: true,
    isArchived: false,
    trackingMode: "HOLDINGS",
    createdAt: new Date("2026-08-05T00:00:00Z"),
    updatedAt: new Date("2026-08-05T00:00:00Z"),
    providerAccountId,
  };
}

describe("canAddHoldings", () => {
  it("allows manual holdings accounts", () => {
    expect(canAddHoldings(holdingsAccount())).toBe(true);
  });

  it("keeps connected holdings read-only outside remediation", () => {
    expect(canAddHoldings(holdingsAccount("provider-account"))).toBe(false);
  });
});
