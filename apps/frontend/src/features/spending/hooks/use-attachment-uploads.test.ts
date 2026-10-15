import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AttachmentUploadOptions, TransactionAttachment } from "@/adapters";

const uploadTransactionAttachment = vi.fn();
const deleteTransactionAttachment = vi.fn();
let supportsCancel = true;

vi.mock("@/adapters", () => ({
  uploadTransactionAttachment: (...args: unknown[]) => uploadTransactionAttachment(...args),
  deleteTransactionAttachment: (...args: unknown[]) => deleteTransactionAttachment(...args),
  get SUPPORTS_UPLOAD_PROGRESS() {
    return true;
  },
  get SUPPORTS_UPLOAD_CANCEL() {
    return supportsCancel;
  },
}));

const { useAttachmentUploads } = await import("./use-attachment-uploads");

const png = (name: string) => new File(["bytes"], name, { type: "image/png" });

const ACTIVITY = { kind: "activity", id: "act-1" } as const;
const DRAFT = { kind: "draft", token: "draft-token" } as const;

const stored = (id: string): TransactionAttachment => ({
  id,
  activityId: "act-1",
  filename: `${id}.png`,
  contentType: "image/png",
  sizeBytes: 5,
  createdAt: "2026-10-13",
});

function deferred<T>() {
  let settle!: (value: T) => void;
  let fail!: (error: unknown) => void;
  const promise = new Promise<T>((resolve, reject) => {
    settle = resolve;
    fail = reject;
  });
  return { promise, settle, fail };
}

beforeEach(() => {
  supportsCancel = true;
  uploadTransactionAttachment.mockReset();
  deleteTransactionAttachment.mockReset();
});

describe("useAttachmentUploads", () => {
  it("uploads on select when an activity already exists, one file at a time", async () => {
    const inFlight: string[] = [];
    uploadTransactionAttachment.mockImplementation(async (_target, file: File) => {
      inFlight.push(file.name);
      expect(inFlight).toHaveLength(1);
      await Promise.resolve();
      inFlight.pop();
      return stored(file.name);
    });
    const onUploaded = vi.fn();
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY, onUploaded }));

    act(() => result.current.add([png("a.png"), png("b.png")]));

    await waitFor(() => expect(result.current.isUploading).toBe(false));
    expect(uploadTransactionAttachment).toHaveBeenCalledTimes(2);
    expect(onUploaded).toHaveBeenCalledTimes(2);
    expect(result.current.uploads.map((upload) => upload.status)).toEqual(["done", "done"]);
    expect(result.current.uploads.map((upload) => upload.attachmentId)).toEqual(["a.png", "b.png"]);
  });

  it("stages against a draft token when there is no transaction yet", async () => {
    uploadTransactionAttachment.mockImplementation(async (_target, file: File) =>
      stored(file.name),
    );
    const { result } = renderHook(() => useAttachmentUploads({ target: DRAFT }));

    act(() => result.current.add([png("a.png")]));

    await waitFor(() => expect(result.current.uploads[0].status).toBe("done"));
    expect(uploadTransactionAttachment).toHaveBeenCalledWith(
      DRAFT,
      expect.any(File),
      expect.anything(),
    );
  });

  it("flush waits for an upload already in flight before resolving", async () => {
    const gate = deferred<TransactionAttachment>();
    uploadTransactionAttachment.mockReturnValue(gate.promise);
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY }));

    act(() => result.current.add([png("a.png")]));
    await waitFor(() => expect(result.current.uploads[0].status).toBe("uploading"));

    let settled = false;
    const flushed = act(async () => {
      await result.current.flush();
      settled = true;
    });
    await Promise.resolve();
    expect(settled).toBe(false);

    gate.settle(stored("a"));
    await flushed;
    expect(settled).toBe(true);
  });

  it("reports measured progress and surfaces the adapter's failure message", async () => {
    uploadTransactionAttachment.mockImplementationOnce(
      async (_t, _f, options: AttachmentUploadOptions) => {
        options.onProgress?.(0.5);
        throw new Error("Attachment request failed");
      },
    );
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY }));

    act(() => result.current.add([png("a.png")]));

    await waitFor(() => expect(result.current.uploads[0].status).toBe("failed"));
    expect(result.current.uploads[0].error).toBe("Attachment request failed");
    expect(result.current.uploads[0].progress).toBeUndefined();

    uploadTransactionAttachment.mockResolvedValueOnce(stored("a"));
    act(() => result.current.retry(result.current.uploads[0].id));
    await waitFor(() => expect(result.current.uploads[0].status).toBe("done"));
    expect(result.current.uploads[0].error).toBeUndefined();
    expect(result.current.resolveAttachmentId(result.current.uploads[0].id)).toBe("a");
  });

  it("marks an aborted upload cancelled without deleting anything", async () => {
    uploadTransactionAttachment.mockImplementation(
      (_t, _f, options: AttachmentUploadOptions) =>
        new Promise((_resolve, reject) =>
          options.signal?.addEventListener("abort", () =>
            reject(Object.assign(new Error("Upload aborted"), { name: "AbortError" })),
          ),
        ),
    );
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY }));

    act(() => result.current.add([png("a.png")]));
    await waitFor(() => expect(result.current.uploads[0].status).toBe("uploading"));

    act(() => result.current.cancel(result.current.uploads[0].id));

    await waitFor(() => expect(result.current.uploads[0].status).toBe("cancelled"));
    expect(deleteTransactionAttachment).not.toHaveBeenCalled();
  });

  it("deletes the stored file when the transport could not stop mid-flight (Tauri)", async () => {
    supportsCancel = false;
    const gate = deferred<TransactionAttachment>();
    uploadTransactionAttachment.mockReturnValue(gate.promise);
    deleteTransactionAttachment.mockResolvedValue(undefined);
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY }));

    act(() => result.current.add([png("a.png")]));
    await waitFor(() => expect(result.current.uploads[0].status).toBe("uploading"));
    act(() => result.current.cancel(result.current.uploads[0].id));

    await act(async () => {
      gate.settle(stored("file-7"));
      await gate.promise;
    });

    await waitFor(() =>
      expect(deleteTransactionAttachment).toHaveBeenCalledWith(ACTIVITY, "file-7"),
    );
    expect(result.current.uploads[0].status).toBe("cancelled");
  });

  it("does not delete a retried upload that reuses a cancelled item", async () => {
    supportsCancel = false;
    const { result } = renderHook(() => useAttachmentUploads({ target: ACTIVITY }));

    uploadTransactionAttachment.mockRejectedValueOnce(new Error("boom"));
    act(() => result.current.add([png("a.png")]));
    await waitFor(() => expect(result.current.uploads[0].status).toBe("failed"));

    act(() => result.current.cancel(result.current.uploads[0].id));
    uploadTransactionAttachment.mockResolvedValueOnce(stored("file-8"));
    act(() => result.current.retry(result.current.uploads[0].id));

    await waitFor(() => expect(result.current.uploads[0].status).toBe("done"));
    expect(deleteTransactionAttachment).not.toHaveBeenCalled();
  });
});
