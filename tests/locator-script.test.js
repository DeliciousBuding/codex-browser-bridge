"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const source = fs.readFileSync(path.join(__dirname, "..", "src", "locator.js"), "utf8");

function fixture() {
  class Event { constructor(type, options) { this.type = type; Object.assign(this, options); } }
  class Input {}
  Object.defineProperty(Input.prototype, "value", {
    get() { return this.nativeValue || ""; },
    set(value) { this.nativeValue = value; },
  });
  const win = { innerWidth: 800, innerHeight: 600, Event, HTMLInputElement: Input,
    HTMLTextAreaElement: Input, getComputedStyle: (el) => el.style };
  function document() {
    const doc = { nodes: [], defaultView: win, activeElement: null,
      querySelectorAll() { return this.nodes; },
      getElementById(id) { return this.nodes.find((el) => el.attrs.id === id); },
      elementFromPoint() { return this.hit || this.nodes[0]; } };
    return doc;
  }
  const doc = document();
  function element(selector, options = {}, owner = doc) {
    const el = Object.assign(Object.create(Input.prototype), {
      tagName: "DIV", selector, ownerDocument: owner, attrs: {}, children: [],
      innerText: "", textContent: "", style: { display: "block", visibility: "visible", opacity: "1" },
      rect: { left: 10, top: 10, right: 110, bottom: 30, width: 100, height: 20 },
      events: [], type: "text", disabled: false, readOnly: false, clicks: 0,
      getAttribute(name) { return this.attrs[name] ?? null; },
      hasAttribute(name) { return name in this.attrs; },
      matches(query) { return query === ":disabled" ? this.disabled : query === "*" || query === this.selector; },
      querySelectorAll() { return this.children; },
      closest() { return null; },
      getRootNode() { return owner; },
      getBoundingClientRect() { return this.rect; },
      scrollIntoView() { this.scrolled = true; },
      contains(child) { return this.children.includes(child); },
      focus() { this.getRootNode().activeElement = this; },
      dispatchEvent(event) { this.events.push(event.type); },
      click() { this.clicks++; this.checked = !this.checked; },
    }, options);
    owner.nodes.push(el);
    return el;
  }
  function run(spec) {
    return JSON.parse(vm.runInNewContext(source.replace("__CODEX_LOCATOR_SPEC__", JSON.stringify(spec)), {
      document: doc, ...win,
    }, { timeout: 1000 }));
  }
  return { doc, document, element, run };
}

test("rendered and raw text reads preserve their distinct whitespace and hidden content", () => {
  const f = fixture();
  f.element("#text", { innerText: "A\n  B", textContent: "A\n  B\nHidden" });
  assert.equal(f.run({ selector: "#text", action: "inner_text" }).value, "A\n  B");
  assert.equal(f.run({ selector: "#text", action: "text_content" }).value, "A\n  B\nHidden");
  assert.deepEqual(f.run({ selector: "#text", action: "all_text_contents" }).values, ["A\n  B\nHidden"]);
});

test("test IDs default to exact while explicit substring matching remains available", () => {
  const f = fixture();
  f.element("#a", { attrs: { "data-testid": "save" } });
  f.element("#b", { attrs: { "data-testid": "save-draft" } });
  assert.equal(f.run({ test_id: "save", action: "count" }).count, 1);
  assert.equal(f.run({ test_id: "save", exact: false, action: "count" }).count, 2);
});

test("wait checks the selected nth and handles absent, hidden, detached, and strict matches", () => {
  const f = fixture();
  f.element(".item");
  f.element(".item", { style: { visibility: "hidden", display: "block", opacity: "1" } });
  assert.equal(f.run({ selector: ".item", nth: 1, action: "probe", wait_state: "visible" }).matched, false);
  assert.equal(f.run({ selector: ".item", nth: -1, action: "probe", wait_state: "hidden" }).matched, true);
  assert.equal(f.run({ selector: ".item", nth: 999, action: "probe", wait_state: "attached" }).matched, false);
  assert.equal(f.run({ selector: ".missing", action: "probe", wait_state: "detached" }).matched, true);
  assert.equal(f.run({ selector: ".missing", action: "probe", wait_state: "hidden" }).matched, true);
  assert.match(f.run({ selector: ".item", action: "probe" }).error, /2 elements/);
});

test("fill bypasses an own React tracker setter and emits input/change after native value update", () => {
  const f = fixture();
  const input = f.element("#input", { tagName: "INPUT" });
  let ownSetterCalls = 0;
  Object.defineProperty(input, "value", {
    get() { return this.nativeValue || ""; },
    set() { ownSetterCalls++; },
  });
  assert.equal(f.run({ selector: "#input", action: "fill", value: "new" }).value, "new");
  assert.equal(ownSetterCalls, 0);
  assert.equal(input.nativeValue, "new");
  assert.deepEqual(input.events, ["input", "change"]);
});

test("fill and checked mutations reject disabled and readonly elements without changing them", () => {
  for (const property of ["disabled", "readOnly"]) {
    const f = fixture();
    const input = f.element("#input", { tagName: "INPUT", [property]: true });
    assert.equal(f.run({ selector: "#input", action: "fill", value: "new" }).ok, false);
    assert.equal(input.nativeValue, undefined);
    assert.equal(input.events.length, 0);
    input.type = "checkbox";
    assert.equal(f.run({ selector: "#input", action: "set_checked", checked: true }).ok, false);
    assert.equal(input.clicks, 0);
  }
});

test("set_checked clicks only when needed and rejects a failed postcondition", () => {
  const f = fixture();
  const input = f.element("#box", { tagName: "INPUT", type: "checkbox", checked: false });
  assert.equal(f.run({ selector: "#box", action: "set_checked", checked: true }).value, true);
  assert.equal(input.clicks, 1);
  assert.equal(f.run({ selector: "#box", action: "set_checked", checked: true }).value, true);
  assert.equal(input.clicks, 1);
  input.click = () => {};
  assert.match(f.run({ selector: "#box", action: "set_checked", checked: false }).error, /requested checked state/);
});

test("descendant scopes, text filters, visibility, and last compose in the same resolver", () => {
  const f = fixture();
  const section = f.element("section");
  const first = f.element("button", { textContent: "save draft" });
  const last = f.element("button", { textContent: "save final" });
  section.children = [first, last];
  assert.equal(f.run({ scopes: [{ selector: "section" }], selector: "button", has_text: "save",
    has_not_text: "draft", visible: true, nth: -1, action: "text_content" }).value, "save final");
});

test("same-origin frame chains resolve reads and translate pointer coordinates to the top viewport", () => {
  const f = fixture();
  const child = f.document();
  const frame = f.element("iframe", { tagName: "IFRAME", contentDocument: child, clientLeft: 2, clientTop: 2,
    rect: { left: 100, top: 80, right: 400, bottom: 280, width: 300, height: 200 } });
  f.doc.hit = frame;
  const button = f.element("button", { tagName: "BUTTON", textContent: "inside" }, child);
  child.hit = button;
  const spec = { frame_selectors: ["iframe"], selector: "button" };
  assert.equal(f.run({ ...spec, action: "text_content" }).value, "inside");
  assert.deepEqual(f.run({ ...spec, action: "click" }).rect, { x: 162, y: 102 });
  frame.contentDocument = null;
  assert.match(f.run({ ...spec, action: "click" }).error, /cross-origin frames/);
});

test("pointer checks preserve scrolling and reject an overlay", () => {
  const f = fixture();
  const button = f.element("button", { tagName: "BUTTON" });
  button.scrollIntoView = () => { button.scrolled = true; };
  assert.equal(f.run({ selector: "button", action: "click" }).ok, true);
  assert.equal(f.run({ selector: "button", action: "pointer" }).ok, true);
  assert.equal(button.scrolled, true);
  f.doc.hit = f.element("overlay");
  assert.match(f.run({ selector: "button", action: "click" }).error, /covered/);
});

test("select_option requires an exact existing option before dispatching changes", () => {
  const f = fixture();
  const select = f.element("select", {
    tagName: "SELECT",
    options: [{ value: "a" }, { value: "b" }],
  });
  assert.equal(f.run({ selector: "select", action: "select_option", value: "b" }).value, "b");
  assert.deepEqual(select.events, ["input", "change"]);
  select.events = [];
  assert.match(f.run({ selector: "select", action: "select_option", value: "missing" }).error,
    /no option with value/);
  assert.equal(select.value, "b");
  assert.deepEqual(select.events, []);
});

test("shadow root active element is used when checking focus", () => {
  const f = fixture();
  const input = f.element("input");
  const shadow = { activeElement: null };
  input.getRootNode = () => shadow;
  assert.equal(f.run({ selector: "input", action: "press" }).focused, true);
  assert.notEqual(f.doc.activeElement, input);
});
