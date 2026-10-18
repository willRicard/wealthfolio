import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@/test/render";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { fetchProcurementPhoto } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItem } from "../types";
import { ItemPhoto } from "./item-photo";

vi.mock("@/adapters", () => ({ fetchProcurementPhoto: vi.fn() }));
const decode = vi.fn<() => Promise<void>>();
beforeEach(() => {
  decode.mockReset().mockResolvedValue(undefined);
  vi.stubGlobal(
    "Image",
    class {
      src = "";
      decode = decode;
    },
  );
  vi.mocked(fetchProcurementPhoto)
    .mockReset()
    .mockResolvedValue(new Blob(["photo"], { type: "image/webp" }));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const item = {
  id: "item",
  name: "Coffee",
  photo: { key: "photo-1" },
} as ProcurementItem;
const key = [QueryKeys.PROCUREMENT_ITEM_PHOTO, item.id, item.photo?.key];

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = (photoItem = item) => (
    <QueryClientProvider client={client}>
      <ItemPhoto item={photoItem} />
    </QueryClientProvider>
  );
  return { client, view };
}

it("renders the same ready image on the first commit in another dialog, then evicts it after the default GC time", async () => {
  const create = vi.spyOn(URL, "createObjectURL");
  const { client, view } = setup();
  const first = render(view());
  const image = await screen.findByRole("img", { name: "Coffee" });
  const src = image.getAttribute("src");
  expect(src).toMatch(/^data:image\/webp;base64,/);
  expect(decode).toHaveBeenCalledTimes(1);
  vi.useFakeTimers();
  first.unmount();
  await act(() => vi.advanceTimersByTimeAsync(2 * 60 * 1000));
  vi.useRealTimers();
  const second = render(view());
  expect(screen.getByRole("img", { name: "Coffee" })).toHaveAttribute("src", src);
  expect(fetchProcurementPhoto).toHaveBeenCalledTimes(1);
  expect(decode).toHaveBeenCalledTimes(1);
  expect(create).not.toHaveBeenCalled();
  vi.useFakeTimers();
  second.unmount();
  await act(() => vi.advanceTimersByTimeAsync(6 * 60 * 1000));
  vi.useRealTimers();
  expect(client.getQueryData(key)).toBeUndefined();
  const third = render(view());
  await screen.findByRole("img", { name: "Coffee" });
  expect(fetchProcurementPhoto).toHaveBeenCalledTimes(2);
  third.unmount();
  client.clear();
});

it("waits for decoding before publishing the thumbnail", async () => {
  let finish!: () => void;
  decode.mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finish = resolve;
    }),
  );
  const { client, view } = setup();
  const result = render(view());
  await waitFor(() => expect(decode).toHaveBeenCalled());
  expect(client.getQueryData(key)).toBeUndefined();
  expect(screen.queryByRole("img")).toBeNull();
  act(() => finish());
  await screen.findByRole("img", { name: "Coffee" });
  result.unmount();
  client.clear();
});

it("falls back on decode and display errors, and loads a changed photo key", async () => {
  decode.mockRejectedValueOnce(new Error("Invalid image"));
  const { client, view } = setup();
  const result = render(view());
  await waitFor(() => expect(client.getQueryState(key)?.status).toBe("error"));
  expect(screen.queryByRole("img")).toBeNull();
  result.rerender(view({ ...item, photo: { ...item.photo!, key: "photo-2" } }));
  const image = await screen.findByRole("img", { name: "Coffee" });
  fireEvent.error(image);
  expect(screen.queryByRole("img")).toBeNull();
  expect(fetchProcurementPhoto).toHaveBeenCalledTimes(2);
  result.rerender(view({ ...item, photo: null }));
  expect(fetchProcurementPhoto).toHaveBeenCalledTimes(2);
  result.unmount();
  client.clear();
});

it("does not fetch items without photos", () => {
  const { client, view } = setup();
  const result = render(view({ ...item, photo: null }));
  expect(fetchProcurementPhoto).not.toHaveBeenCalled();
  expect(screen.queryByRole("img")).toBeNull();
  result.unmount();
  client.clear();
});

it.each(["fetch", "read"])("shows the fallback when the thumbnail %s fails", async (failure) => {
  if (failure === "fetch") {
    vi.mocked(fetchProcurementPhoto).mockRejectedValueOnce(new Error("Unavailable"));
  } else {
    vi.stubGlobal(
      "FileReader",
      class {
        error = new Error("Unreadable thumbnail");
        onerror?: () => void;
        readAsDataURL() {
          this.onerror?.();
        }
      },
    );
  }
  const { client, view } = setup();
  const result = render(view());
  await waitFor(() => expect(client.getQueryState(key)?.status).toBe("error"));
  expect(screen.queryByRole("img")).toBeNull();
  expect(decode).not.toHaveBeenCalled();
  result.unmount();
  client.clear();
});
