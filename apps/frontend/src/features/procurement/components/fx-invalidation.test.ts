import { QueryClient } from "@tanstack/react-query";
import { expect, it } from "vitest";
import { invalidatePerformanceCaches } from "@/lib/performance-cache";
import { QueryKeys } from "@/lib/query-keys";

it("invalidates procurement prices when exchange rates change", () => {
  const client = new QueryClient();
  const keys = [[QueryKeys.PROCUREMENT_ITEMS], [QueryKeys.PROCUREMENT_ITEM_PRICES, "item"]];
  for (const key of keys) client.setQueryData(key, { cached: true });
  client.setQueryData([QueryKeys.BROKER_CONNECTIONS], []);
  invalidatePerformanceCaches(client);
  for (const key of keys) expect(client.getQueryState(key)?.isInvalidated).toBe(true);
  expect(client.getQueryState([QueryKeys.BROKER_CONNECTIONS])?.isInvalidated).toBe(false);
});
