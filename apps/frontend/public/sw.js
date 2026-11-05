/**
 * Wealthfolio service worker.
 */

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

async function anyClientVisible() {
  const clients = await self.clients.matchAll({
    type: "window",
    includeUncontrolled: true,
  });
  return clients.some((client) => client.visibilityState === "visible");
}

function parsePayload(event) {
  if (!event.data) return null;
  try {
    return event.data.json();
  } catch {
    return null;
  }
}

self.addEventListener("push", (event) => {
  event.waitUntil(
    (async () => {
      const payload = parsePayload(event);
      if (!payload) {
        await self.registration.showNotification("Wealthfolio", {
          body: "Something changed in your portfolio.",
          icon: "/app-icon-192.png",
          badge: "/app-icon-192.png",
          tag: "wf-generic",
        });
        return;
      }

      if (await anyClientVisible()) return;

      await self.registration.showNotification(payload.title ?? "Wealthfolio", {
        body: payload.body ?? "",
        icon: "/app-icon-192.png",
        badge: "/app-icon-192.png",
        tag: payload.tag ?? "wealthfolio",
        renotify: Boolean(payload.tag),
        data: { url: payload.url ?? "/", kind: payload.kind ?? "" },
      });
    })(),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const target = event.notification.data?.url ?? "/";

  event.waitUntil(
    (async () => {
      const clients = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      for (const client of clients) {
        if ("focus" in client) {
          if ("navigate" in client) {
            try {
              await client.navigate(target);
            } catch {
              // Cross-origin or a navigation already in flight; focusing the
              // existing tab is still better than opening another.
            }
          }
          await client.focus();
          return;
        }
      }
      await self.clients.openWindow(target);
    })(),
  );
});
