import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

class FakeXhr {
  static last: FakeXhr;
  upload = {} as { onprogress?: (event: ProgressEvent) => void };
  onload?: () => void;
  onerror?: () => void;
  onabort?: () => void;
  status = 200;
  responseText = "{}";
  headers: Record<string, string> = {};
  sentHeaders: Record<string, string> = {};
  aborted = false;
  body?: unknown;

  constructor() {
    FakeXhr.last = this;
  }
  open() {}
  setRequestHeader(name: string, value: string) {
    this.sentHeaders[name] = value;
  }
  getResponseHeader(name: string) {
    return this.headers[name.toLowerCase()] ?? null;
  }
  send(body: unknown) {
    this.body = body;
  }
  abort() {
    this.aborted = true;
    this.onabort?.();
  }

  respond(status: number, responseText = "{}", headers: Record<string, string> = {}) {
    this.status = status;
    this.responseText = responseText;
    this.headers = headers;
    this.onload?.();
  }
}

const file = () => new File(["receipt"], "receipt.png", { type: "image/png" });

let postFileWithProgress: typeof import("./upload").postFileWithProgress;
let session: typeof import("@/features/profiles/session");
let setUnauthorizedHandler: typeof import("@/lib/auth-token").setUnauthorizedHandler;

beforeEach(async () => {
  vi.resetModules();
  vi.stubGlobal("XMLHttpRequest", FakeXhr);
  session = await import("@/features/profiles/session");
  ({ setUnauthorizedHandler } = await import("@/lib/auth-token"));
  ({ postFileWithProgress } = await import("./upload"));
  session.installProfileSession({ profileId: "p-1", scopeId: "scope-1" });
});

afterEach(() => {
  vi.unstubAllGlobals();
  setUnauthorizedHandler(null);
});

describe("postFileWithProgress", () => {
  it("sends the profile scope that fetch would have sent", async () => {
    const promise = postFileWithProgress("/upload", file());
    FakeXhr.last.respond(200, JSON.stringify({ id: "a-1" }));
    await expect(promise).resolves.toEqual({ id: "a-1" });
    expect(FakeXhr.last.sentHeaders["x-wf-profile-scope"]).toBe("scope-1");
    expect(FakeXhr.last.body).toBeInstanceOf(FormData);
  });

  it("reports progress only while the length is known", async () => {
    const onProgress = vi.fn();
    const promise = postFileWithProgress("/upload", file(), { onProgress });

    FakeXhr.last.upload.onprogress?.({
      lengthComputable: true,
      loaded: 25,
      total: 100,
    } as unknown as ProgressEvent);
    FakeXhr.last.upload.onprogress?.({
      lengthComputable: false,
      loaded: 50,
      total: 0,
    } as unknown as ProgressEvent);
    FakeXhr.last.upload.onprogress?.({
      lengthComputable: true,
      loaded: 100,
      total: 100,
    } as unknown as ProgressEvent);

    FakeXhr.last.respond(200, JSON.stringify({ id: "a-1" }));
    await promise;
    expect(onProgress.mock.calls).toEqual([[0.25], [1]]);
  });

  it("surfaces the server's message on a failed upload", async () => {
    const promise = postFileWithProgress("/upload", file());
    FakeXhr.last.respond(400, JSON.stringify({ message: "File too large" }));
    await expect(promise).rejects.toThrow("File too large");
  });

  it("falls back to a generic message when the error body is not JSON", async () => {
    const promise = postFileWithProgress("/upload", file());
    FakeXhr.last.respond(500, "<html>nope</html>");
    await expect(promise).rejects.toThrow("Attachment request failed");
  });

  it("treats a proxy sign-in page as an auth requirement", async () => {
    const unauthorized = vi.fn();
    setUnauthorizedHandler(unauthorized);
    const promise = postFileWithProgress("/upload", file());
    FakeXhr.last.respond(200, "<html>sign in</html>", { "content-type": "text/html" });
    await expect(promise).rejects.toThrow("PROFILE_AUTH_REQUIRED");
    expect(unauthorized).toHaveBeenCalledWith("signIn");
  });

  it("revokes the profile session on 423", async () => {
    const promise = postFileWithProgress("/upload", file());
    FakeXhr.last.respond(423, JSON.stringify({ message: "Locked" }));
    await expect(promise).rejects.toThrow();
    expect(session.hasProfileSession()).toBe(false);
  });

  it("does not start when the signal is already aborted", async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(
      postFileWithProgress("/upload", file(), { signal: controller.signal }),
    ).rejects.toThrow("Upload aborted");
  });

  it("aborts an in-flight upload", async () => {
    const controller = new AbortController();
    const promise = postFileWithProgress("/upload", file(), { signal: controller.signal });
    controller.abort();
    await expect(promise).rejects.toThrow("Upload aborted");
    expect(FakeXhr.last.aborted).toBe(true);
  });
});
