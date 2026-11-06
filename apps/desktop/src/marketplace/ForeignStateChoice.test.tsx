import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { ForeignStateChoice } from "./ReviewDialog";

const render = (keep: boolean) =>
  renderToStaticMarkup(<ForeignStateChoice keep={keep} disabled={false} onChange={() => {}} />);

const radios = (html: string) => html.match(/<input[^>]*type="radio"[^>]*>/g) ?? [];

describe("saved progress from another source (decision 18.6)", () => {
  it("is a labelled group of two radios with delete selected", () => {
    const html = render(false);
    expect(html).toContain("<fieldset");
    expect(html).toContain("<legend");
    const [del, keep] = radios(html);
    expect(radios(html)).toHaveLength(2);
    expect(del).toContain("checked");
    expect(keep).not.toContain("checked");
    expect(html.indexOf("Delete that saved progress")).toBeLessThan(
      html.indexOf("Keep it for this install"),
    );
    expect(html).not.toContain("trust both sources");
  });

  it("warns once keep is chosen and ties the warning to the radio", () => {
    const html = render(true);
    const [del, keep] = radios(html);
    expect(del).not.toContain("checked");
    expect(keep).toContain("checked");
    expect(keep).toContain('aria-describedby="review-saved-warning"');
    expect(html).toContain('id="review-saved-warning"');
    expect(html).toContain("trust both sources");
  });
});
