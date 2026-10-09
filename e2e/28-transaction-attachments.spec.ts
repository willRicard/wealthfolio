import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { BASE_URL, completeOnboardingIfNeeded, gotoAppPath } from "./helpers";

const receipt = fileURLToPath(
  new URL("./fixtures/transaction-attachments/receipt.png", import.meta.url),
);
const pdfReceipt = fileURLToPath(
  new URL("./fixtures/transaction-attachments/receipt.pdf", import.meta.url),
);

test("Spending saves detailed notes and displays attachment thumbnails", async ({
  page,
}, testInfo) => {
  test.setTimeout(180000);
  await completeOnboardingIfNeeded(page);
  const accountResponse = await page.request.post(`${BASE_URL}/api/v1/accounts`, {
    data: {
      name: "Receipt account",
      accountType: "CASH",
      currency: "USD",
      isActive: true,
      isDefault: false,
    },
  });
  expect(accountResponse.ok()).toBe(true);
  const account = (await accountResponse.json()) as { id: string };
  const settingsResponse = await page.request.put(`${BASE_URL}/api/v1/spending/settings`, {
    data: { enabled: true, accountIds: [account.id] },
  });
  expect(settingsResponse.ok()).toBe(true);
  const activityResponse = await page.request.post(`${BASE_URL}/api/v1/activities`, {
    data: {
      accountId: account.id,
      activityType: "WITHDRAWAL",
      activityDate: new Date().toISOString(),
      amount: 42,
      currency: "USD",
      comment: "Receipt test payee",
    },
  });
  expect(activityResponse.ok()).toBe(true);
  const activity = (await activityResponse.json()) as { id: string };
  await gotoAppPath(page, "/activities?tab=spending");
  const row = page.getByRole("row").filter({ hasText: "Receipt test payee" });
  await expect(row).toBeVisible({ timeout: 20000 });
  await row.getByRole("button", { name: "Row actions" }).click();
  await page.getByRole("menuitem", { name: "Edit", exact: true }).click();
  const panel = page.getByRole("dialog", { name: "Edit Transaction" });
  await expect(panel).toBeVisible();
  await expect(panel.getByLabel("Detailed notes")).toBeEnabled();
  await panel.getByLabel("Detailed notes").fill("Line items and receipt details");
  await panel.getByLabel("Attachments", { exact: true }).setInputFiles([receipt, pdfReceipt]);
  const requests: string[] = [];
  page.context().on("request", (request) => {
    if (request.url().includes("/attachments/")) requests.push(request.url());
  });
  await panel.getByRole("button", { name: "Update", exact: true }).click();
  await expect(panel).not.toBeVisible({ timeout: 30000 });
  await row.getByRole("button", { name: "Row actions" }).click();
  await page.getByRole("menuitem", { name: "Edit", exact: true }).click();
  await expect(panel.getByLabel("Detailed notes")).toHaveValue("Line items and receipt details");
  const previews = panel.locator("img[src*='/thumbnail']");
  await expect(previews).toHaveCount(2);
  for (const image of await previews.all()) {
    await expect
      .poll(() => image.evaluate((element: HTMLImageElement) => element.naturalWidth))
      .toBeGreaterThan(0);
  }
  expect(requests.filter((url) => !url.includes("/thumbnail"))).toEqual([]);
  await panel.getByLabel("Attachments", { exact: true }).scrollIntoViewIfNeeded();
  await panel.screenshot({ path: testInfo.outputPath("transaction-attachments.png") });
  const [original] = await Promise.all([
    page.waitForEvent("popup"),
    panel.getByRole("button", { name: "Open original: receipt.png" }).click(),
  ]);
  await original.waitForURL(/\/attachments\//);
  await expect
    .poll(() => original.locator("img").evaluate((image: HTMLImageElement) => image.naturalWidth))
    .toBe(941);
  expect(original.url()).toContain("/attachments/");
  expect(original.url()).not.toContain("/thumbnail");
  await original.screenshot({ path: testInfo.outputPath("original-image.png") });
  await original.close();
  const attachmentsResponse = await page.request.get(
    `${BASE_URL}/api/v1/spending/transactions/${activity.id}/attachments`,
  );
  expect((await attachmentsResponse.json()) as { id: string }[]).toHaveLength(2);
  const storedActivityResponse = await page.request.post(`${BASE_URL}/api/v1/activities/search`, {
    data: { page: 0, pageSize: 10, activityIdFilter: [activity.id] },
  });
  const storedActivity = (
    (await storedActivityResponse.json()) as {
      data: { comment: string; detailedNotes: string }[];
    }
  ).data[0];
  expect(storedActivity.comment).toBe("Receipt test payee");
  expect(storedActivity.detailedNotes).toBe("Line items and receipt details");
  await panel.getByRole("button", { name: "Remove", exact: true }).first().click();
  await expect(previews).toHaveCount(1);
});
