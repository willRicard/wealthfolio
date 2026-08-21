import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SandboxTickerAvatar } from "./sandbox-ticker-avatar";

describe("SandboxTickerAvatar", () => {
  afterEach(() => {
    globalThis.__wealthfolioRequestTickerLogo = undefined;
    vi.restoreAllMocks();
    Reflect.deleteProperty(URL, "createObjectURL");
    Reflect.deleteProperty(URL, "revokeObjectURL");
  });

  it("tries the full symbol before the base-symbol fallback and revokes its object URL", async () => {
    const logo = new Blob(["png"], { type: "image/png" });
    const requestLogo = vi.fn().mockResolvedValueOnce(null).mockResolvedValueOnce(logo);
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: vi.fn(() => "blob:ticker-logo"),
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: vi.fn(() => undefined),
    });
    const createObjectURL = vi.mocked(URL.createObjectURL);
    const revokeObjectURL = vi.mocked(URL.revokeObjectURL);
    globalThis.__wealthfolioRequestTickerLogo = requestLogo;

    const view = render(<SandboxTickerAvatar symbol="SHOP.TO" />);
    await waitFor(() => expect(requestLogo).toHaveBeenCalledTimes(2));
    expect(requestLogo).toHaveBeenNthCalledWith(1, "SHOP.TO");
    expect(requestLogo).toHaveBeenNthCalledWith(2, "SHOP");
    expect(createObjectURL).toHaveBeenCalledWith(logo);

    view.unmount();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:ticker-logo");
  });

  it("keeps the initials fallback when no logo exists", async () => {
    globalThis.__wealthfolioRequestTickerLogo = vi.fn().mockResolvedValue(null);
    const view = render(<SandboxTickerAvatar symbol="MISS" />);

    await waitFor(() =>
      expect(globalThis.__wealthfolioRequestTickerLogo).toHaveBeenCalledWith("MISS"),
    );
    expect(view.getByText("MISS")).toBeInTheDocument();
    expect(view.container.querySelector("img")).toBeNull();
  });
});
