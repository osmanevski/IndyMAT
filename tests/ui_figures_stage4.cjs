"use strict";
// Run only through the orchestrator's fresh-copy gate/server. Authoring this
// suite did not execute graphics, start a browser or measure MATLAB behavior.
const { assert, expect, openFigures, run, require3D, projected, pixel, mode, settle } = require("./figure_ui_support.cjs");

const grid = "[x,y]=meshgrid(linspace(-2,2,9));z=x.^2-y.^2;";
const vertices = "v=[0 0 1;1 0 1;1 1 1;0 1 1];";
const make = (name, code) => `close all;figure('name','D25 ${name}','position',[0 0 400 400]);${code}`;

async function artifact(page, job) {
  return page.evaluate(async (job) => {
    const headers = { "X-MF-Token": sessionStorage.getItem("mf-token") };
    const state = await (await fetch("/api/state", { headers })).json();
    if (state.job !== job || state.figures.length !== 1) throw new Error("Stage 4 requires one current-job figure");
    const figure = state.figures[0];
    if (figure.job && figure.job !== job) throw new Error("Stale figure manifest");
    const response = await fetch(`/api/figure?job=${encodeURIComponent(job)}&file=${encodeURIComponent(figure.data_file)}`, { headers });
    if (!response.ok) throw new Error("Stage 4 JSON unavailable");
    return response.json();
  }, job);
}
async function supported(page, name, code, threeD = true) {
  const job = await run(page, make(name, code));
  if (threeD) await require3D(page, `D25 ${name}`);
  else {
    await expect(page.locator("#figure-caption")).toHaveText(`D25 ${name}`);
    await expect(page.locator("#plot-interactive")).toBeEnabled();
    await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
    await expect(page.locator(".interactive-plot, .figure-3d")).toBeVisible();
  }
  const data = await artifact(page, job);
  assert.equal(data.supported, true, `${name}: expected interactive artifact`);
  assert.equal(data.source.job, job);
  return data;
}
async function fallback(page, name, code, reason) {
  const job = await run(page, make(name, code));
  await expect(page.locator("#figure-caption")).toContainText(`D25 ${name}`);
  await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir:");
  await expect(page.locator("#plot-area img")).toBeVisible();
  await page.locator("#plot-area img").evaluate((img) => img.decode());
  await expect(page.locator(".figure-3d, .interactive-plot")).toHaveCount(0);
  await expect(page.locator("#figure-tools")).toBeHidden();
  await expect(page.locator("#plot-interactive")).toBeDisabled();
  const data = await artifact(page, job);
  assert.equal(data.supported, false, name);
  assert.equal(data.reason_code, reason, `${name}: exact serializer fallback reason`);
}
async function pin(page, point) {
  await mode(page, "tips");
  await settle(page);
  const p = await projected(page, point);
  const bounds = await page.locator(".figure-3d").boundingBox();
  assert(bounds && Number.isFinite(p.x) && Number.isFinite(p.y));
  await page.mouse.click(bounds.x + p.x, bounds.y + p.y);
  return page.locator(".figure-pinned-tip").last();
}
const allSeries = (data) => data.axes.flatMap((axis) => axis.series);
const descriptorRows = (descriptor) => Array.from({ length: descriptor.shape[0] }, (_, row) =>
  Array.from({ length: descriptor.shape[1] }, (_, column) => descriptor.values[row + column * descriptor.shape[0]]));

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    let data = await supported(page, "contour levels", grid + "contour(x,y,z,[-.5 0 .5]);colormap([1 0 0;0 1 0;0 0 1]);caxis([-1 1]);", false);
    const contours = allSeries(data).filter((s) => s.role === "contour");
    assert(contours.length > 0, "resolved contour paths are required");
    assert.deepEqual([...new Set(contours.map((s) => s.level))].sort((a, b) => a - b), [-.5, 0, .5]);
    assert(contours.every((s) => s.kind === "line" && typeof s.contour_closed === "boolean"));
    const expectedLevelColors = new Map([[-.5, [1, 0, 0]], [0, [0, 1, 0]], [.5, [0, 0, 1]]]);
    for (const contour of contours) assert.deepEqual(contour.line_color, expectedLevelColors.get(contour.level), "custom LevelList color mapping");

    data = await supported(page, "contour3 styles", grid + "[~,h]=contour3(x,y,z,[-.5 0 .5]);set(h,'linestyle','--','linewidth',2.5);");
    assert(allSeries(data).filter((s) => s.role === "contour").every((s) => s.line_style === "--" && s.line_width === 2.5));
    data = await supported(page, "contour manual zlevel", grid + "[~,h]=contour(x,y,z,[0 0]);set(h,'zlevelmode','manual','zlevel',2,'linecolor',[.2 .7 .3]);view(3);");
    const manual = allSeries(data).filter((s) => s.role === "contour");
    assert(manual.length > 0 && manual.every((s) => s.z.every((value) => value === null || value === 2)));
    for (const contour of manual) assert.deepEqual(contour.line_color, [.2, .7, .3], "resolved explicit contour RGB");
    const sample = manual[0].x.findIndex((value, index) => value !== null && manual[0].y[index] !== null && Math.abs(value) < 1.8 && Math.abs(manual[0].y[index]) < 1.8);
    assert(sample >= 0, "manual contour must expose an interior source sample");
    const levelTip = await pin(page, [manual[0].x[sample], manual[0].y[sample], 2]);
    await expect(levelTip).toContainText("Seviye: 0");
    await expect(levelTip).toContainText("Z: 2");

    for (const creator of ["surfc", "meshc"]) {
      data = await supported(page, `${creator} resolved contours`, grid + `${creator}(x,y,z);`);
      assert(allSeries(data).some((s) => s.kind === "surface"));
      assert(allSeries(data).some((s) => s.role === "contour"));
    }
    await fallback(page, "contourf labels fallback", grid + "[c,h]=contourf(x,y,z,[-.5 0 .5]);clabel(c,h);", "contour_labels");
    await supported(page, "clabel rotated", grid + "[c,h]=contour(x,y,z,[-.5 0 .5]);clabel(c,h);", false);
    await supported(page, "clabel matrix", grid + "c=contour(x,y,x.^2+y.^2,[1 2 3]);clabel(c);", false);

    data = await supported(page, "patch RGB convex", vertices + "axes('position',[0 0 1 1]);patch('vertices',v,'faces',[1 2 3 4],'facecolor',[.2 .6 .8],'edgecolor','none');view(2);axis([0 1 0 1 0 2]);axis off;");
    const patch = allSeries(data).find((s) => s.kind === "patch3d");
    assert(patch, "nonzero Z patch must retain the 3D viewer even at view(2)");
    assert.equal(patch.triangle_indices.shape[0], 2);
    assert.deepEqual(patch.triangle_faces, [0, 0]);
    assert.deepEqual(descriptorRows(patch.triangle_indices), [[3, 0, 1], [1, 2, 3]], "first valid source-order ear clipping");
    assert.deepEqual(descriptorRows(patch.edge_indices), [[0, 1, 0], [1, 2, 0], [2, 3, 0], [3, 0, 0]], "source boundary edges only");
    // A strictly interior point in an opaque constant RGB face. The expected
    // bytes are input RGB * 255, rounded; no engine measurement is claimed.
    await pixel(page, [.5, .5, 1], [51, 153, 204], 3);
    const patchTip = await pin(page, [.02, .02, 1]);
    await expect(patchTip).toContainText("Yüz: 1");
    await expect(patchTip).toContainText("Köşe: 1");
    await expect(patchTip).toContainText("Z: 1");

    data = await supported(page, "fill3 XYZ concave", "fill3([0 2 2 1 0],[0 0 2 1 2],ones(1,5),[.2 .6 .8],'edgecolor',[.1 .1 .1],'linestyle',':','linewidth',2);view(3);");
    assert.deepEqual(descriptorRows(allSeries(data).find((s) => s.kind === "patch3d").triangle_indices),
      [[1, 2, 3], [0, 1, 3], [0, 3, 4]], "concave face uses deterministic first valid ears");
    data = await supported(page, "patch NaN face padding", vertices + "patch('vertices',[v;2 0 1;3 0 1;2.5 1 1],'faces',[1 2 3 4;5 6 7 NaN],'facecolor',[.2 .6 .8],'edgecolor','none');view(3);");
    assert.deepEqual([...new Set(allSeries(data).find((s) => s.kind === "patch3d").triangle_faces)].sort(), [0, 1]);
    for (const [name, colors] of [["indexed", "[1;2]"], ["RGB", "[1 0 0;0 0 1]"]]) {
      await supported(page, `patch ${name} face flat`, vertices + `patch('vertices',[v;v+[2 0 0]],'faces',[1 2 3 4;5 6 7 8],'facevertexcdata',${colors},'facecolor','flat','edgecolor','none','cdatamapping','direct');colormap([1 0 0;0 0 1]);view(3);`);
    }
    await supported(page, "patch 64 vertices", "q=(0:63)'*2*pi/64;patch('vertices',[cos(q),sin(q),ones(64,1)],'faces',1:64,'facecolor',[.2 .6 .8],'edgecolor','none');view(3);");
    await supported(page, "patch scalar alpha", vertices + "patch('vertices',v,'faces',[1 2 3 4],'facecolor',[.2 .6 .8],'facealpha',.5);view(3);");
    const failures = [
      ["interp invalid rows", "patch('vertices',v,'faces',[1 2 3 4],'facevertexcdata',[0;1],'facecolor','interp');", "patch_colors"],
      ["alpha data", "patch('vertices',v,'faces',[1 2 3 4],'facecolor',[.2 .6 .8],'facealpha','interp','facevertexalphadata',[0;.25;.5;1]);", "transparency"],
      ["lighting", "patch('vertices',v,'faces',[1 2 3 4],'facecolor',[.2 .6 .8],'facelighting','flat');light;", "lighting"],
      ["nonplanar", "v(4,3)=1.2;patch('vertices',v,'faces',[1 2 3 4],'facecolor',[.2 .6 .8]);", "unsupported_patch"],
      ["bowtie", "patch('vertices',v,'faces',[1 3 2 4],'facecolor',[.2 .6 .8]);", "unsupported_patch"],
      ["degenerate", "patch('vertices',[0 0 1;1 0 1;2 0 1],'faces',[1 2 3],'facecolor',[.2 .6 .8]);", "unsupported_patch"],
      ["65 vertices", "q=(0:64)'*2*pi/65;patch('vertices',[cos(q),sin(q),ones(65,1)],'faces',1:65,'facecolor',[.2 .6 .8]);", "budget_exceeded"],
      ["vertex budget", "patch('vertices',[v;zeros(39997,2),ones(39997,1)],'faces',[1 2 3 4],'facecolor',[.2 .6 .8]);", "budget_exceeded"],
      ["per vertex flat", "patch('vertices',v,'faces',[1 2 3 4],'facevertexcdata',[1;2;3;4],'facecolor','flat');", "patch_colors"],
      ["ambiguous flat", "patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3;2 3 1;3 1 2],'facevertexcdata',[1;2;3],'facecolor','flat');", "patch_colors"]
    ];
    for (const [name, code, reason] of failures) await fallback(page, `patch ${name} fallback`, vertices + code + "view(3);", reason);
    assert.deepEqual(errors, []);
    console.log("UI FIGURES STAGE4 PASS: resolved contour paths/styles/levels, planar patch geometry and source tips, constant RGB interior pixel, exact PNG fallback reasons.");
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exit(1); });
