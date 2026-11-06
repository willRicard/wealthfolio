/**
 * Web Push subscription management.
 *
 * These call `profileFetch` directly rather than going through `@/adapters`,
 * for the same reason `features/passkeys/api.ts` does: push notifications are
 * web-only, and the desktop build has no equivalent command. Keeping them out
 * of `adapters/` (and out of any `features/*\/adapters/` directory) also keeps
 * them out of `adapter-command-parity.test.ts`, which would otherwise require
 * a matching Tauri command for each one.
 */
import { profileFetch } from "@/features/profiles/session";

const BASE = "/api/v1/push";

export interface PushConfig {
  enabled: boolean;
  publicKey: string | null;
}

export interface PushPreferences {
  transactions: boolean;
  budgets: boolean;
}

export interface PushPreferencesResponse extends PushPreferences {
  subscriptionCount: number;
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await profileFetch(url, {
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

export function getPushConfig(): Promise<PushConfig> {
  return request<PushConfig>(`${BASE}/config`);
}

export function getPushPreferences(): Promise<PushPreferencesResponse> {
  return request<PushPreferencesResponse>(`${BASE}/preferences`);
}

export function updatePushPreferences(preferences: PushPreferences): Promise<void> {
  return request<void>(`${BASE}/preferences`, {
    method: "PUT",
    body: JSON.stringify(preferences),
  });
}

export function registerSubscription(subscription: PushSubscription, label: string): Promise<void> {
  const json = subscription.toJSON();
  return request<void>(`${BASE}/subscriptions`, {
    method: "POST",
    body: JSON.stringify({
      endpoint: subscription.endpoint,
      keys: { p256dh: json.keys?.p256dh ?? "", auth: json.keys?.auth ?? "" },
      label,
    }),
  });
}

export function removeSubscription(endpoint: string): Promise<void> {
  return request<void>(`${BASE}/subscriptions`, {
    method: "DELETE",
    body: JSON.stringify({ endpoint }),
  });
}

export function sendTestNotification(): Promise<{ delivered: number }> {
  return request<{ delivered: number }>(`${BASE}/test`, { method: "POST" });
}

/**
 * `applicationServerKey` wants raw bytes, while the server sends the key as
 * base64url (the form `web-push generate-vapid-keys` prints).
 */
export function decodeApplicationServerKey(base64Url: string): Uint8Array<ArrayBuffer> {
  const padded = base64Url.padEnd(base64Url.length + ((4 - (base64Url.length % 4)) % 4), "=");
  const binary = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function supportsPushNotifications(): boolean {
  return (
    typeof navigator !== "undefined" &&
    "serviceWorker" in navigator &&
    typeof window !== "undefined" &&
    "PushManager" in window &&
    "Notification" in window
  );
}

export function requiresHomeScreenInstall(): boolean {
  if (typeof navigator === "undefined" || typeof window === "undefined") return false;
  const isIos =
    ["iPad", "iPhone", "iPod"].some((name) => navigator.userAgent.includes(name)) ||
    // iPadOS 13+ reports itself as a Mac, but still has a touch screen.
    (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
  if (!isIos) return false;
  const standalone =
    window.matchMedia?.("(display-mode: standalone)").matches ||
    (window.navigator as Navigator & { standalone?: boolean }).standalone === true;
  return !standalone;
}
