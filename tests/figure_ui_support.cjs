"use strict";

// Used only by the dedicated UI suites, each on check.py's fresh server/copy.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium, expect } = require("@playwright/test");

const editorOpened = new WeakSet();

async function openFigures() {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ locale: "tr-TR", deviceScaleFactor: 2, viewport: { width: 1512, height: 982 } });
  const errors = [];
  const execute = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/api/execute") execute.push(request.postDataJSON());
  });
  await page.goto(launch.url);
  await expect(page.locator("#status-text")).toHaveText("Hazır", { timeout: 30000 });
  await expect(page.locator("html")).toHaveAttribute("lang", "tr");
  return { browser, page, errors, execute };
}

async function run(page, code) {
  // The Run button (F5) saves and runs a FILE; an unsaved tab is run as text
  // through "run selection" (F9), like the other UI suites do.
  if (!editorOpened.has(page)) {
    await page.locator('[data-action="new"]').first().click();
    editorOpened.add(page);
  }
  const editor = page.locator(".cm-content");
  await editor.fill(code);
  await editor.press("ControlOrMeta+a");
  const submitted = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/execute" && response.request().method() === "POST");
  await editor.press("F9");
  const response = await submitted;
  assert.equal(response.status(), 202);
  const { job } = await response.json();
  await page.waitForFunction(async (job) => {
    const response = await fetch("/api/state", { headers: { "X-MF-Token": sessionStorage.getItem("mf-token") } });
    const state = await response.json();
    return state.job === job && state.status === "idle" && document.querySelector("#status-text").textContent === "Hazır";
  }, job, { timeout: 60000 });
  return job;
}

async function require3D(page, name, caption = name) {
  try {
    await page.waitForFunction((name) => document.querySelector(".figure-3d")?.figureTest && document.querySelector("#figure-caption").textContent.includes(name), name, { timeout: 15000 });
  } catch {
    const caption = await page.locator("#figure-caption").innerText();
    throw new Error(`WebGL2 interactive viewer REQUIRED for ${name}; PNG fallback is a test failure. Headless Chromium must support WebGL2. Caption: ${caption}`);
  }
  await expect(page.locator(".figure-webgl")).toBeVisible();
  await expect(page.locator("#figure-tools")).toBeVisible();
  await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
  await expect(page.locator("#figure-tool-instructions")).toContainText("Döndürmek için sürükleyin");
  await expect(page.locator("#figure-caption")).toHaveText(caption);
}

const states = (page) => page.locator(".figure-3d").evaluate((node) => node.figureTest.camera());
const view = async (page) => (await states(page)).map(({ identity, version, ...camera }) => camera);
const projected = (page, point, axis = 0) => page.locator(".figure-3d").evaluate((node, { point, axis }) => node.figureTest.project(axis, point), { point, axis });
async function pixel(page, point, expected, tolerance = 32) {
  const p = await projected(page, point);
  const samples = await page.locator(".figure-3d").evaluate((node, p) => node.figureTest.pixels([p, { x: p.x + 1, y: p.y }, { x: p.x, y: p.y + 1 }]), p);
  for (const rgba of samples) {
    assert(rgba, "Disposed WebGL context");
    assert.equal(rgba[3], 255, "opaque canvas");
    for (let i = 0; i < 3; i++) assert(Math.abs(rgba[i] - expected[i]) <= tolerance, `Interior pixel ${rgba} differs from ${expected} (tolerance ${tolerance})`);
  }
}
async function mode(page, value) {
  const selector = page.locator("#figure-tool-select");
  if (await selector.isVisible()) await selector.selectOption(value);
  else await page.locator(`#figure-tool-${value}`).check({ force: true });
  await expect(page.locator(".figure-3d")).toHaveAttribute("data-figure-mode", value);
}
async function drag(page, dx, dy) {
  const bounds = await page.locator(".figure-3d").boundingBox();
  assert(bounds);
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width / 2 + dx, bounds.y + bounds.height / 2 + dy, { steps: 4 });
  await page.mouse.up();
}
async function settle(page) {
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

module.exports = { assert, expect, openFigures, run, require3D, states, view, projected, pixel, mode, drag, settle };
