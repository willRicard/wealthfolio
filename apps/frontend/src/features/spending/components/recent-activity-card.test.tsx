import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";

import { RecentActivityCard } from "./recent-activity-card";
import type { CashActivity } from "../types/cash-activity";
import type { CategoryMetaMap } from "./category-chips";

function renderRecentActivityCard(
  activities: CashActivity[] = [],
  categoriesMeta: CategoryMetaMap = new Map(),
) {
  return render(
    <MemoryRouter>
      <RecentActivityCard activities={activities} categoriesMeta={categoriesMeta} currency="USD" />
    </MemoryRouter>,
  );
}

describe("RecentActivityCard", () => {
  it("shows the standard empty state without a setup link", () => {
    renderRecentActivityCard();

    expect(screen.getByText("No recent activity.")).toBeInTheDocument();
    expect(screen.queryByText("No spending accounts selected.")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: "Open spending settings →" }),
    ).not.toBeInTheDocument();
  });

  it("formats converted amounts in the application base currency", () => {
    renderRecentActivityCard([
      {
        id: "activity-1",
        accountId: "account-1",
        activityType: "WITHDRAWAL",
        activityDate: "2026-08-07T00:00:00.000Z",
        amount: "100",
        currency: "JPY",
        baseAmount: "0.67",
        baseCurrency: "USD",
        cashFlowBucket: "spending",
        assignments: [],
        splits: [],
        notes: "Lunch",
        status: "POSTED",
        isUserModified: false,
        needsReview: false,
        createdAt: "2026-08-07T00:00:00.000Z",
        updatedAt: "2026-08-07T00:00:00.000Z",
      },
    ]);

    expect(screen.getByText("$0.67")).toBeInTheDocument();
  });

  it("falls back to the native currency when conversion is unavailable", () => {
    renderRecentActivityCard([
      {
        id: "activity-1",
        accountId: "account-1",
        activityType: "WITHDRAWAL",
        activityDate: "2026-08-07T00:00:00.000Z",
        amount: "100",
        currency: "JPY",
        cashFlowBucket: "spending",
        assignments: [],
        splits: [],
        notes: "Lunch",
        status: "POSTED",
        isUserModified: false,
        needsReview: false,
        createdAt: "2026-08-07T00:00:00.000Z",
        updatedAt: "2026-08-07T00:00:00.000Z",
      },
    ]);

    expect(screen.getByText("¥100.00")).toBeInTheDocument();
  });

  it("uses the assignments included in the cash activity for its category badge", () => {
    const categoriesMeta: CategoryMetaMap = new Map([
      [
        "food",
        {
          name: "Food",
          color: "#22C55E",
          icon: null,
          parentId: null,
        },
      ],
    ]);

    renderRecentActivityCard(
      [
        {
          id: "activity-1",
          accountId: "account-1",
          activityType: "WITHDRAWAL",
          activityDate: "2026-08-07T00:00:00.000Z",
          amount: "100",
          currency: "JPY",
          baseAmount: "0.67",
          baseCurrency: "USD",
          cashFlowBucket: "spending",
          assignments: [
            {
              id: "assignment-1",
              activityId: "activity-1",
              taxonomyId: "spending_categories",
              categoryId: "food",
              weight: 1,
              source: "manual",
              createdAt: "2026-08-07T00:00:00.000Z",
              updatedAt: "2026-08-07T00:00:00.000Z",
            },
          ],
          splits: [],
          notes: "Lunch",
          status: "POSTED",
          isUserModified: false,
          needsReview: false,
          createdAt: "2026-08-07T00:00:00.000Z",
          updatedAt: "2026-08-07T00:00:00.000Z",
        },
      ],
      categoriesMeta,
    );

    expect(screen.getByText("Food")).toBeInTheDocument();
  });
});
