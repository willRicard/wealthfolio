import { describe, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { render, screen, waitFor } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  deleteTransactionAttachment,
  fetchTransactionAttachmentThumbnail,
  openTransactionAttachment,
  type AttachmentTarget,
  type TransactionAttachment,
} from "@/adapters";
import { TransactionAttachments } from "./transaction-attachments";
import { transactionFileError, MAX_TRANSACTION_FILE_BYTES } from "../lib/transaction-files";
import { useAttachmentOrder } from "../hooks/use-attachment-order";
import type { AttachmentUpload, UseAttachmentUploads } from "../hooks/use-attachment-uploads";

vi.mock("@/adapters", () => ({
  deleteTransactionAttachment: vi.fn(),
  fetchTransactionAttachmentThumbnail: vi.fn(() =>
    Promise.resolve(new Blob([new Uint8Array([0])], { type: "image/webp" })),
  ),
  openTransactionAttachment: vi.fn(),
  SUPPORTS_UPLOAD_PROGRESS: true,
}));

const ACTIVITY = { kind: "activity", id: "act-1" } as const;

const noUploads = (uploads: AttachmentUpload[] = []): UseAttachmentUploads => ({
  uploads,
  isUploading: false,
  add: vi.fn(),
  retry: vi.fn(),
  cancel: vi.fn(),
  remove: vi.fn(),
  flush: vi.fn(),
  reset: vi.fn(),
  resolveAttachmentId: vi.fn(),
});

function Attachments({
  target = ACTIVITY as AttachmentTarget,
  attachments = [],
  uploads = noUploads(),
}: {
  target?: AttachmentTarget;
  attachments?: TransactionAttachment[];
  uploads?: UseAttachmentUploads;
}) {
  const order = useAttachmentOrder(attachments, uploads.uploads);
  return (
    <QueryClientProvider client={client}>
      <TransactionAttachments target={target} uploads={uploads} order={order} />
    </QueryClientProvider>
  );
}

let client = new QueryClient();

const attachment = (id: string, filename: string): TransactionAttachment => ({
  id,
  activityId: "act-1",
  filename,
  contentType: "image/png",
  sizeBytes: 10,
  createdAt: "2026-10-13",
});

const openable = () =>
  screen
    .getAllByRole("button", { name: /^Open original: / })
    .map((button) => button.getAttribute("aria-label")?.replace("Open original: ", ""));

describe("transaction attachments", () => {
  it("rejects unsupported, empty, oversized and excessive files", () => {
    const file = new File(["receipt"], "receipt.png", { type: "image/png" });
    expect(transactionFileError([file], 9)).toBeUndefined();
    expect(transactionFileError([file], 10)).toBe("fileCountError");
    expect(transactionFileError([new File([], "empty.png", { type: "image/png" })], 0)).toBe(
      "fileSizeError",
    );
    expect(
      transactionFileError([new File(["<svg/>"], "image.svg", { type: "image/svg+xml" })], 0),
    ).toBe("fileTypeError");
    Object.defineProperty(file, "size", { value: MAX_TRANSACTION_FILE_BYTES + 1 });
    expect(transactionFileError([file], 0)).toBe("fileSizeError");
  });
  it("loads the thumbnail without loading the original", async () => {
    client = new QueryClient();
    const { container } = render(
      <Attachments
        attachments={[
          {
            id: "file-1",
            activityId: "act-1",
            filename: "receipt.pdf",
            contentType: "application/pdf",
            sizeBytes: 2000,
            createdAt: "2026-10-09",
          },
        ]}
      />,
    );
    expect(await screen.findByRole("button", { name: "Open original: receipt.pdf" })).toBeVisible();
    await waitFor(() =>
      expect(container.querySelector("img")?.getAttribute("src")).toMatch(/^blob:/),
    );
    expect(container.querySelector("iframe, object, embed")).toBeNull();
  });
  it("shows measured progress for an upload in flight, and an error with a retry", async () => {
    client = new QueryClient();
    const uploads = noUploads([
      {
        id: "u1",
        file: new File(["a"], "sending.png", { type: "image/png" }),
        status: "uploading",
        progress: 0.42,
      },
      {
        id: "u2",
        file: new File(["b"], "broken.pdf", { type: "application/pdf" }),
        status: "failed",
        error: "Attachment request failed",
      },
    ]);
    render(<Attachments uploads={uploads} />);

    expect(screen.getByText("42%")).toBeVisible();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
    expect(screen.getByRole("alert")).toHaveTextContent("Attachment request failed");

    await userEvent.click(screen.getAllByRole("button", { name: "Cancel" })[0]);
    expect(uploads.cancel).toHaveBeenCalledWith("u1");
    await userEvent.click(screen.getAllByRole("button", { name: "Retry" })[0]);
    expect(uploads.retry).toHaveBeenCalledWith("u2");
    await userEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]);
    expect(uploads.remove).toHaveBeenCalledWith("u2");
  });
  it("routes every operation at a draft target to the draft", async () => {
    client = new QueryClient();
    const target = { kind: "draft", token: "draft-token" } as const;
    render(
      <Attachments
        target={target}
        attachments={[
          {
            id: "file-1",
            token: "draft-token",
            filename: "staged.png",
            contentType: "image/png",
            sizeBytes: 10,
            createdAt: "2026-10-13",
          } as unknown as TransactionAttachment,
        ]}
      />,
    );

    await waitFor(() =>
      expect(fetchTransactionAttachmentThumbnail).toHaveBeenCalledWith(target, "file-1"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Open original: staged.png" }));
    expect(openTransactionAttachment).toHaveBeenCalledWith(
      target,
      expect.objectContaining({ id: "file-1" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(deleteTransactionAttachment).toHaveBeenCalledWith(target, "file-1"));
  });

  it("keeps an uploading tile in its slot when the upload completes", async () => {
    client = new QueryClient();
    const file = new File(["a"], "sending.png", { type: "image/png" });
    const { container, rerender } = render(
      <Attachments
        attachments={[attachment("file-1", "stored.png")]}
        uploads={noUploads([{ id: "u1", file, status: "uploading", progress: 0.4 }])}
      />,
    );
    const slots = () => Array.from(container.querySelectorAll(".grid > div"));
    expect(slots()).toHaveLength(2);
    const uploading = slots()[1];
    expect(uploading).toHaveTextContent("sending.png");

    rerender(
      <Attachments
        attachments={[attachment("file-1", "stored.png")]}
        uploads={noUploads([{ id: "u1", file, status: "done", attachmentId: "file-2" }])}
      />,
    );
    expect(slots()).toHaveLength(2);
    expect(slots()[1]).toBe(uploading);

    rerender(
      <Attachments
        attachments={[attachment("file-1", "stored.png"), attachment("file-2", "sending.png")]}
        uploads={noUploads([{ id: "u1", file, status: "done", attachmentId: "file-2" }])}
      />,
    );
    expect(slots()).toHaveLength(2);
    expect(slots()[1]).toBe(uploading);
    await waitFor(() => expect(openable()).toEqual(["stored.png", "sending.png"]));
  });

  it("reorders with the chevrons, which are disabled at each end", async () => {
    client = new QueryClient();
    render(
      <Attachments
        attachments={[
          attachment("file-1", "first.png"),
          attachment("file-2", "second.png"),
          attachment("file-3", "third.png"),
        ]}
      />,
    );
    await waitFor(() => expect(openable()).toEqual(["first.png", "second.png", "third.png"]));

    expect(screen.getByRole("button", { name: "Move first.png left" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move third.png right" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Move third.png left" }));
    expect(openable()).toEqual(["first.png", "third.png", "second.png"]);

    await userEvent.click(screen.getByRole("button", { name: "Move first.png right" }));
    expect(openable()).toEqual(["third.png", "first.png", "second.png"]);

    expect(screen.getByRole("button", { name: "Move third.png left" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move second.png right" })).toBeDisabled();
  });

  it("offers no chevrons for a single file", async () => {
    client = new QueryClient();
    render(<Attachments attachments={[attachment("file-1", "only.png")]} />);

    await waitFor(() => expect(openable()).toEqual(["only.png"]));
    expect(screen.queryByRole("button", { name: /^Move / })).toBeNull();
  });
});
