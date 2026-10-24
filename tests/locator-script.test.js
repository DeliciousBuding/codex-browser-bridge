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
  const context = vm.createContext({document: doc, ...win});
  function run(spec) {
    return JSON.parse(vm.runInContext(source.replace("__CODEX_LOCATOR_SPEC__", JSON.stringify(spec)), context, { timeout: 1000 }));
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

test("set_checked prepares trusted input and never performs an untrusted DOM click", () => {
  const f = fixture();
  const input = f.element("#box", { tagName: "INPUT", type: "checkbox", checked: false });
  const result = f.run({ selector: "#box", action: "set_checked", checked: true });
  assert.equal(result.value, false);
  assert.equal(result.needs_click, true);
  assert.deepEqual(result.rect, {x: 60, y: 20});
  assert.equal(input.clicks, 0);
  assert.equal(input.checked, false);
  input.checked = true;
  assert.equal(f.run({ selector: "#box", action: "set_checked", checked: true }).value, true);
  assert.equal(f.run({ selector: "#box", action: "set_checked", checked: true }).needs_click, false);
  assert.equal(input.clicks, 0);
});

test("checked postconditions reject replacement targets and ephemeral target references are released", () => {
  const f=fixture();
  const input=f.element("#box",{tagName:"INPUT",type:"checkbox",checked:false});
  const spec={selector:"#box",action:"set_checked",checked:true,target_token:"__test_target"};
  assert.equal(f.run(spec).needs_click,true);
  input.checked=true;
  assert.equal(f.run({...spec,action:"checked_probe"}).value,true);
  const replacement=f.element("#replacement",{tagName:"INPUT",type:"checkbox",checked:true});
  replacement.selector="#box"; input.selector="#detached";
  assert.match(f.run({...spec,action:"checked_probe"}).error,/detached or replaced/);
  assert.equal(f.run({action:"release_target",target_token:"__test_target"}).ok,true);
  assert.match(f.run({...spec,action:"checked_probe"}).error,/detached or replaced/);
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
    /no option matching/);
  assert.equal(select.value, "b");
  assert.deepEqual(select.events, []);
});

test("absent visible and enabled return false but ordinary reads still fail", () => {
  const f = fixture();
  for (const action of ["visible", "enabled"]) {
    assert.deepEqual(f.run({selector:".missing",action}), {ok:true,count:0,visible_count:0,value:false});
  }
  assert.equal(f.run({selector:".missing",action:"text"}).ok, false);
});

test("declarative has, has_not, intersections and unions share descendant and document order", () => {
  const f = fixture();
  const first = f.element("article", {innerText:"Alpha ready",textContent:"Alpha ready"});
  const second = f.element("article", {innerText:"Beta pending",textContent:"Beta pending"});
  const ready = f.element(".ready");
  const pending = f.element(".pending");
  first.children = [ready]; second.children = [pending];
  assert.equal(f.run({selector:"article",has:{selector:".ready"},action:"count"}).count, 1);
  assert.equal(f.run({selector:"article",has_not:{selector:".pending"},action:"text_content"}).value, "Alpha ready");
  assert.equal(f.run({and:[{selector:"article"},{has:{selector:".ready"}}],action:"text_content"}).value,"Alpha ready");
  const all = f.run({or:[{text:"Beta pending",exact:true},{text:"Alpha ready",exact:true}],action:"all"});
  assert.deepEqual(all.values.map((value)=>value.element.text),["Alpha ready","Beta pending"]);
  assert.deepEqual(all.values.map((value)=>value.locator.nth),[0,1]);
  assert.equal(f.run({...all.values[1].locator,action:"text_content"}).value,"Beta pending");
});

test("regex matchers preserve raw whitespace, honor flags and reject unsafe patterns", () => {
  const f = fixture();
  f.element("button",{innerText:"Save\nFinal",textContent:"Save\nFinal"});
  assert.equal(f.run({text:{regex:"^save\\s+Final$",flags:"i"},action:"count"}).count,1);
  assert.equal(f.run({text:{regex:"^save",flags:""},action:"count"}).count,0);
  assert.equal(f.run({has_text:{regex:"(a+)+$"},action:"count"}).ok,false);
  assert.equal(f.run({text:{regex:".*.*"},action:"count"}).ok,false);
  assert.equal(f.run({text:{regex:"["},action:"count"}).ok,false);
});

test("descriptor and multi-select selection validates every option before any mutation", () => {
  const f = fixture();
  const options=[{value:"a",label:"Alpha",selected:true},{value:"b",label:"Beta",selected:false},{value:"c",label:"Gamma",selected:false}];
  const select=f.element("select",{tagName:"SELECT",multiple:true,options});
  const selected=f.run({selector:"select",action:"select_option",options:[{label:"Beta",index:1},"c"]});
  assert.deepEqual(selected.values,["b","c"]);
  assert.deepEqual(options.map((option)=>option.selected),[false,true,true]);
  select.events=[];
  assert.equal(f.run({selector:"select",action:"select_option",options:["a",{label:"Missing"}]}).ok,false);
  assert.deepEqual(options.map((option)=>option.selected),[false,true,true]);
  assert.deepEqual(select.events,[]);
  assert.deepEqual(f.run({selector:"select",action:"select_option",options:[]}).values,[]);
  select.multiple=false;
  assert.match(f.run({selector:"select",action:"select_option",options:["a","b"]}).error,/multiple select/);
});

test("force skips overlay hit checks while trial fill/select leave state and events untouched", () => {
  const f=fixture();
  const input=f.element("input",{tagName:"INPUT"});
  f.doc.hit=f.element(".overlay");
  assert.equal(f.run({selector:"input",action:"click"}).retryable,true);
  assert.equal(f.run({selector:"input",action:"click",force:true}).ok,true);
  assert.equal(f.run({selector:"input",action:"fill",value:"new",trial:true}).trial,true);
  assert.equal(input.nativeValue,undefined);
  assert.deepEqual(input.events,[]);
  const select=f.element("select",{tagName:"SELECT",options:[{value:"a",selected:false}]});
  assert.equal(f.run({selector:"select",action:"select_option",options:"a",trial:true}).trial,true);
  assert.equal(select.options[0].selected,false);
  assert.deepEqual(select.events,[]);
});

test("accessible-name fields do not misidentify placeholders/test IDs and use hidden referenced label text", () => {
  const f=fixture();
  f.element("input",{tagName:"INPUT",attrs:{placeholder:"Password","data-testid":"password"}});
  f.element("label",{attrs:{id:"label"},innerText:"",textContent:"Account name"});
  f.element("#named",{tagName:"INPUT",attrs:{"aria-labelledby":"label"}});
  assert.equal(f.run({name:"Password",action:"count"}).count,0);
  assert.equal(f.run({name:"Account name",selector:"#named",action:"count"}).count,1);
  assert.equal(f.run({placeholder:"Password",action:"count"}).count,1);
});

test("opacity zero remains visible in locator terms and transformed frames fail explicitly", () => {
  const f=fixture();
  f.element("#transparent",{style:{display:"block",visibility:"visible",opacity:"0"}});
  assert.equal(f.run({selector:"#transparent",action:"visible"}).value,true);
  const child=f.document();
  const frame=f.element("iframe",{tagName:"IFRAME",contentDocument:child,clientLeft:0,clientTop:0,offsetWidth:50,offsetHeight:20});
  f.doc.hit=frame;
  f.element("button",{tagName:"BUTTON"},child);
  assert.match(f.run({frame_selectors:["iframe"],selector:"button",action:"click"}).error,/transformed frames/);
});

test("shadow root active element is used when checking focus", () => {
  const f = fixture();
  const input = f.element("input");
  const shadow = { activeElement: null };
  input.getRootNode = () => shadow;
  assert.equal(f.run({ selector: "input", action: "press" }).focused, true);
  assert.notEqual(f.doc.activeElement, input);
});
