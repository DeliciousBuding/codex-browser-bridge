(function(spec) {
  const MAX_ELEMENTS = 50000;
  const maxResults = Math.max(1, Math.min(Number(spec.max_results) || 50, 200));
  const clean = (value) => String(value == null ? "" : value).replace(/\s+/g, " ").trim();
  const textOf = (el) => String(el.innerText != null ? el.innerText : el.textContent || "");
  const windowOf = (el) => el.ownerDocument && el.ownerDocument.defaultView || globalThis;

  function allElements(root, out) {
    if (out.length >= MAX_ELEMENTS) return out;
    for (const el of root.querySelectorAll("*")) {
      out.push(el);
      if (out.length >= MAX_ELEMENTS) return out;
      if (el.shadowRoot) allElements(el.shadowRoot, out);
    }
    return out;
  }

  function roleOf(el) {
    const explicit = clean(el.getAttribute("role")).toLowerCase();
    if (explicit) return explicit.split(" ")[0];
    const tag = el.tagName.toLowerCase();
    if (tag === "button") return "button";
    if (tag === "a" && el.hasAttribute("href")) return "link";
    if (tag === "textarea") return "textbox";
    if (tag === "select") return el.multiple ? "listbox" : "combobox";
    if (tag === "option") return "option";
    if (/^h[1-6]$/.test(tag)) return "heading";
    if (tag === "img") return "img";
    if (tag === "input") {
      const type = clean(el.getAttribute("type") || "text").toLowerCase();
      if (type === "button" || type === "submit" || type === "reset") return "button";
      if (type === "checkbox") return "checkbox";
      if (type === "radio") return "radio";
      if (type === "range") return "slider";
      if (type !== "hidden") return "textbox";
    }
    if (el.isContentEditable) return "textbox";
    return "";
  }

  function accessibleName(el) {
    const labelledBy = clean(el.getAttribute("aria-labelledby"));
    if (labelledBy) {
      const fromIds = labelledBy.split(/\s+/).map((id) => {
        const root = el.getRootNode();
        const node = root && root.getElementById ? root.getElementById(id) : document.getElementById(id);
        return node ? textOf(node) : "";
      }).filter(Boolean).join(" ");
      if (fromIds) return fromIds;
    }
    const aria = clean(el.getAttribute("aria-label"));
    if (aria) return aria;
    if (el.labels && el.labels.length) {
      const labels = Array.from(el.labels).map(textOf).filter(Boolean).join(" ");
      if (labels) return labels;
    }
    return clean(el.getAttribute("alt") || el.getAttribute("title") ||
      el.getAttribute("placeholder") || el.getAttribute("data-testid") || textOf(el));
  }

  function matchesText(actual, expected, exact) {
    if (exact === true) return clean(actual) === clean(expected);
    return clean(actual).toLowerCase().includes(clean(expected).toLowerCase());
  }

  function isVisible(el) {
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const style = windowOf(el).getComputedStyle(el);
    return style.display !== "none" && style.visibility !== "hidden" &&
      style.visibility !== "collapse" && Number(style.opacity) !== 0;
  }

  function isEnabled(el) {
    if (el.matches && el.matches(":disabled")) return false;
    return el.disabled !== true && el.getAttribute("aria-disabled") !== "true" &&
      !(el.closest && el.closest('[aria-disabled="true"]'));
  }

  function describe(el) {
    return {tag: el.tagName.toLowerCase(), role: roleOf(el), name: accessibleName(el), text: clean(textOf(el)).slice(0, 200)};
  }

  function matching(roots, query) {
    let elements = Array.from(new Set(roots.flatMap((root) => allElements(root, []))));
    if (query.selector != null) {
      const selector = String(query.selector);
      elements = elements.filter((el) => {
        try { return el.matches(selector); }
        catch (err) { throw new Error("invalid selector: " + selector); }
      });
    }
    if (query.role != null) {
      const role = clean(query.role).toLowerCase();
      elements = elements.filter((el) => roleOf(el) === role);
    }
    if (query.name != null) elements = elements.filter((el) => matchesText(accessibleName(el), query.name, query.exact));
    if (query.text != null) {
      const textMatches = elements.filter((el) => matchesText(textOf(el), query.text, query.exact));
      const set = new Set(textMatches);
      elements = textMatches.filter((el) => !Array.from(el.querySelectorAll("*")).some((child) => set.has(child)));
    }
    if (query.test_id != null) {
      elements = elements.filter((el) => ["data-testid", "data-test-id", "data-test"]
        .some((name) => el.hasAttribute(name) && matchesText(el.getAttribute(name), query.test_id, query.exact !== false)));
    }
    if (query.placeholder != null) {
      elements = elements.filter((el) => el.hasAttribute("placeholder") && matchesText(el.getAttribute("placeholder"), query.placeholder, query.exact));
    }
    if (query.label != null) {
      elements = elements.filter((el) => {
        const labels = el.labels ? Array.from(el.labels) : [];
        return labels.some((label) => matchesText(textOf(label), query.label, query.exact)) ||
          ((el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby")) &&
            matchesText(accessibleName(el), query.label, query.exact));
      });
    }
    if (query.has_text != null) elements = elements.filter((el) => matchesText(el.textContent, query.has_text, false));
    if (query.has_not_text != null) elements = elements.filter((el) => !matchesText(el.textContent, query.has_not_text, false));
    if (query.visible != null) elements = elements.filter((el) => isVisible(el) === query.visible);
    if (query.nth != null) {
      const index = Number(query.nth);
      if (!Number.isInteger(index) || index < -1) throw new Error("locator nth must be -1 (last) or a nonnegative integer");
      const selected = index === -1 ? elements[elements.length - 1] : elements[index];
      elements = selected ? [selected] : [];
    }
    return elements;
  }

  function assertEditable(el) {
    if (!isEnabled(el)) throw new Error("matched element is disabled");
    if (el.readOnly === true || el.getAttribute("aria-readonly") === "true") throw new Error("matched element is readonly");
    if (!isVisible(el)) throw new Error("matched element is hidden");
  }

  function dispatchChange(el) {
    const EventClass = windowOf(el).Event;
    el.dispatchEvent(new EventClass("input", {bubbles: true, composed: true}));
    el.dispatchEvent(new EventClass("change", {bubbles: true, composed: true}));
  }

  function setNativeValue(el, value) {
    const win = windowOf(el);
    const constructor = el.tagName.toLowerCase() === "textarea" ? win.HTMLTextAreaElement : win.HTMLInputElement;
    const descriptor = constructor && Object.getOwnPropertyDescriptor(constructor.prototype, "value");
    if (!descriptor || !descriptor.set) throw new Error("matched element has no native input value setter");
    descriptor.set.call(el, value);
  }

  try {
    let page = document;
    const frames = [];
    for (const selector of spec.frame_selectors || []) {
      if (!page) break;
      const matches = matching([page], {selector});
      if (!matches.length) { page = null; break; }
      if (matches.length !== 1) throw new Error("frame selector must match exactly one iframe: " + selector);
      const frame = matches[0];
      if (!/^(iframe|frame)$/i.test(frame.tagName)) throw new Error("frame selector matched a non-frame element");
      const child = frame.contentDocument;
      if (!child) throw new Error("frame document is unavailable; cross-origin frames require CDP target routing");
      frames.push(frame);
      page = child;
    }
    let roots = page ? [page] : [];
    for (const scope of spec.scopes || []) roots = matching(roots, scope);
    const elements = matching(roots, spec);

    const count = elements.length;
    const visibleCount = elements.filter(isVisible).length;
    if (spec.action === "count") {
      return JSON.stringify({ok: true, count, visible_count: visibleCount});
    }
    if (spec.action === "all_text" || spec.action === "all_text_contents") {
      return JSON.stringify({
        ok: true,
        count,
        visible_count: visibleCount,
        values: elements.slice(0, maxResults).map((el) => spec.action === "all_text_contents" ? el.textContent : textOf(el)),
        truncated: count > maxResults
      });
    }

    if (count === 0) {
      if (spec.action === "probe") return JSON.stringify({ok: true, count: 0, visible_count: 0,
        matched: spec.wait_state === "hidden" || spec.wait_state === "detached"});
      return JSON.stringify({ok: false, error: "locator matched no elements", count: 0});
    }
    if (count > 1 && spec.strict !== false) {
      return JSON.stringify({
        ok: false,
        error: "locator matched " + count + " elements; pass nth or set strict=false",
        count,
        candidates: elements.slice(0, 5).map(describe)
      });
    }
    const el = elements[0];
    if (spec.action === "probe") {
      const visible = isVisible(el);
      return JSON.stringify({ok: true, count: 1, visible_count: visible ? 1 : 0,
        matched: spec.wait_state === "detached" ? false : spec.wait_state === "hidden" ? !visible :
          spec.wait_state === "attached" ? true : visible});
    }
    const base = {ok: true, count, visible_count: visibleCount, index: spec.nth == null ? 0 : spec.nth, element: describe(el)};

    if (spec.action === "text" || spec.action === "inner_text") return JSON.stringify({...base, value: textOf(el)});
    if (spec.action === "text_content") return JSON.stringify({...base, value: el.textContent});
    if (spec.action === "attribute") return JSON.stringify({...base, value: el.getAttribute(String(spec.attribute || ""))});
    if (spec.action === "value") return JSON.stringify({...base, value: "value" in el ? el.value : textOf(el)});
    if (spec.action === "visible") return JSON.stringify({...base, value: isVisible(el)});
    if (spec.action === "enabled") return JSON.stringify({...base, value: isEnabled(el)});
    if (spec.action === "checked") {
      return JSON.stringify({...base, value: "checked" in el ? !!el.checked : el.getAttribute("aria-checked") === "true"});
    }
    if (spec.action === "click" || spec.action === "dblclick" || spec.action === "pointer") {
      if (!isVisible(el) || !isEnabled(el)) return JSON.stringify({...base, ok: false, error: "matched element is hidden or disabled"});
      for (const frame of frames) frame.scrollIntoView({block: "center", inline: "center", behavior: "instant"});
      el.scrollIntoView({block: "center", inline: "center", behavior: "instant"});
      const rect = el.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return JSON.stringify({...base, ok: false, error: "matched element has no clickable box"});
      const win = windowOf(el);
      const left = Math.max(0, rect.left), right = Math.min(win.innerWidth, rect.right);
      const top = Math.max(0, rect.top), bottom = Math.min(win.innerHeight, rect.bottom);
      if (right <= left || bottom <= top) return JSON.stringify({...base, ok: false, error: "matched element is outside the viewport"});
      let x = (left + right) / 2, y = (top + bottom) / 2;
      const root = el.getRootNode();
      const hit = (root.elementFromPoint ? root : el.ownerDocument).elementFromPoint(x, y);
      if (!hit || (hit !== el && !el.contains(hit))) return JSON.stringify({...base, ok: false, error: "matched element is covered by another element"});
      for (const frame of frames.slice().reverse()) {
        const bounds = frame.getBoundingClientRect();
        if (frame.offsetWidth && (Math.abs(bounds.width - frame.offsetWidth) > 1 || Math.abs(bounds.height - frame.offsetHeight) > 1)) {
          throw new Error("pointer actions in transformed frames require CDP target routing");
        }
        x += bounds.left + frame.clientLeft;
        y += bounds.top + frame.clientTop;
        const parent = frame.ownerDocument;
        const parentHit = parent.elementFromPoint(x, y);
        if (parentHit !== frame) throw new Error("matched frame is covered or outside its parent viewport");
      }
      return JSON.stringify({...base, rect: {x, y}});
    }
    if (spec.action === "focus" || spec.action === "press") {
      if (el.focus) el.focus();
      return JSON.stringify({...base, focused: el.getRootNode().activeElement === el});
    }
    if (spec.action === "fill") {
      assertEditable(el);
      if (el.focus) el.focus();
      if (el.isContentEditable) el.textContent = String(spec.value == null ? "" : spec.value);
      else if (/^(input|textarea)$/i.test(el.tagName) && !/^(checkbox|radio|file|hidden|button|submit|reset|image)$/i.test(el.type || "")) {
        setNativeValue(el, String(spec.value == null ? "" : spec.value));
      }
      else return JSON.stringify({...base, ok: false, error: "matched element is not fillable"});
      dispatchChange(el);
      return JSON.stringify({...base, value: "value" in el ? el.value : textOf(el)});
    }
    if (spec.action === "select_option") {
      assertEditable(el);
      if (el.tagName.toLowerCase() !== "select") return JSON.stringify({...base, ok: false, error: "matched element is not a select"});
      const requested = String(spec.value == null ? "" : spec.value);
      if (!Array.from(el.options || []).some((option) => option.value === requested)) {
        return JSON.stringify({...base, ok: false, error: "select has no option with value " + JSON.stringify(requested)});
      }
      el.value = requested;
      dispatchChange(el);
      return JSON.stringify({...base, value: el.value});
    }
    if (spec.action === "set_checked") {
      assertEditable(el);
      const native = el.tagName.toLowerCase() === "input" && /^(checkbox|radio)$/i.test(el.type);
      if (!native && !/^(checkbox|radio|switch)$/.test(roleOf(el))) return JSON.stringify({...base, ok: false, error: "matched element is not checkable"});
      const checked = () => native ? !!el.checked : el.getAttribute("aria-checked") === "true";
      const desired = spec.checked === true;
      if (checked() !== desired) el.click();
      if (checked() !== desired) throw new Error("click did not produce the requested checked state");
      return JSON.stringify({...base, value: checked()});
    }
    return JSON.stringify({ok: false, error: "unsupported locator action: " + spec.action, count});
  } catch (err) {
    return JSON.stringify({ok: false, error: String(err && err.message || err)});
  }
})(__CODEX_LOCATOR_SPEC__)
