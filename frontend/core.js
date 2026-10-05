import shared from "./state.js";
import registry from "./registry.js";
import { t, getLanguage } from "./i18n.js";

const $ = (s) => document.querySelector(s);
const el = (tag, cls, text) => {
  let n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== void 0) n.textContent = text;
  return n;
};
async function api(path, data) {
  let res = await fetch("/api/" + path, { method: data === void 0 ? "GET" : "POST", headers: { "X-MF-Token": shared.token, "X-MF-Language": getLanguage(), ...data === void 0 ? {} : { "Content-Type": "application/json" } }, body: data === void 0 ? void 0 : JSON.stringify(data) });
  let out = await res.json();
  if (!res.ok) {
    // Machine-readable fields (code and its data) travel with the error, so callers never match on translated text.
    const error = new Error(out.error || t("The request could not be completed."));
    error.code = out.code;
    error.details = out;
    throw error;
  }
  return out;
}
async function textAPI(path) {
  let res = await fetch("/api/" + path, { headers: { "X-MF-Token": shared.token, "X-MF-Language": getLanguage() } });
  if (!res.ok) {
    let out = await res.json().catch(() => ({}));
    throw new Error(out.error || t("The request could not be completed."));
  }
  return res.text();
}
async function blobAPI(path) {
  let res = await fetch("/api/" + path, { headers: { "X-MF-Token": shared.token, "X-MF-Language": getLanguage() } });
  if (!res.ok) throw new Error(t("The image could not be read."));
  return URL.createObjectURL(await res.blob());
}
function toast(text) {
  $("#toast").textContent = text;
  $("#toast").hidden = false;
  clearTimeout(shared.toastTimer);
  shared.toastTimer = setTimeout(() => $("#toast").hidden = true, 5e3);
}
function safe(fn) {
  return Promise.resolve().then(fn).catch((e) => toast(e.message));
}
function on(id, fn) {
  $(id).addEventListener("click", () => safe(fn));
}
function draftStore() {
  try {
    let data = JSON.parse(localStorage.getItem("mf-drafts-v2") || "null");
    return data?.version === 2 && data.scopes ? data : { version: 2, scopes: {} };
  } catch {
    return { version: 2, scopes: {} };
  }
}
function within(path, root) {
  return !!root && (path === root || path.startsWith(root + "/"));
}
function joinPath(root, name) {
  if (name.startsWith("/")) return name;
  return root.replace(/\/$/, "") + "/" + name.replace(/^\.\//, "");
}
Object.assign(registry, { $, el, api, textAPI, blobAPI, toast, safe, on, draftStore, within, joinPath });
