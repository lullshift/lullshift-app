import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Dialog } from "./Dialog";
import { SkipLink, skipTo } from "./SkipLink";

const render = (description?: string) =>
  renderToStaticMarkup(
    <Dialog
      open={false}
      title="Disconnect?"
      onClose={() => undefined}
      actions={null}
      description={description && <p>{description}</p>}
    >
      <p>body</p>
    </Dialog>,
  );

describe("Dialog description", () => {
  it("links the explanation with aria-describedby", () => {
    const html = render("Lullshift removes its hooks.");
    const ref = /aria-describedby="([^"]+)"/.exec(html);
    expect(ref).not.toBeNull();
    expect(html).toContain(`id="${ref![1]}"`);
    expect(html.indexOf(`id="${ref![1]}"`)).toBeGreaterThan(html.indexOf("<h2"));
    expect(html).toMatch(/aria-labelledby="/);
  });
  it("sets no dangling reference without an explanation", () => {
    expect(render()).not.toContain("aria-describedby");
  });
});

describe("skip link", () => {
  it("renders an in-page link to the main landmark", () => {
    const html = renderToStaticMarkup(
      <SkipLink targetId="main-content" targetRef={{ current: null }} />,
    );
    expect(html).toContain('href="#main-content"');
    expect(html).toContain("Skip to main content");
  });
  it("moves focus to the target, and copes with none", () => {
    let focused = 0;
    expect(skipTo({ focus: () => focused++ })).toBe(true);
    expect(focused).toBe(1);
    expect(skipTo(null)).toBe(false);
  });
});
