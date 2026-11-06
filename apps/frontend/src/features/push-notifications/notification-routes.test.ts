import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { matchPath } from "react-router-dom";
import { expect, it } from "vitest";
import { buildCashflowUrl } from "@/features/spending/lib/navigation";

const currentDir = path.dirname(fileURLToPath(import.meta.url));

it("routes server notification links to registered screens", () => {
  const worker = readFileSync(
    path.resolve(currentDir, "../../../../server/src/push/worker.rs"),
    "utf8",
  );
  const routes = readFileSync(path.resolve(currentDir, "../../app-screen-routes.tsx"), "utf8");
  const paths = [...routes.matchAll(/<Route\s+path="([^"]+)"/g)]
    .filter((match) => !match[1].includes("*"))
    .map((match) => `/${match[1]}`);
  const links = [...worker.matchAll(/url: (?:format!\()?"([^"]+)"/g)].map((match) =>
    match[1].replace("{}", "2026-11"),
  );
  expect(links).toHaveLength(3);
  for (const link of links) {
    const url = new URL(link, "https://wealthfolio.example");
    expect(
      paths.some((path) => matchPath(path, url.pathname)),
      link,
    ).toBe(true);
  }
  expect(links.filter((link) => link === buildCashflowUrl({}))).toHaveLength(2);
});
