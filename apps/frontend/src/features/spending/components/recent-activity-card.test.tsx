import { render, screen } from "@/test/render";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { FormattingProvider } from "@wealthfolio/ui";
import { RecentActivityCard } from "./recent-activity-card";
import type { CashActivity } from "../types/cash-activity";
import type { CategoryMetaMap } from "./category-chips";

function renderRecentActivityCard(
  activities: CashActivity[] = [],
  categoriesMeta: CategoryMetaMap = new Map(),
  timezone?: string,
) {
  return render(
    <MemoryRouter>
      <FormattingProvider locale="en-US" timezone={timezone}>
        <RecentActivityCard activities={activities} categoriesMeta={categoriesMeta} />
      </FormattingProvider>
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
        netAmount: -100,
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
        netAmount: -100,
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

    expect(screen.getByText("¥100")).toBeInTheDocument();
  });

  it("groups activity dates in the configured timezone", () => {
    renderRecentActivityCard(
      [
        {
          id: "activity-1",
          accountId: "account-1",
          activityType: "WITHDRAWAL",
          activityDate: "2020-08-07T02:00:00.000Z",
          amount: "10",
          currency: "USD",
          cashFlowBucket: "spending",
          netAmount: -10,
          assignments: [],
          splits: [],
          notes: "Late dinner",
          status: "POSTED",
          isUserModified: false,
          needsReview: false,
          createdAt: "2020-08-07T02:00:00.000Z",
          updatedAt: "2020-08-07T02:00:00.000Z",
        },
      ],
      new Map(),
      "America/Toronto",
    );

    expect(screen.getByText("Thu, Aug 6")).toBeInTheDocument();
    expect(screen.queryByText("Fri, Aug 7")).not.toBeInTheDocument();
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
          netAmount: -100,
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

  it("labels each activity with its own currency, not the base currency", () => {
    const activity: CashActivity = {
      id: "1",
      accountId: "acc-1",
      activityDate: new Date().toISOString().slice(0, 10),
      activityType: "WITHDRAWAL",
      amount: "100",
      currency: "EUR",
      notes: "Coffee",
      cashFlowBucket: "spending",
      assignments: [],
      splits: [],
      netAmount: -100,
      status: "POSTED",
      isUserModified: false,
      needsReview: false,
      createdAt: "2026-08-07T00:00:00.000Z",
      updatedAt: "2026-08-07T00:00:00.000Z",
    };

    renderRecentActivityCard([activity]);

    const row = screen.getByText("Coffee").closest("a");
    expect(row).toBeInTheDocument();
    expect(row?.textContent).toContain("€");
    expect(row?.textContent).not.toContain("$");
  });
});
