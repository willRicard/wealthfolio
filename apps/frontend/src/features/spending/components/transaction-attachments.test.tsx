import { describe, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { render, screen, waitFor } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  deleteTransactionAttachment,
  fetchTransactionAttachmentThumbnail,
  openTransactionAttachment,
} from "@/adapters";
import { TransactionAttachments } from "./transaction-attachments";
import { transactionFileError, MAX_TRANSACTION_FILE_BYTES } from "../lib/transaction-files";
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
});

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
    const queryClient = new QueryClient();
    const { container } = render(
      <QueryClientProvider client={queryClient}>
        <TransactionAttachments
          target={ACTIVITY}
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
          uploads={noUploads()}
        />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("button", { name: "Open original: receipt.pdf" })).toBeVisible();
    await waitFor(() =>
      expect(container.querySelector("img")?.getAttribute("src")).toMatch(/^blob:/),
    );
    expect(container.querySelector("iframe, object, embed")).toBeNull();
  });
  it("shows measured progress for an upload in flight, and an error with a retry", async () => {
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
    render(
      <QueryClientProvider client={new QueryClient()}>
        <TransactionAttachments target={ACTIVITY} attachments={[]} uploads={uploads} />
      </QueryClientProvider>,
    );

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
    const target = { kind: "draft", token: "draft-token" } as const;
    render(
      <QueryClientProvider client={new QueryClient()}>
        <TransactionAttachments
          target={target}
          attachments={[
            {
              id: "file-1",
              token: "draft-token",
              filename: "staged.png",
              contentType: "image/png",
              sizeBytes: 10,
              createdAt: "2026-10-13",
            },
          ]}
          uploads={noUploads()}
        />
      </QueryClientProvider>,
    );

    await waitFor(() =>
      expect(fetchTransactionAttachmentThumbnail).toHaveBeenCalledWith(target, "file-1"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Open original: staged.png" }));
    expect(openTransactionAttachment).toHaveBeenCalledWith(target, expect.objectContaining({ id: "file-1" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() =>
      expect(deleteTransactionAttachment).toHaveBeenCalledWith(target, "file-1"),
    );
  });
});
