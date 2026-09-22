// Tests for markdown.ts (issue #15).
//
// Two of these groups are not about typography. Model output reaches this
// renderer having passed through a vault the player may not have written
// every line of, so "a link never becomes an element" and "raw HTML never
// executes" are the properties that matter most, and they are asserted
// against the rendered DOM rather than against the HTML string — the string
// is what the parser said, the DOM is what the player would get.

import { describe, expect, it } from "vitest";
import { renderInlineMarkdown, renderMarkdown } from "../markdown";

/// The rendered row as a caller would mount it, so a query runs against the
/// same tree the transcript shows.
function mount(rendered: { nodes: DocumentFragment }): HTMLElement {
  const host = document.createElement("div");
  host.append(rendered.nodes);
  return host;
}

function html(text: string): string {
  return mount(renderMarkdown(text)).innerHTML;
}

describe("typography", () => {
  it("renders bold, italic and inline code", () => {
    const el = mount(renderMarkdown("She was **certain**, or *nearly* so. `toll`"));
    expect(el.querySelector("strong")?.textContent).toBe("certain");
    expect(el.querySelector("em")?.textContent).toBe("nearly");
    expect(el.querySelector("code")?.textContent).toBe("toll");
  });

  it("treats a single newline as a line break", () => {
    expect(html("one\ntwo")).toContain("<br>");
  });

  it("reports one paragraph as inline, so the row keeps its <p>", () => {
    expect(renderMarkdown("Just prose.").kind).toBe("inline");
  });

  it("reports structure as block, because it cannot live inside a <p>", () => {
    for (const source of ["- one\n- two", "> quoted", "| a |\n|---|\n| 1 |", "# heading"]) {
      expect(renderMarkdown(source).kind).toBe("block");
    }
  });

  it("flattens block syntax in a spoken line rather than honouring it", () => {
    const el = document.createElement("div");
    el.append(renderInlineMarkdown("- not a list, just a dash"));
    expect(el.querySelector("ul")).toBeNull();
    expect(el.textContent).toBe("- not a list, just a dash");
  });
});

describe("no network egress (migration_plan.md §2.3)", () => {
  it("renders a link's text and drops its URL entirely", () => {
    const el = mount(renderMarkdown("Ask about [the toll](https://example.test/toll)."));
    expect(el.querySelector("a")).toBeNull();
    expect(el.innerHTML).not.toContain("example.test");
    expect(el.textContent).toBe("Ask about the toll.");
  });

  it("renders an image as its alt text and never as an element", () => {
    const el = mount(renderMarkdown("![the mill](https://example.test/mill.png)"));
    expect(el.querySelector("img")).toBeNull();
    expect(el.innerHTML).not.toContain("example.test");
    expect(el.textContent).toContain("the mill");
  });

  it("drops a reference-style link's URL too", () => {
    const el = mount(renderMarkdown("See [the mill][1].\n\n[1]: https://example.test/mill"));
    expect(el.querySelector("a")).toBeNull();
    expect(el.innerHTML).not.toContain("example.test");
  });

  it("leaves a bare URL as inert text, not an element", () => {
    const el = mount(renderMarkdown("It was at https://example.test/mill, apparently."));
    expect(el.querySelector("a")).toBeNull();
    expect(el.textContent).toContain("https://example.test/mill");
  });
});

describe("untrusted markup", () => {
  it("never yields a script element", () => {
    const el = mount(renderMarkdown("<script>globalThis.pwned = true</script>"));
    expect(el.querySelector("script")).toBeNull();
    expect((globalThis as Record<string, unknown>).pwned).toBeUndefined();
  });

  // The string "onerror" does survive here, and should: the renderer escapes
  // raw HTML rather than dropping it, so a note containing an <img> tag shows
  // the player that it does. What must not survive is an element carrying it.
  // Asserting on the attribute rather than on the text is the difference
  // between testing the property and testing the spelling.
  it("escapes an event-handler into text instead of leaving it an attribute", () => {
    const el = mount(renderMarkdown('<img src=x onerror="globalThis.pwned = true">'));
    expect(el.querySelector("img")).toBeNull();
    for (const node of el.querySelectorAll("*")) {
      expect(node.getAttributeNames()).toEqual([]);
    }
    expect(el.textContent).toContain("onerror");
  });

  it("keeps no attributes at all on the tags it does allow", () => {
    const el = mount(renderMarkdown('<strong class="x" id="y" style="color:red">bold</strong>'));
    for (const node of el.querySelectorAll("*")) {
      expect(node.attributes.length).toBe(0);
    }
  });

  it("shows raw HTML as the characters it is, so a note's angle brackets are visible", () => {
    expect(mount(renderMarkdown("a <b>tag</b> in the note")).textContent).toBe(
      "a <b>tag</b> in the note",
    );
  });

  it("sanitises inline rendering on the same terms", () => {
    const el = document.createElement("div");
    el.append(renderInlineMarkdown('[x](javascript:alert(1)) <img src=x onerror=alert(1)>'));
    expect(el.querySelector("a")).toBeNull();
    expect(el.querySelector("img")).toBeNull();
    expect(el.innerHTML).not.toContain("javascript:");
  });
});

describe("quoted speech keeps its weight", () => {
  it("bolds a double-quoted phrase inside narration", () => {
    const el = mount(renderMarkdown('He leaned in. "Two coppers," he said.'));
    expect([...el.querySelectorAll("strong")].map((n) => n.textContent)).toContain(
      '"Two coppers,"',
    );
  });

  it("does not treat an apostrophe as a quote", () => {
    const el = mount(renderMarkdown("You're certain, and I'd not argue."));
    expect(el.querySelector("strong")).toBeNull();
  });

  it("leaves code spans alone", () => {
    const el = mount(renderMarkdown('Run `say "hello"` first.'));
    expect(el.querySelector("code strong")).toBeNull();
    expect(el.querySelector("code")?.textContent).toBe('say "hello"');
  });

  it("does not nest a second strong inside emphasis that already exists", () => {
    const el = mount(renderMarkdown('**He said "no" flatly.**'));
    expect(el.querySelector("strong strong")).toBeNull();
  });
});

describe("wiki-links", () => {
  it("shows the note's name, not the brackets", () => {
    expect(mount(renderMarkdown("Ask [[Bram Holt]] about it.")).textContent).toBe(
      "Ask Bram Holt about it.",
    );
  });

  it("prefers the label when one is given", () => {
    expect(mount(renderMarkdown("Ask [[bram_holt|the miller]].")).textContent).toBe(
      "Ask the miller.",
    );
  });

  it("does not turn one into a link", () => {
    expect(mount(renderMarkdown("[[Bram Holt]]")).querySelector("a")).toBeNull();
  });
});
