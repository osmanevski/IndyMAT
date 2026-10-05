import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

function escapedAt(text, index) {
  let slashes = 0;
  for (let i = index - 1; i >= 0 && text[i] === "\\"; i--) slashes++;
  return slashes % 2 === 1;
}
// A long technical report can contain hundreds of ordinary equations. These
// limits bound conversion work and macro input, not code/output dollar signs.
const publishMathLimits = Object.freeze({ equations: 500, equationLength: 4096, totalBytes: 128 * 1024 });
function texRanges(text, budget = { count: 0, bytes: 0 }) {
  let ranges = [], start = -1, display = false, open = 0;
  for (let i = 0; i < text.length; i++) {
    if (text[i] !== "$" || escapedAt(text, i)) continue;
    if (start < 0) {
      start = i;
      display = text[i + 1] === "$";
      open = display ? 2 : 1;
      i += open - 1;
      continue;
    }
    if (display && text[i + 1] !== "$") continue;
    let close = display ? 2 : 1, length = i - start - open;
    if (length > publishMathLimits.equationLength) throw new Error(t("An equation exceeds the {count}-character limit.", { count: publishMathLimits.equationLength }));
    let tex = text.slice(start + open, i);
    if (tex.trim()) {
      if (++budget.count > publishMathLimits.equations) throw new Error(t("The report exceeds the {count}-equation limit.", { count: publishMathLimits.equations }));
      budget.bytes += new TextEncoder().encode(tex).length;
      if (budget.bytes > publishMathLimits.totalBytes) throw new Error(t("The combined equation text exceeds the 128 KiB limit."));
      ranges.push({ start, end: i + close, tex, display, raw: text.slice(start, i + close) });
    }
    i += close - 1;
    start = -1;
  }
  return ranges;
}
// Octave 11.3 emits only dollar delimiters, with no equation class/wrapper.
// Heading anchors (<a id>) are prose; hyperlinks (including the TOC), code,
// and output blocks stay literal. Keep the heading id for TOC navigation.
function preparePublishedMath(html) {
  let doc = new DOMParser().parseFromString(html, "text/html"), items = [], budget = { count: 0, bytes: 0 };
  doc.querySelectorAll("script").forEach((node) => node.remove());
  let walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT), candidates = [];
  // Complete the capped preflight BEFORE replacing any nodes or loading
  // MathJax: an over-limit report never becomes a partly-rendered document.
  while (walker.nextNode()) {
    let node = walker.currentNode;
    if (node.parentElement?.closest("a[href],pre,code,style,textarea,svg,mjx-container")) continue;
    let ranges = texRanges(node.data, budget);
    if (!ranges.length) continue;
    candidates.push({ node, ranges });
  }
  for (let { node, ranges } of candidates) {
    let fragment = doc.createDocumentFragment(), cursor = 0;
    for (let range of ranges) {
      fragment.append(node.data.slice(cursor, range.start));
      let marker = doc.createElement("span");
      marker.setAttribute("data-mf-equation", String(items.length));
      marker.textContent = range.raw;
      fragment.append(marker);
      items.push({ ...range, marker });
      cursor = range.end;
    }
    fragment.append(node.data.slice(cursor));
    node.replaceWith(fragment);
  }
  return { doc, items };
}
function loadMathJax() {
  if (shared.mathJaxPromise) return shared.mathJaxPromise;
  shared.mathJaxPromise = new Promise((resolve, reject) => {
    window.MathJax = { startup: { typeset: false }, tex: { packages: ["base", "ams", "newcommand", "noundefined"] }, svg: { fontCache: "local" }, options: { enableMenu: false, enableAssistiveMml: false } };
    let script = document.createElement("script");
    script.src = "/mathjax-tex-svg.js";
    script.async = true;
    script.onload = () => Promise.resolve(window.MathJax.startup?.promise).then(() => resolve(window.MathJax), reject);
    script.onerror = () => reject(new Error(t("The local equation renderer could not be loaded.")));
    document.head.append(script);
  });
  return shared.mathJaxPromise;
}
async function renderPublishedMath(prepared) {
  let MathJax = await loadMathJax();
  for (let item of prepared.items) {
    let output = await MathJax.tex2svgPromise(item.tex, { display: item.display }), rendered = prepared.doc.importNode(output, true), svg = rendered.querySelector("svg");
    if (!svg) throw new Error(t("The equation could not be rendered as SVG."));
    // The combined bundle/menu preferences can still add assistive MathML.
    // Export just the SVG; the explicit TeX label/title provides accessibility.
    for (let child of [...rendered.children]) if (child !== svg) child.remove();
    svg.removeAttribute("aria-hidden");
    svg.removeAttribute("xmlns");
    svg.removeAttribute("xmlns:xlink");
    for (let use of svg.querySelectorAll("use")) {
      let href = use.getAttribute("xlink:href");
      if (href) {
        use.setAttribute("href", href);
        use.removeAttribute("xlink:href");
      }
    }
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", item.raw);
    svg.setAttribute("focusable", "false");
    let title = prepared.doc.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = item.raw;
    svg.insertBefore(title, svg.firstChild);
    rendered.setAttribute("data-tex-source", item.raw);
    rendered.style.direction = "ltr";
    rendered.style.display = item.display ? "block" : "inline-block";
    if (item.display) {
      rendered.style.textAlign = "center";
      rendered.style.margin = "1em 0";
    }
    item.marker.replaceWith(rendered);
  }
  // Static subset of MathJax 3 SVG styles: preserve overflow, glyph weight and
  // AMS table rules. SVG's own vertical-align and ex dimensions stay intact.
  let style = prepared.doc.createElement("style");
  style.setAttribute("data-mf-publish-math", "");
  style.textContent = `
mjx-container[data-tex-source] > svg { overflow: visible; min-height: 1px; min-width: 1px; }
mjx-container[data-tex-source] path[data-c], mjx-container[data-tex-source] use[data-c] { stroke-width: 3; }
g[data-mml-node="mtable"] > line[data-line], svg[data-table] > g > line[data-line],
g[data-mml-node="mtable"] > rect[data-frame], svg[data-table] > g > rect[data-frame] { stroke-width: 70px; fill: none; }
g[data-mml-node="mtable"] > .mjx-dashed, svg[data-table] > g > .mjx-dashed { stroke-dasharray: 140; }
g[data-mml-node="mtable"] > .mjx-dotted, svg[data-table] > g > .mjx-dotted { stroke-linecap: round; stroke-dasharray: 0,140; }
g[data-mml-node="mtable"] > g > svg { overflow: visible; }
`;
  prepared.doc.head.append(style);
  prepared.doc.querySelectorAll("script").forEach((node) => node.remove());
  return "<!doctype html>\n" + prepared.doc.documentElement.outerHTML;
}
async function showPublished(report, job, render) {
  if (report.staged) throw new Error(t("The published report has not been securely finalized by the server."));
  if (render) {
    try {
      let source = await registry.textAPI("published-render?job=" + encodeURIComponent(job) + "&render=" + encodeURIComponent(render)), prepared = preparePublishedMath(source);
      if (prepared.items.length) {
        let html = await renderPublishedMath(prepared);
        await registry.api("published-render", { job, render, html });
      }
    } catch (error) {
      registry.toast(t("Equations could not be rendered; showing the safe report with source TeX. {message}", { message: error.message }));
    }
  }
  let url = await registry.blobAPI("published?path=" + encodeURIComponent(report.path)), wrap = registry.el("div"), actions = registry.el("div", "publish-actions"), info = registry.el("p", "", t("{name} · {count} images", { name: report.name, count: report.images })), open = registry.el("button", "", t("Open in New Tab")), save = registry.el("button", "", t("Download HTML")), frame = registry.el("iframe", "publish-frame");
  frame.title = report.name;
  frame.setAttribute("sandbox", "");
  frame.src = url;
  open.onclick = () => {
    let page = window.open("about:blank", "_blank");
    if (!page) return;
    page.opener = null;
    page.document.title = report.name;
    page.document.body.style.margin = "0";
    let preview = page.document.createElement("iframe");
    preview.setAttribute("sandbox", "");
    preview.src = url;
    preview.style.cssText = "border:0;width:100vw;height:100vh";
    page.document.body.append(preview);
  };
  save.onclick = () => registry.download(url, report.name);
  actions.append(info, open, save);
  wrap.append(actions, frame);
  registry.$("#modal").addEventListener("close", () => URL.revokeObjectURL(url), { once: true });
  registry.modal(t("Publish Report"), wrap);
  const stopLanguage = onLanguageChange(() => {
    info.textContent = t("{name} · {count} images", { name: report.name, count: report.images });
    open.textContent = t("Open in New Tab");
    save.textContent = t("Download HTML");
    registry.$("#modal-title").textContent = t("Publish Report");
  });
  registry.$("#modal").addEventListener("close", stopLanguage, { once: true });
}
Object.assign(registry, { escapedAt, texRanges, publishMathLimits, preparePublishedMath, loadMathJax, renderPublishedMath, showPublished });
