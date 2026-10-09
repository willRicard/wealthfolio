import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TransactionAttachments } from "./transaction-attachments";
import { transactionFileError, MAX_TRANSACTION_FILE_BYTES } from "../lib/transaction-files";

vi.mock("@/adapters", () => ({
  deleteTransactionAttachment: vi.fn(),
  transactionAttachmentThumbnailUrl: (activityId: string, id: string) =>
    Promise.resolve(`/files/${activityId}/${id}/thumbnail`),
  openTransactionAttachment: vi.fn(),
}));

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
          activityId="act-1"
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
          pendingFiles={[]}
          onFilesChange={vi.fn()}
        />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("button", { name: "Open original: receipt.pdf" })).toBeVisible();
    expect(container.querySelector("img")).toHaveAttribute("src", "/files/act-1/file-1/thumbnail");
    expect(container.querySelector("iframe, object, embed")).toBeNull();
  });
});
