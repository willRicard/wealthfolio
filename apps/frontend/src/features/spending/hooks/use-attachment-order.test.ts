import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { TransactionAttachment } from "@/adapters";

import { useAttachmentOrder } from "./use-attachment-order";
import type { AttachmentUpload } from "./use-attachment-uploads";

const stored = (id: string): TransactionAttachment => ({
  id,
  activityId: "act-1",
  filename: `${id}.png`,
  contentType: "image/png",
  sizeBytes: 5,
  createdAt: "2026-10-13",
});

const upload = (id: string, overrides: Partial<AttachmentUpload> = {}): AttachmentUpload => ({
  id,
  file: new File(["bytes"], `${id}.png`, { type: "image/png" }),
  status: "uploading",
  ...overrides,
});

const done = (id: string, attachmentId: string) => upload(id, { status: "done", attachmentId });

const keys = (result: { current: { items: { key: string }[] } }) =>
  result.current.items.map((item) => item.key);

describe("useAttachmentOrder", () => {
  it("lists stored files first and queued uploads after them", () => {
    const { result } = renderHook(() =>
      useAttachmentOrder([stored("a"), stored("b")], [upload("u1")]),
    );

    expect(keys(result)).toEqual(["a", "b", "u1"]);
    expect(result.current.items.map((item) => item.kind)).toEqual(["saved", "saved", "upload"]);
    expect(result.current.isDirty).toBe(false);
  });

  it("keeps a finished upload on its own key and drops the server's copy of it", () => {
    const { result, rerender } = renderHook(
      ({ attachments, uploads }) => useAttachmentOrder(attachments, uploads),
      { initialProps: { attachments: [stored("a")], uploads: [upload("u1")] } },
    );

    expect(keys(result)).toEqual(["a", "u1"]);

    rerender({ attachments: [stored("a")], uploads: [done("u1", "b")] });
    expect(keys(result)).toEqual(["a", "u1"]);
    const claimed = result.current.items[1];
    expect(claimed.kind).toBe("saved");
    if (claimed.kind !== "saved") throw new Error("expected a saved item");
    expect(claimed.attachment.id).toBe("b");
    expect(claimed.localFile?.name).toBe("u1.png");
    expect(claimed.uploadId).toBe("u1");

    rerender({ attachments: [stored("a"), stored("b")], uploads: [done("u1", "b")] });
    expect(keys(result)).toEqual(["a", "u1"]);
  });

  it("moves a tile to another tile's slot and reports the order as dirty", () => {
    const { result } = renderHook(() =>
      useAttachmentOrder([stored("a"), stored("b"), stored("c")], []),
    );

    act(() => result.current.move("c", "a"));

    expect(keys(result)).toEqual(["c", "a", "b"]);
    expect(result.current.isDirty).toBe(true);
    expect(result.current.currentOrder()).toEqual(["c", "a", "b"]);
  });

  it("nudges one place at a time and clamps at both ends", () => {
    const { result } = renderHook(() =>
      useAttachmentOrder([stored("a"), stored("b"), stored("c")], []),
    );

    act(() => result.current.moveBy("a", -1));
    expect(keys(result)).toEqual(["a", "b", "c"]);
    expect(result.current.isDirty).toBe(false);

    act(() => result.current.moveBy("a", 1));
    expect(keys(result)).toEqual(["b", "a", "c"]);

    act(() => result.current.moveBy("c", 1));
    expect(keys(result)).toEqual(["b", "a", "c"]);
  });

  it("appends a newly picked file after an arranged order", () => {
    const { result, rerender } = renderHook(
      ({ attachments, uploads }) => useAttachmentOrder(attachments, uploads),
      {
        initialProps: {
          attachments: [stored("a"), stored("b")],
          uploads: [] as AttachmentUpload[],
        },
      },
    );

    act(() => result.current.move("b", "a"));
    expect(keys(result)).toEqual(["b", "a"]);

    rerender({ attachments: [stored("a"), stored("b")], uploads: [upload("u1")] });
    expect(keys(result)).toEqual(["b", "a", "u1"]);
  });

  it("drops a removed file from the arranged order without disturbing the rest", () => {
    const { result, rerender } = renderHook(
      ({ attachments }) => useAttachmentOrder(attachments, []),
      { initialProps: { attachments: [stored("a"), stored("b"), stored("c")] } },
    );

    act(() => result.current.move("c", "a"));
    expect(keys(result)).toEqual(["c", "a", "b"]);

    rerender({ attachments: [stored("a"), stored("b")] });
    expect(keys(result)).toEqual(["a", "b"]);
  });

  it("reset returns to the order the files arrived in", () => {
    const { result } = renderHook(() => useAttachmentOrder([stored("a"), stored("b")], []));

    act(() => result.current.move("b", "a"));
    expect(result.current.isDirty).toBe(true);

    act(() => result.current.reset());
    expect(keys(result)).toEqual(["a", "b"]);
    expect(result.current.isDirty).toBe(false);
  });
});
