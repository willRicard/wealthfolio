import { describe, expect, it, vi } from "vitest";
import { normalizeTickerLogoSymbol, TickerLogoAssetBridge } from "./ticker-logo-asset-bridge";

function pngResponse(content = "png", headers: Record<string, string> = {}) {
  return new Response(new Blob([content], { type: "image/png" }), {
    headers: { "content-type": "image/png", ...headers },
    status: 200,
  });
}

describe("TickerLogoAssetBridge", () => {
  it("normalizes symbols and rejects traversal or path separators", () => {
    expect(normalizeTickerLogoSymbol(" brk.b ")).toBe("BRK.B");
    expect(normalizeTickerLogoSymbol("$cash-usd")).toBe("$CASH-USD");
    expect(normalizeTickerLogoSymbol("../secret")).toBeUndefined();
    expect(normalizeTickerLogoSymbol("foo/bar")).toBeUndefined();
    expect(normalizeTickerLogoSymbol("foo\\bar")).toBeUndefined();
  });

  it("deduplicates concurrent requests and bounds the Blob LRU", async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(pngResponse()));
    const bridge = new TickerLogoAssetBridge(fetchMock as unknown as typeof fetch, 2);

    const [first, second] = await Promise.all([bridge.load("AAPL"), bridge.load("AAPL")]);
    expect(first).toBe(second);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await bridge.load("MSFT");
    await bridge.load("GOOG");
    expect(bridge.cacheSize).toBe(2);
    await bridge.load("AAPL");
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("calls Window.fetch with the Window receiver required by WebKit", async () => {
    const fetchMock = vi.fn(function (this: unknown) {
      if (this !== globalThis) {
        throw new TypeError("Window.fetch called with an invalid receiver");
      }
      return Promise.resolve(pngResponse());
    });
    const bridge = new TickerLogoAssetBridge(fetchMock as unknown as typeof fetch);

    await expect(bridge.load("AAPL")).resolves.toBeInstanceOf(Blob);
    expect(fetchMock).toHaveBeenCalledOnce();
  });

  it("returns null for missing, non-PNG, and oversized responses", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 404 }))
      .mockResolvedValueOnce(
        new Response("text", { headers: { "content-type": "text/plain" }, status: 200 }),
      )
      .mockResolvedValueOnce(pngResponse("small", { "content-length": String(512 * 1024 + 1) }));
    const bridge = new TickerLogoAssetBridge(fetchMock as unknown as typeof fetch);

    await expect(bridge.load("MISSING")).resolves.toBeNull();
    await expect(bridge.load("TEXT")).resolves.toBeNull();
    await expect(bridge.load("HUGE")).resolves.toBeNull();
  });

  it("caches misses in the bounded LRU and retries transient failures", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 404 }))
      .mockResolvedValueOnce(pngResponse("msft"))
      .mockResolvedValueOnce(pngResponse("goog"))
      .mockResolvedValueOnce(new Response(null, { status: 404 }));
    const bridge = new TickerLogoAssetBridge(fetchMock as unknown as typeof fetch, 2);

    await expect(bridge.load("MISSING")).resolves.toBeNull();
    await expect(bridge.load("MISSING")).resolves.toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await bridge.load("MSFT");
    await bridge.load("GOOG");
    expect(bridge.cacheSize).toBe(2);
    await expect(bridge.load("MISSING")).resolves.toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(4);

    const transientFetch = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("network unavailable"))
      .mockResolvedValueOnce(pngResponse("recovered"));
    const retryingBridge = new TickerLogoAssetBridge(transientFetch as unknown as typeof fetch);
    await expect(retryingBridge.load("RETRY")).resolves.toBeNull();
    await expect(retryingBridge.load("RETRY")).resolves.toBeInstanceOf(Blob);
    expect(transientFetch).toHaveBeenCalledTimes(2);
  });
});
