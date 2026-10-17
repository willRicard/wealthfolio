import { expect, test } from "@playwright/test";
import { BASE_URL, completeOnboardingIfNeeded, gotoAppPath } from "./helpers";

for (const compact of [false, true]) {
  test(`Procurement item lifecycle (${compact ? "mobile" : "desktop"})`, async ({
    page,
  }, testInfo) => {
    test.setTimeout(180_000);
    await completeOnboardingIfNeeded(page);
    if (compact) await page.setViewportSize({ width: 390, height: 844 });
    await gotoAppPath(page, "/procurement");
    const name = `Procurement ${compact ? "mobile" : "desktop"}`;
    await page.getByRole("button", { name: "Add", exact: true }).click();
    let dialog = page.getByRole("dialog");
    await dialog.getByLabel("Name", { exact: true }).fill(name);
    await dialog.evaluate(async (element) => {
      await Promise.all(element.getAnimations().map((animation) => animation.finished));
    });
    const nameBounds = await dialog.getByLabel("Name", { exact: true }).boundingBox();
    const avatarBounds = await dialog
      .getByRole("button", { name: "Photo", exact: true })
      .boundingBox();
    expect(avatarBounds!.x + avatarBounds!.width).toBeLessThanOrEqual(nameBounds!.x);
    expect(
      Math.abs(avatarBounds!.y + avatarBounds!.height / 2 - nameBounds!.y - nameBounds!.height / 2),
    ).toBeLessThanOrEqual(1);
    await dialog.getByLabel("Brand", { exact: true }).fill("Test brand");
    const taxonomyResponse = await page.request.get(
      `${BASE_URL}/api/v1/taxonomies/procurement_categories`,
    );
    expect(taxonomyResponse.ok()).toBeTruthy();
    const taxonomy = (await taxonomyResponse.json()) as {
      categories: { id: string; name: string }[];
    };
    const categoryLabel = taxonomy.categories.find(
      (category) => category.id === "cat_shopping",
    )!.name;
    const subcategoryLabel = taxonomy.categories.find(
      (category) => category.id === "cat_shopping_home",
    )!.name;
    await dialog.getByLabel("Category", { exact: true }).click();
    await page
      .getByRole("option", { name: `${categoryLabel} / ${subcategoryLabel}`, exact: true })
      .click();
    await dialog.getByLabel("Memo", { exact: true }).fill("Personal purchase notes");
    await dialog.getByRole("button", { name: "Advanced", exact: true }).click();
    await dialog.getByRole("button", { name: "Add alias", exact: true }).click();
    await dialog.getByRole("textbox", { name: "Aliases 1", exact: true }).fill("Alias one");
    await dialog.getByRole("textbox", { name: "Aliases 1", exact: true }).press("Enter");
    await dialog.getByRole("button", { name: "Add alias", exact: true }).click();
    await dialog.getByRole("textbox", { name: "Aliases 2", exact: true }).fill("Alias two");
    await dialog.getByRole("textbox", { name: "Aliases 2", exact: true }).press("Enter");
    const png = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
      "base64",
    );
    await dialog
      .locator("input[type=file]")
      .setInputFiles({ name: "photo.png", mimeType: "image/png", buffer: png });
    await expect(dialog.getByRole("img", { name, exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Save", exact: true })).toBeEnabled();
    await dialog.getByLabel("Name", { exact: true }).scrollIntoViewIfNeeded();
    await dialog.screenshot({ path: testInfo.outputPath("procurement-form.png") });
    await dialog.getByRole("button", { name: "Save", exact: true }).click();
    await expect(dialog).not.toBeVisible();
    let row = page
      .getByRole("row")
      .filter({ has: page.getByRole("button", { name, exact: true }) });
    await expect(row).toBeVisible();
    await page.getByRole("button", { name: "Category", exact: true }).click();
    await page.getByRole("option").filter({ hasText: categoryLabel }).click();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Subcategory", exact: true }).click();
    await page.getByRole("option").filter({ hasText: subcategoryLabel }).click();
    await page.keyboard.press("Escape");
    await expect(row).toBeVisible();
    await expect(page.getByRole("option")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Columns", exact: true })).toHaveCount(0);
    if (compact) {
      const rowBounds = await row.boundingBox();
      expect(rowBounds!.width).toBeLessThanOrEqual(390);
    }
    await page.screenshot({ path: testInfo.outputPath("procurement.png"), fullPage: true });
    await row.getByRole("button", { name, exact: true }).click();
    dialog = page.getByRole("dialog");
    const photo = dialog.getByRole("img", { name, exact: true });
    await expect(photo).toBeVisible();
    await dialog.evaluate(async (element) => {
      await Promise.all(element.getAnimations().map((animation) => animation.finished));
    });
    const photoStyle = await photo.evaluate((element) => ({
      width: element.clientWidth,
      height: element.clientHeight,
      radius: getComputedStyle(element).borderRadius,
    }));
    expect(photoStyle.width).toBe(photoStyle.height);
    expect(photoStyle.width).toBe(64);
    expect(parseFloat(photoStyle.radius)).toBeGreaterThanOrEqual(photoStyle.width / 2);
    const taglineBounds = await dialog
      .getByText(`Test brand · ${subcategoryLabel}`, { exact: true })
      .boundingBox();
    const priceCardsBounds = await dialog.locator("dl").boundingBox();
    expect(priceCardsBounds!.y - taglineBounds!.y - taglineBounds!.height).toBeGreaterThanOrEqual(
      20,
    );
    if (compact) {
      const dialogBounds = await dialog.boundingBox();
      expect(priceCardsBounds!.x - dialogBounds!.x).toBeGreaterThanOrEqual(16);
      expect(
        dialogBounds!.x + dialogBounds!.width - priceCardsBounds!.x - priceCardsBounds!.width,
      ).toBeGreaterThanOrEqual(16);
    }
    await dialog.screenshot({ path: testInfo.outputPath("procurement-view.png") });
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await row.getByRole("button", { name: `Actions: ${name}`, exact: true }).click();
    await page.getByRole(compact ? "button" : "menuitem", { name: "Clone", exact: true }).click();
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(2);
    row = page
      .getByRole("row")
      .filter({ has: page.getByRole("button", { name, exact: true }) })
      .last();
    await row.getByRole("button", { name: `Actions: ${name}`, exact: true }).click();
    await page.getByRole(compact ? "button" : "menuitem", { name: "Edit", exact: true }).click();
    dialog = page.getByRole("dialog");
    await expect(dialog.getByLabel("Memo", { exact: true })).toHaveValue("Personal purchase notes");
    await dialog.evaluate(async (element) => {
      await Promise.all(element.getAnimations().map((animation) => animation.finished));
    });
    const editNameBounds = await dialog.getByLabel("Name", { exact: true }).boundingBox();
    const editAvatarBounds = await dialog
      .getByRole("button", { name: "Photo", exact: true })
      .boundingBox();
    expect(
      Math.abs(
        editAvatarBounds!.y +
          editAvatarBounds!.height / 2 -
          editNameBounds!.y -
          editNameBounds!.height / 2,
      ),
    ).toBeLessThanOrEqual(1);
    await dialog.getByLabel("Name", { exact: true }).fill(`${name} edited`);
    await dialog.getByRole("button", { name: "Save", exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await page.getByPlaceholder("Search ...").fill("edited");
    await page.getByPlaceholder("Search ...").press("Enter");
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: `${name} edited`, exact: true })).toBeVisible();
    await page.getByRole("button", { name: `${name} edited`, exact: true }).click();
    dialog = page.getByRole("dialog");
    await expect(dialog.getByText("No purchases yet.")).toBeVisible();
    await expect(dialog.getByText("Personal purchase notes")).toBeVisible();
    await expect(dialog.getByLabel("Aliases", { exact: true })).toHaveCount(0);
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByPlaceholder("Search ...").fill("");
    await page.getByPlaceholder("Search ...").press("Enter");
    await page.getByRole("checkbox", { name: `Select ${name}`, exact: true }).check();
    await page.getByRole("checkbox", { name: `Select ${name} edited`, exact: true }).check();
    const selectionBar = page.getByRole("region", { name: "Bulk actions", exact: true });
    await expect(selectionBar).toContainText("2 selected");
    await selectionBar.getByRole("button", { name: "Clear", exact: true }).click();
    await expect(selectionBar).not.toBeVisible();
    await page.getByRole("checkbox", { name: `Select ${name}`, exact: true }).check();
    await page.getByRole("checkbox", { name: `Select ${name} edited`, exact: true }).check();
    await selectionBar.getByRole("button", { name: "Delete", exact: true }).click();
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Delete", exact: true })
      .click();
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: `${name} edited`, exact: true })).toHaveCount(0);
  });
}

test("Procurement prices follow source activity edits and deletion", async ({ page }) => {
  test.setTimeout(180_000);
  await completeOnboardingIfNeeded(page);
  const api = `${BASE_URL}/api/v1`;
  const settingsResponse = await page.request.get(`${api}/settings`);
  expect(settingsResponse.ok()).toBeTruthy();
  const { baseCurrency: currency } = (await settingsResponse.json()) as { baseCurrency: string };
  const itemResponse = await page.request.post(`${api}/procurement/items`, {
    data: {
      name: "Price history item",
      brand: null,
      categoryId: "cat_groceries",
      subcategoryId: null,
      rating: null,
      aliases: [],
    },
  });
  expect(itemResponse.ok()).toBeTruthy();
  const item = (await itemResponse.json()) as { id: string };
  const accountResponse = await page.request.post(`${api}/accounts`, {
    data: {
      name: "Procurement prices",
      accountType: "CASH",
      currency,
      isDefault: false,
      isActive: true,
      trackingMode: "TRANSACTIONS",
    },
  });
  expect(accountResponse.ok()).toBeTruthy();
  const account = (await accountResponse.json()) as { id: string };
  const activityInput = {
    accountId: account.id,
    activityType: "WITHDRAWAL",
    activityDate: "2026-01-02T12:00:00Z",
    currency,
    amount: "25",
    fee: "0",
    needsReview: false,
    notes: "Shop A",
  };
  const activityResponse = await page.request.post(`${api}/activities`, { data: activityInput });
  expect(activityResponse.ok()).toBeTruthy();
  const activity = (await activityResponse.json()) as { id: string };
  const purchaseResponse = await page.request.post(`${api}/procurement/purchases`, {
    data: {
      itemId: item.id,
      activityId: activity.id,
      quantity: "2",
      unitPrice: "12.50",
    },
  });
  expect(purchaseResponse.ok()).toBeTruthy();
  expect(await purchaseResponse.json()).not.toHaveProperty("id");
  const purchasePath = `${api}/procurement/purchases/${encodeURIComponent(activity.id)}/${encodeURIComponent(item.id)}`;
  const purchaseInput = {
    activityId: activity.id,
    itemId: item.id,
    quantity: "3",
    unitPrice: "12.50",
  };
  const purchaseUpdate = await page.request.put(purchasePath, { data: purchaseInput });
  expect(purchaseUpdate.ok()).toBeTruthy();
  expect(await purchaseUpdate.json()).toMatchObject({
    activityId: activity.id,
    itemId: item.id,
    quantity: "3",
  });
  const mismatchedPurchase = await page.request.put(purchasePath, {
    data: { ...purchaseInput, itemId: "another-item" },
  });
  expect(mismatchedPurchase.status()).toBe(400);
  await gotoAppPath(page, "/procurement");
  let row = page
    .getByRole("row")
    .filter({ has: page.getByRole("button", { name: "Price history item", exact: true }) });
  await expect(row).toContainText("12.50");
  await row.getByRole("button", { name: "Price history item", exact: true }).click();
  await expect(page.getByRole("dialog").locator(".recharts-area-dot")).toHaveCount(1);
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
  const updateResponse = await page.request.put(`${api}/activities`, {
    data: {
      ...activityInput,
      id: activity.id,
      activityDate: "2026-02-02T12:00:00Z",
      notes: "Shop B",
    },
  });
  expect(updateResponse.ok()).toBeTruthy();
  const pricesResponse = await page.request.get(`${api}/procurement/items/${item.id}/prices`);
  expect(pricesResponse.ok()).toBeTruthy();
  const prices = (await pricesResponse.json()) as { prices: { date: string; shop: string }[] };
  expect(prices.prices[0].shop).toBe("Shop B");
  expect(prices.prices[0].date).toContain("2026-02-02");
  const purchaseDelete = await page.request.delete(purchasePath);
  expect(purchaseDelete.ok()).toBeTruthy();
  const missingPurchase = await page.request.delete(purchasePath);
  expect(missingPurchase.status()).toBe(404);
  const restoredPurchase = await page.request.post(`${api}/procurement/purchases`, {
    data: purchaseInput,
  });
  expect(restoredPurchase.ok()).toBeTruthy();
  const deleteResponse = await page.request.delete(`${api}/activities/${activity.id}`);
  expect(deleteResponse.ok()).toBeTruthy();
  await page.reload();
  row = page
    .getByRole("row")
    .filter({ has: page.getByRole("button", { name: "Price history item", exact: true }) });
  await expect(row).not.toContainText("12.50");
  await row.getByRole("button", { name: "Price history item", exact: true }).click();
  await expect(page.getByRole("dialog").getByText("No purchases yet.")).toBeVisible();
});
