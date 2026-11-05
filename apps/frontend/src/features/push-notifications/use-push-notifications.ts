import { useCallback, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { QueryKeys } from "@/lib/query-keys";

import {
  decodeApplicationServerKey,
  getPushConfig,
  getPushPreferences,
  registerSubscription,
  removeSubscription,
  requiresHomeScreenInstall,
  sendTestNotification,
  supportsPushNotifications,
  updatePushPreferences,
  type PushPreferences,
} from "./api";

const SERVICE_WORKER_URL = "/sw.js";

export type PushPermission = NotificationPermission | "unsupported";

function describeBrowser(): string {
  if (typeof navigator === "undefined") return "Browser";
  const agent = navigator.userAgent;
  const browser = /Edg\//.test(agent)
    ? "Edge"
    : /OPR\//.test(agent)
      ? "Opera"
      : /Chrome\//.test(agent)
        ? "Chrome"
        : /Firefox\//.test(agent)
          ? "Firefox"
          : /Safari\//.test(agent)
            ? "Safari"
            : "Browser";
  const platform = /Android/.test(agent)
    ? "Android"
    : /iPhone|iPad|iPod/.test(agent)
      ? "iOS"
      : /Mac OS X/.test(agent)
        ? "macOS"
        : /Windows/.test(agent)
          ? "Windows"
          : /Linux/.test(agent)
            ? "Linux"
            : "";
  return platform ? `${browser} on ${platform}` : browser;
}

export function usePushNotifications() {
  const queryClient = useQueryClient();
  const supported = supportsPushNotifications();

  const [subscription, setSubscription] = useState<PushSubscription | null>(null);
  const [permission, setPermission] = useState<PushPermission>(
    supported ? Notification.permission : "unsupported",
  );
  const [reading, setReading] = useState(supported);

  useEffect(() => {
    if (!supported) return;
    let cancelled = false;

    void (async () => {
      try {
        const registration = await navigator.serviceWorker.getRegistration(SERVICE_WORKER_URL);
        const existing = (await registration?.pushManager.getSubscription()) ?? null;
        if (!cancelled) setSubscription(existing);
      } catch {
        if (!cancelled) setSubscription(null);
      } finally {
        if (!cancelled) setReading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [supported]);

  const configQuery = useQuery({
    queryKey: [QueryKeys.PUSH_CONFIG],
    queryFn: getPushConfig,
    enabled: supported,
  });

  const preferencesQuery = useQuery({
    queryKey: [QueryKeys.PUSH_PREFERENCES],
    queryFn: getPushPreferences,
    enabled: supported && configQuery.data?.enabled === true,
  });

  const enable = useMutation({
    mutationFn: async () => {
      const publicKey = configQuery.data?.publicKey;
      if (!publicKey) {
        throw new Error("This server has no push notification keys configured.");
      }

      const granted = await Notification.requestPermission();
      setPermission(granted);
      if (granted !== "granted") {
        throw new Error("Notification permission was not granted.");
      }

      const registration = await navigator.serviceWorker.register(SERVICE_WORKER_URL);
      await navigator.serviceWorker.ready;

      const existing = await registration.pushManager.getSubscription();
      await existing?.unsubscribe();

      const created = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: decodeApplicationServerKey(publicKey),
      });

      await registerSubscription(created, describeBrowser());
      return created;
    },
    onSuccess: (created) => {
      setSubscription(created);
      void queryClient.invalidateQueries({ queryKey: [QueryKeys.PUSH_PREFERENCES] });
    },
  });

  const disable = useMutation({
    mutationFn: async () => {
      if (!subscription) return;
      const { endpoint } = subscription;
      await subscription.unsubscribe().catch(() => {
        // Already gone on the browser side; the server still needs telling.
      });
      await removeSubscription(endpoint);
    },
    onSuccess: () => {
      setSubscription(null);
      void queryClient.invalidateQueries({ queryKey: [QueryKeys.PUSH_PREFERENCES] });
    },
  });

  const savePreferences = useMutation({
    mutationFn: (preferences: PushPreferences) => updatePushPreferences(preferences),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [QueryKeys.PUSH_PREFERENCES] });
    },
  });

  const sendTest = useMutation({ mutationFn: sendTestNotification });

  const setPreference = useCallback(
    (key: keyof PushPreferences, value: boolean) => {
      const current = preferencesQuery.data;
      if (!current) return;
      savePreferences.mutate({
        transactions: current.transactions,
        budgets: current.budgets,
        [key]: value,
      });
    },
    [preferencesQuery.data, savePreferences],
  );

  return {
    supported,
    needsHomeScreenInstall: requiresHomeScreenInstall(),
    permission,
    serverEnabled: configQuery.data?.enabled ?? false,
    loading: reading || configQuery.isLoading,
    subscribed: subscription !== null,
    preferences: preferencesQuery.data,
    enable,
    disable,
    sendTest,
    savePreferences,
    setPreference,
  };
}
