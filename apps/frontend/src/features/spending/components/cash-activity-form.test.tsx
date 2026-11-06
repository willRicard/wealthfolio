import { createInstance } from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import enCommon from "@/i18n/locales/en/common.json";
import enCommonTaxonomies from "@/i18n/locales/en/taxonomies.json";
import zhCommon from "@/i18n/locales/zh/common.json";
import zhCommonTaxonomies from "@/i18n/locales/zh/taxonomies.json";
import enSpending from "@/i18n/locales/en/spending.json";
import zhSpending from "@/i18n/locales/zh/spending.json";
import { act, fireEvent, render, screen, waitFor } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import {
  claimTransactionAttachmentDrafts,
  createActivity,
  getProcurementActivityPurchases,
  setProcurementActivityPurchases,
  discardTransactionAttachmentDrafts,
  listTransactionAttachments,
  reorderTransactionAttachments,
  updateActivity,
  uploadTransactionAttachment,
} from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { Activity } from "@/lib/types";
import { CashActivityForm } from "./cash-activity-form";

beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

async function chooseAccount(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  await user.click(screen.getByRole("combobox", { name: "Account" }));
  await user.click(await screen.findByRole("option", { name }));
}

vi.mock("@/adapters", () => ({
  createActivity: vi.fn(),
  getProcurementItems: vi.fn(() =>
    Promise.resolve([
      { id: "item-1", name: "Coffee", aliases: [], categoryId: "cat_groceries" },
      { id: "item-2", name: "Tea", aliases: [], categoryId: "cat_groceries" },
    ]),
  ),
  getProcurementActivityPurchases: vi.fn(() => Promise.resolve([])),
  setProcurementActivityPurchases: vi.fn(() => Promise.resolve([])),
  listTransactionAttachments: vi.fn(() => Promise.resolve([])),
  newAttachmentDraftToken: () => "draft-token",
  claimTransactionAttachmentDrafts: vi.fn(() => Promise.resolve(1)),
  discardTransactionAttachmentDrafts: vi.fn(() => Promise.resolve()),
  reorderTransactionAttachments: vi.fn(() => Promise.resolve()),
  uploadTransactionAttachment: vi.fn(() => Promise.resolve({ id: "uploaded" })),
  SUPPORTS_UPLOAD_PROGRESS: true,
  SUPPORTS_UPLOAD_CANCEL: true,
  deleteTransactionAttachment: vi.fn(() => Promise.resolve()),
  fetchTransactionAttachmentThumbnail: vi.fn(() =>
    Promise.resolve(new Blob([new Uint8Array([0])], { type: "image/webp" })),
  ),
  openTransactionAttachment: vi.fn(() => Promise.resolve()),
  updateActivity: vi.fn((payload: unknown) => Promise.resolve(payload)),
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));
// Each factory builds its value ONCE and hands back the same reference. The
// real hooks are React Query-backed and stable; a mock that returns a fresh
// array every render makes `spendingAccounts` a new identity each pass, which
// re-fires the reset effect that depends on it and never settles.
vi.mock("@/hooks/use-accounts", () => {
  const accounts = [
    {
      id: "card",
      name: "WS Visa Infinite",
      currency: "CAD",
      accountType: "CREDIT_CARD",
      isActive: true,
    },
    { id: "euro", name: "Spending Euro", currency: "EUR", accountType: "CASH", isActive: true },
    { id: "chequing", name: "WS Chequing", currency: "CAD", accountType: "CASH", isActive: true },
  ];
  return { useAccounts: () => ({ accounts }) };
});
vi.mock("@/hooks/use-settings", () => {
  const data = { baseCurrency: "CAD" };
  return { useSettings: () => ({ data }) };
});
const platformMock = vi.hoisted(() => ({ isMobileViewport: false }));
vi.mock("@/hooks/use-platform", () => ({
  useIsMobileViewport: () => platformMock.isMobileViewport,
}));
vi.mock("@/hooks/use-taxonomies", () => {
  const result = { data: null };
  return { useTaxonomy: vi.fn(() => result) };
});
vi.mock("../hooks/use-spending-settings", () => {
  const settings = { accountIds: ["card", "euro", "chequing"] };
  return { useSpendingSettings: () => ({ settings }) };
});
vi.mock("../hooks/use-spending-events", () => {
  const empty = { data: [] };
  return { useSpendingEvents: () => empty, useEventTypes: () => empty };
});
vi.mock("./event-dialog-provider", () => ({
  useEventDialog: () => ({ openEventDialog: vi.fn(), openEventTypeDialog: vi.fn() }),
}));

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

/** A charge made abroad: booked in USD on a card denominated in CAD. */
const foreignCharge = {
  id: "act-1",
  accountId: "card",
  activityType: "WITHDRAWAL",
  activityDate: "2026-07-13T12:00:00.000Z",
  amount: "9.59",
  currency: "USD",
  fxRate: "1.37",
  notes: "Sq *Morning Owl",
  detailedNotes: "Stored detailed notes",
} as unknown as Activity;

describe("CashActivityForm currency", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platformMock.isMobileViewport = false;
  });

  it("directs split category changes to the explicit split editor", () => {
    render(
      <CashActivityForm
        open
        onOpenChange={vi.fn()}
        activity={{ ...foreignCharge, splits: [{ id: "split-1" } as never] }}
      />,
      { wrapper },
    );
    expect(
      screen.getByText(
        "Split allocations are protected. Edit or explicitly replace them in the split editor.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pick a category" })).not.toBeInTheDocument();
  });

  it("keeps the neutral-transfer label for an existing transfer", () => {
    render(
      <CashActivityForm
        open
        onOpenChange={vi.fn()}
        activity={
          {
            ...foreignCharge,
            accountId: "chequing",
            activityType: "TRANSFER_IN",
            sourceGroupId: "linked-transfer",
            cashFlowBucket: "neutral",
          } as Activity
        }
      />,
      { wrapper },
    );
    expect(screen.getByText("Neutral transfer")).toBeInTheDocument();
  });

  it.each([null, "ADJUSTMENT", "BONUS"])(
    "labels an existing CREDIT/%s accurately and preserves its meaning on save",
    async (subtype) => {
      const user = userEvent.setup();
      render(
        <CashActivityForm
          open
          onOpenChange={vi.fn()}
          activity={
            {
              ...foreignCharge,
              accountId: "chequing",
              activityType: "CREDIT",
              subtype,
              cashFlowBucket: subtype === "BONUS" ? "income" : "neutral",
              metadata: { flow: { is_external: false } },
            } as Activity
          }
        />,
        { wrapper },
      );

      expect(screen.getByRole("combobox", { name: "Type" })).toHaveTextContent("Credit");
      expect(screen.queryByText("Reimbursement / refund")).not.toBeInTheDocument();
      expect(screen.queryByText("Neutral transfer")).not.toBeInTheDocument();
      if (subtype !== "BONUS") expect(screen.getByText("Neutral")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: /update/i }));
      await waitFor(() => expect(updateActivity).toHaveBeenCalled());
      expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
        activityType: "CREDIT",
        subtype,
        metadata: { flow: { is_external: false } },
      });
    },
  );

  /**
   * The currency used to be recomputed from the account on every save. Saving
   * an untouched foreign charge therefore rewrote it to the account's own
   * currency — the amount stayed 9.59 while its meaning changed from USD to
   * CAD, with nothing on screen to show it had happened.
   */
  it("keeps an existing activity's own currency when nothing was edited", async () => {
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });

    await user.click(await screen.findByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
      id: "act-1",
      currency: "USD",
    });
  });

  it("carries the activity's FX rate through an untouched save", async () => {
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });

    await user.click(await screen.findByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({ fxRate: 1.37 });
  });

  it("clears the FX rate when the activity currency changes", async () => {
    const user = userEvent.setup();
    render(
      <CashActivityForm
        open
        onOpenChange={vi.fn()}
        activity={{ ...foreignCharge, accountId: "euro" } as unknown as Activity}
      />,
      { wrapper },
    );

    await user.click(screen.getByTestId("advanced-options-button"));
    await user.click(screen.getByRole("button", { name: "CAD" }));
    await user.click(screen.getByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
      accountId: "euro",
      currency: "CAD",
      fxRate: null,
    });
  });

  /**
   * An FX rate converts the activity's currency into the ACCOUNT's. Moving the
   * activity to an account with a different denomination leaves the rate
   * describing a pair that no longer exists — and the field is collapsed by
   * default, so a USD->CAD rate would be silently reread as USD->EUR.
   */
  it("clears the FX rate when the activity moves to a differently-denominated account", async () => {
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });

    await chooseAccount(user, /Spending Euro/);
    await user.click(screen.getByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
      accountId: "euro",
      currency: "USD",
      fxRate: null,
    });
  });

  /** The opposite case: same denomination, so the rate still describes the pair. */
  it("keeps the FX rate when the new account has the same currency", async () => {
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });

    await chooseAccount(user, /WS Chequing/);
    await user.click(screen.getByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
      accountId: "chequing",
      currency: "USD",
      fxRate: 1.37,
    });
  });

  /**
   * The rate only describes a conversion when one happened. Keeping a stale one
   * on a same-currency row would have the server multiply by it.
   */
  it("drops the FX rate when the activity is in the account's own currency", async () => {
    const user = userEvent.setup();
    render(
      <CashActivityForm
        open
        onOpenChange={vi.fn()}
        activity={{ ...foreignCharge, currency: "CAD" } as unknown as Activity}
      />,
      { wrapper },
    );

    await user.click(await screen.findByRole("button", { name: /update/i }));

    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(vi.mocked(updateActivity).mock.calls[0][0]).toMatchObject({
      currency: "CAD",
      fxRate: null,
    });
  });
});

describe("CashActivityForm category display", () => {
  it.each([
    { parentName: "Food & Dining", name: "Groceries" },
    { parentName: "My food", name: "My groceries" },
  ])(
    "localizes only unchanged seed names ($name) and retains raw save inputs",
    async ({ parentName, name }) => {
      vi.clearAllMocks();
      const categories = [
        {
          id: "cat_food",
          taxonomyId: "spending_categories",
          name: parentName,
          parentId: null,
          color: "#123456",
        },
        {
          id: "cat_groceries",
          taxonomyId: "spending_categories",
          name,
          parentId: "cat_food",
          color: "#123456",
        },
      ];
      const original = structuredClone(categories);
      const data = { data: { categories } } as ReturnType<typeof useTaxonomy>;
      const empty = { data: null } as unknown as ReturnType<typeof useTaxonomy>;
      vi.mocked(useTaxonomy).mockImplementation((id) =>
        id === "spending_categories" ? data : empty,
      );
      const i18n = createInstance();
      await i18n.use(initReactI18next).init({
        lng: "zh",
        fallbackLng: "en",
        interpolation: { escapeValue: false },
        resources: {
          en: { common: enCommon, taxonomies: enCommonTaxonomies, spending: enSpending },
          zh: { common: zhCommon, taxonomies: zhCommonTaxonomies, spending: zhSpending },
        },
      });
      const activity = {
        ...foreignCharge,
        categoryTaxonomyId: "spending_categories",
        categoryId: "cat_groceries",
        categoryAssignmentId: "assignment-1",
      };
      const user = userEvent.setup();
      const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      render(
        <QueryClientProvider client={client}>
          <I18nextProvider i18n={i18n}>
            <CashActivityForm open onOpenChange={vi.fn()} activity={activity} />
          </I18nextProvider>
        </QueryClientProvider>,
      );
      const displayParent =
        parentName === "Food & Dining"
          ? zhCommonTaxonomies.categoryNames.spending_categories.cat_food
          : parentName;
      const displayName =
        name === "Groceries"
          ? zhCommonTaxonomies.categoryNames.spending_categories.cat_groceries
          : name;
      expect(screen.getByText(`${displayParent} / ${displayName}`)).toBeInTheDocument();
      expect(
        screen.getByRole("button", {
          name: i18n.t("spending:transactions.changeCategory", { name: displayName }),
        }),
      ).toBeInTheDocument();
      await act(() => i18n.changeLanguage("en"));
      expect(screen.getByText(`${parentName} / ${name}`)).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /update/i }));
      await waitFor(() => expect(updateActivity).toHaveBeenCalled());
      expect(vi.mocked(updateActivity).mock.lastCall?.[0]).toMatchObject({
        id: activity.id,
        amount: 9.59,
        currency: "USD",
        fxRate: 1.37,
      });
      expect(categories).toEqual(original);
      vi.mocked(useTaxonomy).mockReturnValue(empty);
    },
  );
});

vi.mock("@/features/spending/components/activity-split-review-provider", () => ({
  useActivitySplitReview: () => async (updates: unknown[]) => updates,
  isActivityEditCancelled: () => false,
}));

describe("CashActivityForm mobile steps", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platformMock.isMobileViewport = true;
  });

  it("moves to the details step without submitting the form", async () => {
    render(<CashActivityForm open onOpenChange={vi.fn()} />, { wrapper });

    const nextButton = await screen.findByRole("button", { name: /next/i });
    expect(fireEvent.click(nextButton)).toBe(false);

    expect(await screen.findByRole("button", { name: /create transaction/i })).toBeInTheDocument();
    expect(createActivity).not.toHaveBeenCalled();
  });
});

describe("CashActivityForm detailed notes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platformMock.isMobileViewport = false;
  });
  it("loads and saves detailed notes with the transaction payload", async () => {
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });
    const notes = await screen.findByLabelText("Detailed notes");
    await waitFor(() => expect(notes).toHaveValue("Stored detailed notes"));
    await user.clear(notes);
    await user.type(notes, "Receipt line items");
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(listTransactionAttachments).toHaveBeenCalledWith({ kind: "activity", id: "act-1" });
    expect(vi.mocked(updateActivity).mock.calls[0][0].detailedNotes).toBe("Receipt line items");
    expect(vi.mocked(updateActivity).mock.calls[0][0].comment).toBe("Sq *Morning Owl");
    expect(vi.mocked(updateActivity).mock.calls[0][0].metadata).toBeUndefined();
  });
});

it("uploads on select in both modes, staging a new transaction's files under a draft token", async () => {
  vi.clearAllMocks();
  platformMock.isMobileViewport = false;
  const receipt = () => new File(["receipt"], "receipt.png", { type: "image/png" });

  const editing = render(
    <CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />,
    { wrapper },
  );
  const picker = await screen.findByLabelText("Attachments", { exact: true });
  await waitFor(() => expect(picker).toBeEnabled());
  await userEvent.setup().upload(picker, receipt());
  await waitFor(() =>
    expect(uploadTransactionAttachment).toHaveBeenCalledWith(
      { kind: "activity", id: "act-1" },
      expect.any(File),
      expect.anything(),
    ),
  );
  expect(updateActivity).not.toHaveBeenCalled();
  expect(claimTransactionAttachmentDrafts).not.toHaveBeenCalled();
  editing.unmount();

  vi.clearAllMocks();
  vi.mocked(createActivity).mockResolvedValue({ ...foreignCharge, id: "new-1" });
  const user = userEvent.setup();
  render(<CashActivityForm open onOpenChange={vi.fn()} />, { wrapper });
  const creating = await screen.findByLabelText("Attachments", { exact: true });
  await waitFor(() => expect(creating).toBeEnabled());
  await user.upload(creating, receipt());

  await waitFor(() =>
    expect(uploadTransactionAttachment).toHaveBeenCalledWith(
      { kind: "draft", token: "draft-token" },
      expect.any(File),
      expect.anything(),
    ),
  );
  expect(createActivity).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: /^create$/i }));
  await waitFor(() =>
    expect(claimTransactionAttachmentDrafts).toHaveBeenCalledWith("draft-token", "new-1"),
  );
  expect(uploadTransactionAttachment).toHaveBeenCalledTimes(1);
});

it("sends the arranged attachment order on save, and only once it was arranged", async () => {
  const stored = (id: string, filename: string) => ({
    id,
    activityId: "act-1",
    filename,
    contentType: "image/png",
    sizeBytes: 10,
    createdAt: "2026-10-13",
  });
  const listed = [stored("file-1", "first.png"), stored("file-2", "second.png")];
  vi.clearAllMocks();
  platformMock.isMobileViewport = false;
  vi.mocked(listTransactionAttachments).mockResolvedValue(listed);
  const user = userEvent.setup();

  const untouched = render(
    <CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />,
    { wrapper },
  );
  await screen.findByRole("button", { name: "Open original: first.png" });
  await user.click(screen.getByRole("button", { name: /update/i }));
  await waitFor(() => expect(updateActivity).toHaveBeenCalled());
  expect(reorderTransactionAttachments).not.toHaveBeenCalled();
  untouched.unmount();

  vi.clearAllMocks();
  vi.mocked(listTransactionAttachments).mockResolvedValue(listed);
  render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });
  await screen.findByRole("button", { name: "Open original: first.png" });
  await user.click(screen.getByRole("button", { name: "Move second.png left" }));
  await user.click(screen.getByRole("button", { name: /update/i }));

  await waitFor(() =>
    expect(reorderTransactionAttachments).toHaveBeenCalledWith("act-1", ["file-2", "file-1"]),
  );
});

it("discards staged files when the sheet closes unsaved, but not after a claim", async () => {
  vi.clearAllMocks();
  platformMock.isMobileViewport = false;
  vi.mocked(createActivity).mockResolvedValue({ ...foreignCharge, id: "new-2" });
  const user = userEvent.setup();

  const abandoned = render(<CashActivityForm open onOpenChange={vi.fn()} />, { wrapper });
  await user.click(await screen.findByRole("button", { name: /cancel/i }));
  expect(discardTransactionAttachmentDrafts).toHaveBeenCalledWith("draft-token");
  abandoned.unmount();

  vi.clearAllMocks();
  render(<CashActivityForm open onOpenChange={vi.fn()} />, { wrapper });
  await user.click(await screen.findByRole("button", { name: /^create$/i }));
  await waitFor(() => expect(claimTransactionAttachmentDrafts).toHaveBeenCalled());
  // The files belong to the transaction now; discarding would delete them.
  expect(discardTransactionAttachmentDrafts).not.toHaveBeenCalled();
});

it("retries a failed claim against the saved draft without creating another transaction", async () => {
  vi.clearAllMocks();
  platformMock.isMobileViewport = false;
  vi.mocked(createActivity).mockResolvedValue({ ...foreignCharge, id: "saved-draft" });
  vi.mocked(claimTransactionAttachmentDrafts).mockRejectedValueOnce(
    new Error("Maximum 10 attachments per transaction"),
  );
  const user = userEvent.setup();
  const onOpenChange = vi.fn();
  render(<CashActivityForm open onOpenChange={onOpenChange} />, { wrapper });
  const picker = await screen.findByLabelText("Attachments", { exact: true });
  await waitFor(() => expect(picker).toBeEnabled());
  await user.upload(picker, new File(["receipt"], "receipt.png", { type: "image/png" }));
  await waitFor(() => expect(uploadTransactionAttachment).toHaveBeenCalledTimes(1));

  const submit = screen.getByRole("button", { name: /^create$/i });
  await user.click(submit);
  await waitFor(() => expect(claimTransactionAttachmentDrafts).toHaveBeenCalledTimes(1));
  expect(onOpenChange).not.toHaveBeenCalledWith(false);

  await waitFor(() => expect(submit).toBeEnabled());
  await user.click(submit);
  await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  expect(createActivity).toHaveBeenCalledTimes(1);
  expect(updateActivity).toHaveBeenCalledWith(expect.objectContaining({ id: "saved-draft" }));
  expect(claimTransactionAttachmentDrafts).toHaveBeenCalledTimes(2);
  expect(uploadTransactionAttachment).toHaveBeenCalledTimes(1);
});

describe("CashActivityForm purchases", () => {
  const stored = {
    itemId: "item-1",
    activityId: "act-1",
    quantity: "2",
    subtotal: "3.50",
    createdAt: "2026-10-01",
    updatedAt: "2026-10-01",
  };
  beforeEach(() => {
    vi.clearAllMocks();
    platformMock.isMobileViewport = false;
    vi.mocked(getProcurementActivityPurchases).mockResolvedValue([stored]);
    vi.mocked(setProcurementActivityPurchases).mockResolvedValue([]);
  });

  function cachedForm() {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData([QueryKeys.PROCUREMENT_ACTIVITY_PURCHASES, foreignCharge.id], [stored]);
    const onOpenChange = vi.fn();
    const view = (open: boolean) => (
      <QueryClientProvider client={client}>
        <CashActivityForm open={open} onOpenChange={onOpenChange} activity={foreignCharge} />
      </QueryClientProvider>
    );
    return { client, onOpenChange, view, ...render(view(true)) };
  }

  it("hydrates fresh cached rows on mount and reopening, without writing untouched rows", async () => {
    const user = userEvent.setup();
    const { rerender, view } = cachedForm();
    const quantity = await screen.findByLabelText("Qty 1");
    expect(quantity).toHaveValue("2");
    await user.clear(quantity);
    await user.type(quantity, "4");
    rerender(view(false));
    rerender(view(true));
    expect(await screen.findByLabelText("Qty 1")).toHaveValue("2");
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(setProcurementActivityPurchases).not.toHaveBeenCalled();
  });

  it("disables purchase changes until existing purchases load", async () => {
    let resolve!: (value: (typeof stored)[]) => void;
    vi.mocked(getProcurementActivityPurchases).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });
    expect(screen.getByRole("button", { name: "Add item" })).toBeDisabled();
    act(() => resolve([stored]));
    expect(await screen.findByLabelText("Qty 1")).toHaveValue("2");
    expect(screen.getByRole("button", { name: "Add item" })).toBeEnabled();
  });

  it("shows load failure and allows unrelated activity edits without replacing purchases", async () => {
    vi.mocked(getProcurementActivityPurchases).mockRejectedValueOnce(new Error("offline"));
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });
    await waitFor(() => expect(screen.getByRole("alert")).toBeVisible());
    expect(screen.getByRole("button", { name: "Add item" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(updateActivity).toHaveBeenCalled());
    expect(setProcurementActivityPurchases).not.toHaveBeenCalled();
  });

  it("loads purchases after retrying a failed read", async () => {
    vi.mocked(getProcurementActivityPurchases).mockRejectedValueOnce(new Error("offline"));
    const user = userEvent.setup();
    render(<CashActivityForm open onOpenChange={vi.fn()} activity={foreignCharge} />, { wrapper });
    await user.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByLabelText("Qty 1")).toHaveValue("2");
    expect(screen.getByRole("button", { name: "Add item" })).toBeEnabled();
  });

  it("preserves edited rows when query data refreshes and saves the full list", async () => {
    const user = userEvent.setup();
    const { client } = cachedForm();
    const quantity = await screen.findByLabelText("Qty 1");
    await user.clear(quantity);
    await user.type(quantity, "4");
    act(() => {
      client.setQueryData(
        [QueryKeys.PROCUREMENT_ACTIVITY_PURCHASES, foreignCharge.id],
        [{ ...stored, quantity: "9" }],
      );
    });
    expect(quantity).toHaveValue("4");
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() =>
      expect(setProcurementActivityPurchases).toHaveBeenCalledWith("act-1", [
        { activityId: "act-1", itemId: "item-1", quantity: "4", subtotal: "3.5" },
      ]),
    );
  });

  it("blocks a dirty replacement after a refresh fails, then preserves edits through retry", async () => {
    const user = userEvent.setup();
    const { client } = cachedForm();
    const quantity = await screen.findByLabelText("Qty 1");
    await user.clear(quantity);
    await user.type(quantity, "4");
    vi.mocked(getProcurementActivityPurchases).mockRejectedValueOnce(new Error("offline"));
    await act(() =>
      client.invalidateQueries({
        queryKey: [QueryKeys.PROCUREMENT_ACTIVITY_PURCHASES, foreignCharge.id],
      }),
    );
    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: /update/i }));
    expect(updateActivity).not.toHaveBeenCalled();
    expect(setProcurementActivityPurchases).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(quantity).toBeEnabled());
    expect(quantity).toHaveValue("4");
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() =>
      expect(setProcurementActivityPurchases).toHaveBeenCalledWith("act-1", [
        { activityId: "act-1", itemId: "item-1", quantity: "4", subtotal: "3.5" },
      ]),
    );
  });

  it("clears all purchases after deleting the last row", async () => {
    const user = userEvent.setup();
    cachedForm();
    await user.click(await screen.findByRole("button", { name: "Delete 1" }));
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(setProcurementActivityPurchases).toHaveBeenCalledWith("act-1", []));
  });

  it("blocks incomplete rows and retains edits after a failed replacement", async () => {
    const user = userEvent.setup();
    const { onOpenChange } = cachedForm();
    const price = await screen.findByLabelText("Subtotal 1");
    await user.clear(price);
    await user.click(screen.getByRole("button", { name: /update/i }));
    expect(updateActivity).not.toHaveBeenCalled();
    await user.type(price, "5");
    vi.mocked(setProcurementActivityPurchases).mockRejectedValueOnce(new Error("failed"));
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(setProcurementActivityPurchases).toHaveBeenCalledTimes(1));
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(price).toHaveValue("5");
    await waitFor(() => expect(screen.getByRole("button", { name: /update/i })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: /update/i }));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(setProcurementActivityPurchases).toHaveBeenCalledTimes(2);
  });
});
