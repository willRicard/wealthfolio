import { fireEvent, render, screen, waitFor } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import {
  claimTransactionAttachmentDrafts,
  createActivity,
  discardTransactionAttachmentDrafts,
  listTransactionAttachments,
  updateActivity,
  uploadTransactionAttachment,
} from "@/adapters";
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

vi.mock("@/adapters", () => ({
  createActivity: vi.fn(),
  listTransactionAttachments: vi.fn(() => Promise.resolve([])),
  newAttachmentDraftToken: () => "draft-token",
  claimTransactionAttachmentDrafts: vi.fn(() => Promise.resolve(1)),
  discardTransactionAttachmentDrafts: vi.fn(() => Promise.resolve()),
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
  return { useTaxonomy: () => result };
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

    // Radix Select cannot be opened under jsdom, but it mirrors its value into a
    // hidden native select for form compatibility; driving that is equivalent to
    // picking the option.
    const accountSelect = [...document.querySelectorAll("select")].find((element) =>
      [...element.options].some((option) => option.value === "euro"),
    );
    if (!accountSelect) throw new Error("account select not found");
    fireEvent.change(accountSelect, { target: { value: "euro" } });
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

    const accountSelect = [...document.querySelectorAll("select")].find((element) =>
      [...element.options].some((option) => option.value === "chequing"),
    );
    if (!accountSelect) throw new Error("account select not found");
    fireEvent.change(accountSelect, { target: { value: "chequing" } });
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
