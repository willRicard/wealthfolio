import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LoginPage } from "@/pages/auth/login-page";
import { AuthGate, AuthProvider } from "./auth-context";

vi.mock("@/lib/reload-application", () => ({ reloadApplication: vi.fn() }));
vi.mock("@/adapters", () => ({ isWeb: true, isTauri: false }));
vi.mock("@/features/profiles/session", () => ({
  revokeProfileSession: vi.fn(),
  hasProfileSession: vi.fn(() => false),
}));

const startAuthentication = vi.fn();
const browserSupportsWebAuthnAutofill = vi.fn();
const cancelCeremony = vi.fn();

vi.mock("@simplewebauthn/browser", () => ({
  startAuthentication: (...args: unknown[]) => startAuthentication(...args),
  startRegistration: vi.fn(),
  browserSupportsWebAuthn: () => true,
  browserSupportsWebAuthnAutofill: () => browserSupportsWebAuthnAutofill(),
  WebAuthnAbortService: {
    cancelCeremony: () => cancelCeremony(),
    createNewAbortSignal: () => new AbortController().signal,
  },
}));

const ASSERTION = { id: "cred", rawId: "cred", type: "public-key", response: {} };

function routeFetch(
  overrides: Partial<Record<"status" | "me" | "start" | "finish", () => Response | Promise<never>>>,
  statusBody: Record<string, unknown> = {},
) {
  return vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/auth/status")) {
      return Promise.resolve(
        overrides.status?.() ??
          Response.json({ requiresPassword: true, oidcEnabled: false, ...statusBody }),
      );
    }
    if (url.endsWith("/auth/me")) {
      return Promise.resolve(overrides.me?.() ?? new Response(null, { status: 401 }));
    }
    if (url.endsWith("/auth/passkey/start")) {
      return Promise.resolve(
        overrides.start?.() ?? Response.json({ publicKey: { challenge: "abc" } }),
      );
    }
    if (url.endsWith("/auth/passkey/finish")) {
      return Promise.resolve(
        overrides.finish?.() ?? Response.json({ authenticated: true, expiresIn: 3600 }),
      );
    }
    return Promise.resolve(new Response(null, { status: 404 }));
  });
}

function mount(fetchImpl: ReturnType<typeof routeFetch>) {
  vi.stubGlobal("fetch", fetchImpl);
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <AuthProvider>
        <AuthGate fallback={<LoginPage />}>
          <div>Private portfolio</div>
        </AuthGate>
      </AuthProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  startAuthentication.mockReset().mockReturnValue(new Promise(() => {}));
  cancelCeremony.mockReset();
  browserSupportsWebAuthnAutofill.mockReset().mockResolvedValue(true);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it("offers passkeys only once the server reports one is registered", async () => {
  mount(routeFetch({}, { passkeyEnabled: true }));
  expect(await screen.findByTestId("login-passkey-button")).toBeInTheDocument();
});

it("hides the passkey button when no passkey is registered", async () => {
  mount(routeFetch({}, { passkeyEnabled: false }));
  await screen.findByTestId("login-password-input");
  expect(screen.queryByTestId("login-passkey-button")).not.toBeInTheDocument();
});

it("stays usable against a server too old to report passkey support", async () => {
  mount(routeFetch({}));
  expect(await screen.findByTestId("login-password-input")).toBeInTheDocument();
  expect(screen.queryByTestId("login-passkey-button")).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("marks the password field so passkeys appear in its autofill menu", async () => {
  mount(routeFetch({}, { passkeyEnabled: true }));
  const input = await screen.findByTestId("login-password-input");
  expect(input).toHaveAttribute("autocomplete", "current-password webauthn");
});

it("leaves the password field alone when passkeys are off", async () => {
  mount(routeFetch({}, { passkeyEnabled: false }));
  const input = await screen.findByTestId("login-password-input");
  expect(input).toHaveAttribute("autocomplete", "current-password");
});

it("arms autofill on load when the browser supports conditional mediation", async () => {
  startAuthentication.mockReturnValue(new Promise(() => {}));
  mount(routeFetch({}, { passkeyEnabled: true }));
  await screen.findByTestId("login-passkey-button");
  await waitFor(() =>
    expect(startAuthentication).toHaveBeenCalledWith(
      expect.objectContaining({ useBrowserAutofill: true }),
    ),
  );
});

it("skips autofill where it is unsupported, as on iOS standalone", async () => {
  browserSupportsWebAuthnAutofill.mockResolvedValue(false);
  mount(routeFetch({}, { passkeyEnabled: true }));
  const button = await screen.findByTestId("login-passkey-button");
  expect(button).toBeEnabled();
  await waitFor(() => expect(browserSupportsWebAuthnAutofill).toHaveBeenCalled());
  expect(startAuthentication).not.toHaveBeenCalled();
});

it("signs in when the user picks a passkey from the button", async () => {
  browserSupportsWebAuthnAutofill.mockResolvedValue(false);
  startAuthentication.mockResolvedValue(ASSERTION);
  mount(routeFetch({}, { passkeyEnabled: true }));

  fireEvent.click(await screen.findByTestId("login-passkey-button"));

  expect(await screen.findByText("Private portfolio")).toBeInTheDocument();
  expect(startAuthentication).toHaveBeenCalledWith(
    expect.objectContaining({ useBrowserAutofill: false }),
  );
});

it("says nothing when the user dismisses the system prompt", async () => {
  browserSupportsWebAuthnAutofill.mockResolvedValue(false);
  const dismissed = new Error("dismissed");
  dismissed.name = "NotAllowedError";
  startAuthentication.mockRejectedValue(dismissed);
  mount(routeFetch({}, { passkeyEnabled: true }));

  const button = await screen.findByTestId("login-passkey-button");
  fireEvent.click(button);

  await waitFor(() => expect(button).toBeEnabled());
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByTestId("login-password-input")).toBeInTheDocument();
});

it("surfaces a rejected passkey so the user knows to try the password", async () => {
  browserSupportsWebAuthnAutofill.mockResolvedValue(false);
  startAuthentication.mockResolvedValue(ASSERTION);
  mount(
    routeFetch(
      {
        finish: () =>
          Response.json({ code: 401, message: "This passkey was not recognised" }, { status: 401 }),
      },
      { passkeyEnabled: true },
    ),
  );

  fireEvent.click(await screen.findByTestId("login-passkey-button"));

  expect(await screen.findByRole("alert")).toHaveTextContent("This passkey was not recognised");
  expect(screen.queryByText("Private portfolio")).not.toBeInTheDocument();
});

it("cancels a pending ceremony when the login page goes away", async () => {
  startAuthentication.mockReturnValue(new Promise(() => {}));
  const { unmount } = mount(routeFetch({}, { passkeyEnabled: true }));
  await screen.findByTestId("login-passkey-button");
  unmount();
  expect(cancelCeremony).toHaveBeenCalled();
});
