const MAX_TICKER_LOGO_BYTES = 512 * 1024;
const DEFAULT_CACHE_SIZE = 256;

export function normalizeTickerLogoSymbol(symbol: unknown) {
  if (typeof symbol !== "string") {
    return undefined;
  }

  const normalized = symbol.trim().toUpperCase();
  if (
    !normalized ||
    normalized.includes("..") ||
    normalized.includes("/") ||
    normalized.includes("\\") ||
    !/^[A-Z0-9$^._:-]+$/.test(normalized)
  ) {
    return undefined;
  }
  return normalized;
}

export class TickerLogoAssetBridge {
  private readonly cache = new Map<string, Blob | null>();
  private readonly pending = new Map<string, Promise<Blob | null>>();

  constructor(
    private readonly fetchAsset: typeof fetch = fetch,
    private readonly maxEntries = DEFAULT_CACHE_SIZE,
  ) {}

  load(symbol: unknown): Promise<Blob | null> {
    const normalized = normalizeTickerLogoSymbol(symbol);
    if (!normalized) {
      return Promise.resolve(null);
    }

    const cached = this.cache.get(normalized);
    if (cached !== undefined) {
      this.cache.delete(normalized);
      this.cache.set(normalized, cached);
      return Promise.resolve(cached);
    }

    const inFlight = this.pending.get(normalized);
    if (inFlight) {
      return inFlight;
    }

    const request = this.fetchLogo(normalized).finally(() => {
      this.pending.delete(normalized);
    });
    this.pending.set(normalized, request);
    return request;
  }

  get cacheSize() {
    return this.cache.size;
  }

  private async fetchLogo(symbol: string): Promise<Blob | null> {
    try {
      const basePath = import.meta.env.BASE_URL || "/";
      const url = new URL(
        `${basePath.replace(/\/?$/, "/")}ticker-logos/${encodeURIComponent(symbol)}.png`,
        window.location.href,
      );
      // WebKit brand-checks Window.fetch; a normal class-field call uses this bridge as receiver.
      const response = await this.fetchAsset.call(globalThis, url, { cache: "no-cache" });
      const contentLength = Number(response.headers.get("content-length") || "0");
      const contentType = response.headers
        .get("content-type")
        ?.split(";", 1)[0]
        .trim()
        .toLowerCase();
      if (
        !response.ok ||
        contentType !== "image/png" ||
        (contentLength > 0 && contentLength > MAX_TICKER_LOGO_BYTES)
      ) {
        if (response.status === 404 || response.ok) {
          this.remember(symbol, null);
        }
        return null;
      }

      const blob = await response.blob();
      if (blob.type.toLowerCase() !== "image/png" || blob.size > MAX_TICKER_LOGO_BYTES) {
        this.remember(symbol, null);
        return null;
      }

      this.remember(symbol, blob);
      return blob;
    } catch {
      return null;
    }
  }

  private remember(symbol: string, value: Blob | null) {
    this.cache.delete(symbol);
    this.cache.set(symbol, value);
    while (this.cache.size > this.maxEntries) {
      const oldest = this.cache.keys().next().value;
      if (!oldest) {
        break;
      }
      this.cache.delete(oldest);
    }
  }
}

export const tickerLogoAssetBridge = new TickerLogoAssetBridge();
