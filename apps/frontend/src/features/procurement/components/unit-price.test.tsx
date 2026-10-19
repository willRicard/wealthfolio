import { render, screen } from "@/test/render";
import { expect, it } from "vitest";
import { UnitPrice } from "./unit-price";

it("reads as a price per unit", () => {
  render(<UnitPrice value="400" currency="JPY" unit="kg" />);
  expect(screen.getByText("¥400")).toBeInTheDocument();
  expect(screen.getByText("/kg")).toBeInTheDocument();
});

it("draws a dash when the item has no price yet", () => {
  const { container } = render(<UnitPrice value={null} currency="JPY" unit="kg" />);
  expect(container).toHaveTextContent("—");
  expect(screen.queryByText("/kg")).toBeNull();
});

it("hides the amount but keeps the unit when balances are private", () => {
  render(<UnitPrice value="400" currency="JPY" unit="袋" isHidden />);
  expect(screen.getByText("••••")).toBeInTheDocument();
  expect(screen.getByText("/袋")).toBeInTheDocument();
  expect(screen.queryByText("¥400")).toBeNull();
});
