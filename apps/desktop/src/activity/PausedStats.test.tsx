import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PausedStats } from "./PausedStats";

const render = (summary: { label: string; value: string | number }[] | null) =>
  renderToStaticMarkup(
    <PausedStats name="Moss Paths" summary={summary} playedNow={125} preview={null} />,
  );

describe("paused view stats", () => {
  it("keeps host stats and activity stats in separate groups", () => {
    const html = render([{ label: "Played", value: 99 }]);
    const [host, own] = html.split("<section");
    expect(host).toContain("Played this time");
    expect(host).not.toContain("In total");
    expect(host).not.toContain(">Played<");
    expect(own).toContain(">Played<");
    expect(own).not.toContain("In total");
  });
  it("never shows a running total", () => {
    expect(render(null)).not.toMatch(/total/i);
  });
  it("heads the activity group with the activity's name and labels it", () => {
    const html = render([{ label: "Moves", value: 12 }]);
    const heading = /<h3 id="([^"]+)"[^>]*>From Moss Paths<\/h3>/.exec(html);
    expect(heading).not.toBeNull();
    expect(html).toContain(`aria-labelledby="${heading![1]}"`);
    expect(html).toContain("<dt>Moves</dt>");
  });
  it("shows no activity group when it reported nothing", () => {
    expect(render(null)).not.toContain("From Moss Paths");
    expect(render([])).not.toContain("From Moss Paths");
  });
});
