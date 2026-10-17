import { fireEvent, render } from "@/test/render";
import { StarRating } from "@wealthfolio/ui/components/ui/star-rating";
import { describe, expect, it, vi } from "vitest";

describe("procurement rating", () => {
  it("keeps whole-star input as the default and read-only ratings do not emit changes", () => {
    const change = vi.fn();
    const { container, rerender } = render(
      <StarRating rating={0} interactive onRatingChange={change} />,
    );
    fireEvent.click(container.firstElementChild!.children[2], { clientX: 0 });
    expect(change).toHaveBeenCalledWith(3);
    change.mockClear();
    rerender(<StarRating rating={3.5} onRatingChange={change} />);
    fireEvent.click(container.firstElementChild!.children[0]);
    expect(change).not.toHaveBeenCalled();
  });
});
