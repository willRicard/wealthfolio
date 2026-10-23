import { useState } from "react";
import { act, fireEvent, render, screen, waitFor } from "@/test/render";
import { afterEach, expect, it, vi } from "vitest";
import { ItemForm } from "./item-form";
import type { ProcurementItem } from "../types";

const mocks = vi.hoisted(() => ({ upload: vi.fn(), discard: vi.fn() }));
vi.mock("@/adapters", () => ({
  uploadProcurementPhotoDraft: mocks.upload,
  discardProcurementPhotoDraft: mocks.discard,
  createProcurementItem: vi.fn(),
  updateProcurementItem: vi.fn(),
}));
vi.mock("@/hooks/use-taxonomies", () => ({ useTaxonomy: () => ({ data: { categories: [] } }) }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("./item-photo", () => ({ ItemPhoto: () => null }));

afterEach(() => vi.unstubAllGlobals());

it("discards a replacement photo that finishes while Cancel is discarding the old draft", async () => {
  const nativeUrl = URL;
  vi.stubGlobal(
    "URL",
    class extends nativeUrl {
      static override createObjectURL() {
        return "blob:test-photo";
      }
      static override revokeObjectURL() {
        /* No real blob URL is created in this test. */
      }
    },
  );
  let finishReselection!: () => void;
  let finishCancellation!: () => void;
  const reselect = new Promise<void>((resolve) => {
    finishReselection = resolve;
  });
  const cancel = new Promise<void>((resolve) => {
    finishCancellation = resolve;
  });
  mocks.upload
    .mockResolvedValueOnce({ id: "old-draft" })
    .mockResolvedValueOnce({ id: "new-draft" });
  mocks.discard
    .mockResolvedValue(undefined)
    .mockImplementationOnce(() => reselect)
    .mockImplementationOnce(() => cancel);
  function Harness() {
    const [open, setOpen] = useState(true);
    return open ? <ItemForm onClose={() => setOpen(false)} onSaved={() => setOpen(false)} /> : null;
  }
  render(<Harness />);
  const input = screen.getByLabelText("photo", { selector: "input" });
  const file = new File(["photo"], "photo.png", { type: "image/png" });
  fireEvent.change(input, { target: { files: [file] } });
  await waitFor(() => expect(input).toBeEnabled());
  expect(mocks.upload).toHaveBeenCalledTimes(1);
  fireEvent.change(input, { target: { files: [file] } });
  await waitFor(() => expect(mocks.discard).toHaveBeenCalledTimes(1));
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  await waitFor(() => expect(mocks.discard).toHaveBeenCalledTimes(2));
  await act(async () => {
    finishReselection();
    await reselect;
  });
  await waitFor(() => expect(input).toBeEnabled());
  expect(mocks.upload).toHaveBeenCalledTimes(2);
  await act(async () => {
    finishCancellation();
    await cancel;
  });
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(mocks.discard).toHaveBeenCalledWith("new-draft");
});

it("defaults a new item to 個 and keeps a saved unit when editing", () => {
  const { unmount } = render(<ItemForm onClose={vi.fn()} onSaved={vi.fn()} />);
  expect(screen.getByRole("combobox", { name: "unit" })).toHaveTextContent("個");
  expect(screen.getByRole("combobox", { name: "taxType" })).toHaveTextContent("taxIncluded");
  unmount();
  render(
    <ItemForm
      item={
        {
          id: "coffee",
          name: "Coffee",
          brand: null,
          memo: null,
          categoryId: "cat_groceries",
          subcategoryId: null,
          rating: null,
          unit: "kg",
          taxType: "EXCLUDED_10",
          aliases: [],
          photo: null,
          createdAt: "2026-10-01",
          updatedAt: "2026-10-01",
        } as ProcurementItem
      }
      onClose={vi.fn()}
      onSaved={vi.fn()}
    />,
  );
  expect(screen.getByRole("combobox", { name: "unit" })).toHaveTextContent("kg");
  expect(screen.getByRole("combobox", { name: "taxType" })).toHaveTextContent("taxExcluded10");
});

it("uploads a dropped photo immediately and shows a spinner until the upload finishes", async () => {
  mocks.upload.mockReset();
  let rejectUpload!: (error: Error) => void;
  mocks.upload.mockImplementationOnce(
    () =>
      new Promise((_, reject) => {
        rejectUpload = reject;
      }),
  );
  render(<ItemForm onClose={vi.fn()} onSaved={vi.fn()} />);
  const photo = screen.getByRole("button", { name: "photo" });
  const file = new File(["photo"], "photo.png", { type: "image/png" });
  fireEvent.drop(photo, { dataTransfer: { files: [file] } });
  expect(mocks.upload).toHaveBeenCalledWith(file);
  expect(screen.getByRole("status", { name: "uploading" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
  await act(() => {
    rejectUpload(new Error("Upload failed"));
    return Promise.resolve();
  });
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("uploadFailed"));
  expect(screen.queryByRole("status", { name: "uploading" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "save" })).toBeEnabled();
});
