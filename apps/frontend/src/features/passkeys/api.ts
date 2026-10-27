/**
 * Passkey (WebAuthn) sign-in and credential management.
 *
 * These call `fetch` directly rather than going through `@/adapters`, for the
 * same reason `auth-context.tsx` does: they run before a profile scope exists,
 * and they are web-only — the desktop build never mounts `AuthGate`. Keeping
 * them out of `adapters/` (and out of any `features/*\/adapters/` directory)
 * also keeps them out of `adapter-command-parity.test.ts`, which would
 * otherwise require a matching Tauri command for each one.
 */
import {
  browserSupportsWebAuthn,
  browserSupportsWebAuthnAutofill,
  startAuthentication,
  startRegistration,
  WebAuthnAbortService,
} from "@simplewebauthn/browser";
import type {
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
} from "@simplewebauthn/browser";

const AUTH_BASE = "/api/v1/auth/passkey";
const MANAGE_BASE = "/api/v1/passkeys";

export interface PasskeySummary {
  id: string;
  name: string;
  createdAt: string;
  lastUsedAt: string | null;
}

export interface PasskeyListResponse {
  configured: boolean;
  passkeys: PasskeySummary[];
}

interface ChallengeEnvelope<T> {
  publicKey: T;
}

export class PasskeyCancelled extends Error {
  constructor() {
    super("Passkey prompt dismissed");
    this.name = "PasskeyCancelled";
  }
}

function isCancellation(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  // NotAllowedError covers both "user cancelled" and "timed out"; AbortError
  // is what we get when a newer ceremony supersedes this one.
  return error.name === "NotAllowedError" || error.name === "AbortError";
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    credentials: "same-origin",
    ...init,
    headers:
      init?.body === undefined
        ? init?.headers
        : { "Content-Type": "application/json", ...init?.headers },
  });

  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      const body: unknown = await response.json();
      if (
        body !== null &&
        typeof body === "object" &&
        "message" in body &&
        typeof body.message === "string" &&
        body.message.length > 0
      ) {
        message = body.message;
      }
    } catch {
      // A non-JSON error body (a proxy page, say) leaves the status message.
    }
    throw new Error(message);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function supportsPasskeys(): boolean {
  return browserSupportsWebAuthn();
}

export function supportsPasskeyAutofill(): Promise<boolean> {
  return browserSupportsWebAuthnAutofill();
}

export function cancelPasskeyCeremony(): void {
  WebAuthnAbortService.cancelCeremony();
}

/**
 * Run a full sign-in ceremony. On success the server has set the session
 * cookie, exactly as password login does.
 */
export async function signInWithPasskey({ autofill = false } = {}): Promise<void> {
  const options = await request<ChallengeEnvelope<PublicKeyCredentialRequestOptionsJSON>>(
    `${AUTH_BASE}/start`,
    { method: "POST" },
  );

  let assertion;
  try {
    assertion = await startAuthentication({
      optionsJSON: options.publicKey,
      useBrowserAutofill: autofill,
    });
  } catch (error) {
    if (isCancellation(error)) throw new PasskeyCancelled();
    throw error;
  }

  await request<unknown>(`${AUTH_BASE}/finish`, {
    method: "POST",
    body: JSON.stringify(assertion),
  });
}

export function listPasskeys(): Promise<PasskeyListResponse> {
  return request<PasskeyListResponse>(MANAGE_BASE);
}

/** Enrol a new passkey. Requires an authenticated session. */
export async function registerPasskey(name: string): Promise<PasskeySummary> {
  const options = await request<ChallengeEnvelope<PublicKeyCredentialCreationOptionsJSON>>(
    `${MANAGE_BASE}/register/start`,
    { method: "POST" },
  );

  let credential;
  try {
    credential = await startRegistration({ optionsJSON: options.publicKey });
  } catch (error) {
    if (isCancellation(error)) throw new PasskeyCancelled();
    throw error;
  }

  return request<PasskeySummary>(`${MANAGE_BASE}/register/finish`, {
    method: "POST",
    body: JSON.stringify({ name, credential }),
  });
}

export function renamePasskey(id: string, name: string): Promise<PasskeySummary> {
  return request<PasskeySummary>(`${MANAGE_BASE}/${id}`, {
    method: "PATCH",
    body: JSON.stringify({ name }),
  });
}

export function deletePasskey(id: string): Promise<void> {
  return request<void>(`${MANAGE_BASE}/${id}`, { method: "DELETE" });
}
