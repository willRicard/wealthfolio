import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@/test/render";
import NotificationsPage from "./notifications-page";

const state = vi.hoisted(() => ({
  preferences: undefined as { transactions: boolean; budgets: boolean } | undefined,
  preferencesError: false,
  reloadPreferences: vi.fn(),
  setPreference: vi.fn(),
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("../settings-header", () => ({ SettingsHeader: () => null }));
vi.mock("@/features/push-notifications/use-push-notifications", () => ({
  usePushNotifications: () => ({
    ...state,
    supported: true,
    serverEnabled: true,
    subscribed: true,
    permission: "granted",
    loading: false,
    enable: { isPending: false },
    disable: { isPending: false },
    savePreferences: { isPending: false },
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  state.preferences = undefined;
  state.preferencesError = false;
});

it.each(["ltr", "rtl"])("shows saved preferences without a false off state (%s)", (dir) => {
  const view = render(
    <div dir={dir}>
      <NotificationsPage />
    </div>,
  );
  expect(screen.queryAllByRole("switch")).toHaveLength(0);
  expect(screen.getAllByText("common:loading")).toHaveLength(2);
  state.preferences = { transactions: true, budgets: true };
  view.rerender(
    <div dir={dir}>
      <NotificationsPage />
    </div>,
  );
  for (const toggle of screen.getAllByRole("switch")) {
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(toggle).not.toBeDisabled();
  }
  state.preferences = { transactions: false, budgets: true };
  view.rerender(
    <div dir={dir}>
      <NotificationsPage />
    </div>,
  );
  expect(
    screen.getByRole("switch", { name: "settings:notifications.transactions_label" }),
  ).toHaveAttribute("aria-checked", "false");
  expect(screen.queryByText("settings:notifications.test_action")).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("switch", { name: "settings:notifications.transactions_label" }),
  );
  expect(state.setPreference).toHaveBeenCalledWith("transactions", true);
});

it("offers retry instead of implying preferences are off when loading fails", () => {
  state.preferencesError = true;
  render(<NotificationsPage />);
  expect(screen.queryAllByRole("switch")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "common:retry" }));
  expect(state.reloadPreferences).toHaveBeenCalled();
});
