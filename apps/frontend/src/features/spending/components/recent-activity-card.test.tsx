import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { RecentActivityCard } from "./recent-activity-card";
import type { CashActivity } from "../types/cash-activity";

vi.mock("@tanstack/react-query", () => ({
  useQueries: vi.fn(() => []),
}));

function renderRecentActivityCard(activities: CashActivity[] = []) {
  return render(
    <MemoryRouter>
      <RecentActivityCard activities={activities} categoriesMeta={new Map()} currency="USD" />
    </MemoryRouter>,
  );
}

describe("RecentActivityCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

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
});
