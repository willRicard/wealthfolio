import { describe, expect, it, vi } from "vitest";

vi.mock("@/adapters", () => ({
  logger: {
    error: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
  },
}));

import {
  addonDevManager,
  getDevelopmentRuntimePackageError,
  shouldReloadDevelopmentAddon,
} from "./addons-dev-mode";

describe("shouldReloadDevelopmentAddon", () => {
  it("waits for a newer completed package generation", () => {
    expect(shouldReloadDevelopmentAddon({ buildInProgress: true, generation: 2 }, 1)).toBe(false);
    expect(shouldReloadDevelopmentAddon({ buildInProgress: false, generation: 1 }, 1)).toBe(false);
    expect(shouldReloadDevelopmentAddon({ buildInProgress: false, generation: 2 }, 1)).toBe(true);
  });

  it("rejects missing and invalid generations", () => {
    expect(shouldReloadDevelopmentAddon({ buildInProgress: false }, 1)).toBe(false);
    expect(
      shouldReloadDevelopmentAddon({ buildInProgress: false, generation: Number.NaN }, 1),
    ).toBe(false);
  });
});

describe("getDevelopmentRuntimePackageError", () => {
  it.each([404, 405])(
    "explains how to upgrade an incompatible development server (%s)",
    (status) => {
      expect(getDevelopmentRuntimePackageError(status, "Not Found")).toBe(
        "Development server does not support Wealthfolio 3.7 runtime packages. " +
          "Upgrade @wealthfolio/addon-dev-tools to version 3.7.0 or newer.",
      );
    },
  );

  it("preserves the server detail for other failures", () => {
    expect(getDevelopmentRuntimePackageError(500, "Build failed")).toBe(
      "Failed to load development addon package: Build failed",
    );
  });
});

describe("development addon reloads", () => {
  it("coalesces overlapping reload requests for the same addon", async () => {
    const manager = addonDevManager as unknown as {
      devServers: Map<string, unknown>;
      fetchRuntimePackage: (server: unknown) => Promise<unknown>;
      reloadAddon: (addonId: string) => Promise<void>;
    };
    const addonId = "reload-coalescing-test";
    manager.devServers.set(addonId, {
      id: addonId,
      name: "Reload coalescing test",
      port: 3001,
      status: "running",
      url: "http://localhost:3001",
    });

    let rejectPackage!: (error: Error) => void;
    const pendingPackage = new Promise<never>((_resolve, reject) => {
      rejectPackage = reject;
    });
    const fetchRuntimePackage = vi
      .spyOn(manager, "fetchRuntimePackage")
      .mockReturnValue(pendingPackage);

    const firstReload = manager.reloadAddon(addonId);
    const overlappingReload = manager.reloadAddon(addonId);
    expect(fetchRuntimePackage).toHaveBeenCalledTimes(1);

    rejectPackage(new Error("test reload failure"));
    await Promise.all([firstReload, overlappingReload]);

    await manager.reloadAddon(addonId);
    expect(fetchRuntimePackage).toHaveBeenCalledTimes(2);

    fetchRuntimePackage.mockRestore();
    manager.devServers.delete(addonId);
  });
});
