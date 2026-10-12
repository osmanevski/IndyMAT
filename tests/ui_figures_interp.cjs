"use strict";
const { assert, expect, openFigures, run, require3D, settle } = require("./figure_ui_support.cjs");

// The existing pixels hook renders and immediately calls readPixels in the
// same browser task. No preserveDrawingBuffer or screenshot decoding needed.
// All coordinates below are local CSS pixels; the hook handles DPR and GL Y.
async function surfacePixels(page, samples) {
  await settle(page);
  const result = await page.locator(".figure-3d").evaluate((node, samples) => {
    const hook = node.figureTest;
    const corners = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]].map((p) => hook.project(0, p));
    const left = Math.min(...corners.map((p) => p.x));
    const right = Math.max(...corners.map((p) => p.x));
    const top = Math.min(...corners.map((p) => p.y));
    const bottom = Math.max(...corners.map((p) => p.y));
    const points = samples.map(({ x, y }) => ({ x: left + x * (right - left), y: bottom - y * (bottom - top) }));
    const canvas = node.querySelector(".figure-webgl").getBoundingClientRect();
    return { corners, left, right, top, bottom, width: canvas.width, height: canvas.height, rgba: hook.pixels(points) };
  }, samples);
  assert([result.left, result.right, result.top, result.bottom].every(Number.isFinite), "nonfinite projected surface rectangle");
  assert(result.right - result.left > 40 && result.bottom - result.top > 40, "surface too small for interior samples");
  assert(result.left >= -1 && result.top >= -1 && result.right <= result.width + 1 && result.bottom <= result.height + 1,
    "projected surface lies outside the canvas");
  const [p00, p10, p01, p11] = result.corners;
  assert(p10.x > p00.x && p00.y > p01.y, "unexpected view(2) orientation");
  for (const difference of [p00.x - p01.x, p10.x - p11.x, p00.y - p10.y, p01.y - p11.y]) {
    assert(Math.abs(difference) < 1, "view(2) surface is not an axis-aligned rectangle");
  }
  return result.rgba;
}

async function checkSurfacePixels(page, samples) {
  const pixels = await surfacePixels(page, samples);
  assert.equal(pixels.length, samples.length);
  samples.forEach(({ x, y, rgb }, index) => {
    const rgba = pixels[index];
    assert(rgba && rgba.length === 4 && rgba[3] === 255, "expected opaque WebGL pixel");
    for (let channel = 0; channel < 3; channel++) {
      assert(Math.abs(rgba[channel] / 255 - rgb[channel]) <= 0.06,
        `Interp pixel at (${x},${y}) was [${rgba}], expected RGB [${rgb}] within 0.06`);
    }
  });
}

const unitSurface = (name, cdata, colormap) =>
  `close all;figure('name','${name}','position',[0 0 400 400]);` +
  `axes('position',[0 0 1 1]);surf([0 1;0 1],[0 0;1 1],zeros(2),${cdata},'facecolor','interp','edgecolor','none');` +
  `colormap(${colormap});caxis([0 1]);view(2);axis([0 1 0 1]);axis off;`;

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    await run(page, unitSurface("D24 interp measured diagonal", "[0 1;0.2 0.6]", "gray(256)"));
    await require3D(page, "D24 interp measured diagonal");
    await expect(page.locator("#figure-caption")).not.toContainText("Yalnızca PNG görünümü kullanılabilir:");
    // Literal samples from the orchestrator's Octave/MATLAB 9x9 pixel table:
    // x columns .02:.12:.98, y rows .98:-.12:.02. Cover both sides of
    // x+y=1 and its centre, where the other diagonal would give .30, not .60.
    // The measured diagonal is the existing i+1--i+rows (corners 1--2).
    await checkSurfacePixels(page, [
      [0.14, 0.26, 0.20], [0.26, 0.14, 0.29], [0.38, 0.38, 0.46],
      [0.26, 0.50, 0.36], [0.50, 0.26, 0.55],
      [0.50, 0.50, 0.60],
      [0.62, 0.62, 0.60], [0.74, 0.86, 0.55], [0.86, 0.74, 0.65],
      [0.50, 0.74, 0.55], [0.74, 0.50, 0.69]
    ].map(([x, y, gray]) => ({ x, y, rgb: [gray, gray, gray] })));

    // The renderer replaces Octave's PNG: reference is mapped-vertex RGB (A).
    // MATLAB R2025b's scalar/fragment lookup (B) is a known engine difference.
    await run(page, unitSurface("D24 interp RGB reference", "[0 1;0 1]", "[repmat([1 0 0],128,1);repmat([0 0 1],128,1)]"));
    await require3D(page, "D24 interp RGB reference");
    await checkSurfacePixels(page, [
      [0.10, [229, 0, 26]], [0.25, [191, 0, 64]], [0.40, [153, 0, 102]],
      [0.60, [103, 0, 152]], [0.75, [64, 0, 191]], [0.90, [26, 0, 229]]
    ].map(([x, rgb]) => ({ x, y: 0.5, rgb: rgb.map((channel) => channel / 255) })));

    await run(page, "close all;figure('name','D24 peaks shading interp');surf(peaks(20));shading interp;colormap(turbo(256));colorbar;");
    await require3D(page, "D24 peaks shading interp");

    await run(page, "close all;figure('name','D24 alpha fallback');surf(ones(2),'facealpha',0.5);");
    await expect(page.locator("#figure-caption")).toContainText("D24 alpha fallback");
    await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir:");
    await expect(page.locator("#figure-caption")).toContainText("saydamlık");
    await expect(page.locator("#plot-area img")).toBeVisible();
    await page.locator("#plot-area img").evaluate((img) => img.decode());
    await expect(page.locator(".figure-3d")).toHaveCount(0);
    await expect(page.locator("#figure-tools")).toBeHidden();
    await expect(page.locator("#plot-interactive")).toBeDisabled();
    assert.deepEqual(errors, []);
    console.log("UI FIGURES INTERP PASS: measured gray diagonal and Octave RGB-space pixels, interactive peaks shading interp and transparent-surface PNG fallback.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
