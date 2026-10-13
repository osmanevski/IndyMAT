"use strict";
// UNVERIFIED graphics acceptance: orchestrator fresh-copy server only.
// No browser/ports/graphics/MATLAB execution while authoring. Pixel oracles
// below are input RGB barycentrics and endpoint interpolation, not goldens.
const { assert, expect, openFigures, run, require3D, projected, pixel, mode, settle } = require("./figure_ui_support.cjs");
const make = (name, code) => `close all;figure('name','D26 ${name}','position',[0 0 400 400]);${code}title('D26 ${name}');`;
const ramp = "[~,h]=contourf([0 1 2],[0 1],[0 1 2;0 1 2],[0 1 2],'linecolor','none');colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);";
const grid = "[x,y]=meshgrid(linspace(-2,2,9));";
const allSeries = (data) => data.axes.flatMap((axis) => axis.series);
const rows = (d) => Array.from({ length: d.shape[0] }, (_, r) => Array.from({ length: d.shape[1] }, (_, c) => d.values[r + c * d.shape[0]]));
async function artifact(page, job) {
  return page.evaluate(async (job) => {
    const headers = { "X-MF-Token": sessionStorage.getItem("mf-token") };
    const state = await (await fetch("/api/state", { headers })).json();
    if (state.job !== job || state.figures.length !== 1) throw new Error("D26 requires one current-job figure");
    const figure = state.figures[0];
    if (figure.job && figure.job !== job) throw new Error("D26 stale manifest");
    const response = await fetch(`/api/figure?job=${encodeURIComponent(job)}&file=${encodeURIComponent(figure.data_file)}`, { headers });
    if (!response.ok) throw new Error("D26 JSON unavailable");
    return response.json();
  }, job);
}
async function supported(page, name, code, threeD = true) {
  const job = await run(page, make(name, code));
  if (threeD) await require3D(page, `D26 ${name}`);
  else {
    await expect(page.locator("#figure-caption")).toHaveText(`D26 ${name}`);
    await expect(page.locator(".interactive-plot")).toBeVisible();
    await expect(page.locator("#plot-interactive")).toBeEnabled();
    await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
  }
  const data = await artifact(page, job);
  assert.equal(data.supported, true, name);
  assert.equal(data.source.job, job);
  return data;
}
async function fallback(page, name, code, reason, args = {}) {
  const job = await run(page, make(name, code));
  await expect(page.locator("#figure-caption")).toContainText(`D26 ${name}`);
  await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir:");
  await expect(page.locator("#plot-area img")).toBeVisible();
  await page.locator("#plot-area img").evaluate((img) => img.decode());
  await expect(page.locator(".figure-3d, .interactive-plot")).toHaveCount(0);
  await expect(page.locator("#figure-tools")).toBeHidden();
  await expect(page.locator("#plot-interactive")).toBeDisabled();
  const data = await artifact(page, job);
  assert.equal(data.supported, false, name);
  assert.equal(data.reason_code, reason, name);
  for (const [key, value] of Object.entries(args)) assert.equal(data.reason_args[key], value, `${name}: ${key}`);
  if (reason === "budget_exceeded") assert(data.reason_args.actual > data.reason_args.limit, "actual budget must exceed limit");
}
async function waitTransparency(page) {
  const diagnostics = await page.locator(".figure-3d").evaluate(async (node) => {
    const until = Date.now() + 6000;
    while (Date.now() < until) {
      const state = node.figureTest.transparency();
      if (state.reason_code || (!state.pending && state.frames > 0)) return state;
      await new Promise((resolve) => setTimeout(resolve, 8));
    }
    return node.figureTest.transparency();
  });
  assert.equal(diagnostics.reason_code || "", "", "transparent render must not silently fall back");
  console.log("D26 TRANSPARENCY", JSON.stringify(diagnostics));
  assert(!diagnostics.pending, "transparent pixels require a completed frame");
  assert(diagnostics.frames > 0, "transparent framebuffer must reach ready before pixel reads");
}
async function pin(page, point) {
  await mode(page, "tips");await settle(page);
  if (await page.locator(".figure-3d").evaluate(node => node.figureTest.transparency().enabled)) await waitTransparency(page);
  const p = await projected(page, point);
  const bounds = await page.locator(".figure-3d").boundingBox();assert(bounds);
  await page.mouse.click(bounds.x + p.x, bounds.y + p.y);
  return page.locator(".figure-pinned-tip").last();
}
const patch = (raw, mapping = "scaled", extra = "") =>
  "axes('position',[0 0 1 1]);v=[0 0 1;1 0 1;0 1 1];" +
  `patch('vertices',v,'faces',[1 2 3],'facevertexcdata',${raw},'facecolor','interp','edgecolor','interp','linewidth',3,'cdatamapping','${mapping}');` +
  "colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);view(2);axis([0 1 0 1 0 2]);axis off;" + extra;

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    await page.locator("#plot-area").evaluate((element) => { element.style.backgroundColor = "#fff"; document.body.style.setProperty("--plot-bg", "#ffffff"); });
    let data = await supported(page, "contourf finite bands", "axes('position',[0 0 1 1]);" + ramp + "axis([0 2 0 1]);axis off;");
    assert.equal(data.version, 3);assert.equal(data.axes[0].dimension, 3);
    const filled = allSeries(data).filter((s) => s.role === "contourf");
    assert(filled.length > 0);
    assert(filled.every((s) => s.kind === "patch3d" && Number.isFinite(s.level) && s.contour_group === 0 && s.faces.shape[0] === 1));
    assert(filled.every((s) => s.triangulation === "native-ring-v1"));
    assert(filled.every((s) => s.no_area === (s.faces.shape[1] < 3)));
    assert(filled.every((s) => s.triangle_count === Math.max(0, s.faces.shape[1] - 2)));
    assert(filled.every((s) => !("triangle_indices" in s) && !("triangle_faces" in s) && !("edge_indices" in s)));
    // Ramp bands occupy [0,1] and [1,2], lower finite level owns each face.
    // Strict interiors avoid contour boundaries and any inferred engine golden.
    await pixel(page, [.5, .5, 0], [255, 0, 0], 3);
    await pixel(page, [1.5, .5, 0], [0, 255, 0], 3);
    const tip = await pin(page, [.5, .5, 0]);
    await expect(tip).toContainText("Seviye: 0");await expect(tip).toContainText("Z: 0");

    for (const [name, recipe] of [["peaks20", "peaks(20)"], ["peaks50", "peaks(50)"], ["peaks100", "peaks(100)"], ["peaks20 explicit5", "peaks(20),[-4 -2 0 2 4]"]]) {
      data = await supported(page, "contourf " + name, `[~,h]=contourf(${recipe});`);
      assert(allSeries(data).every(s => s.role === "contourf" && s.triangulation === "native-ring-v1"));
      console.log("D26 CONTOUR", name, data.vertex_count, data.triangle_count);
    }

    for (const [name, code] of [
      ["contour labels group", "[c,h]=contour(x,y,x+y,[-1 0 1]);clabel(c,h);"],
      ["contour labels matrix", "c=contour(x,y,x+y,[-1 0 1]);clabel(c);"]
    ]) {
      data = await supported(page, name, grid + code, false);
      assert.equal(data.axes[0].dimension, 2);
      const records = allSeries(data).filter((s) => s.kind === "text");
      assert(records.length > 0 && records.every((s) => s.units === "data" && Number.isFinite(s.rotation)));
      await settle(page);
      const drawn = await page.locator(".interactive-plot").evaluate((node) => node.figureTest.texts());
      for (const record of records) assert(drawn.some((item) => item.text === record.lines.join("\n") && item.rotation === record.rotation), "native rotated contour text must be drawn");
    }
    await fallback(page, "contourf labels PNG", ramp + "clabel(get(h,'contourmatrix'),h);", "contour_labels", { property: "3d_text_depth" });
    await fallback(page, "contour3 labels PNG", grid + "[c,h]=contour3(x,y,x+y,[-1 0 1]);clabel(c,h);", "contour_labels", { property: "3d_text_depth" });
    data = await supported(page, "contourf background hole", ramp + "ch=findobj(h,'type','patch');set(ch(1),'facevertexcdata',NaN,'facecolor',[1 1 1]);");
    assert(allSeries(data).some(s => s.background_hole && s.level === null && s.face_color.mode === "constant"));
    const mutated = [
      ["compound", "set(ch(1),'vertices',[0 0;1 0;1 1;0 1;2 0;3 0;3 1;2 1],'faces',[1 2 3 4;5 6 7 8]);", { property: "faces", detail: "compound_faces" }],
      ["nonfinite", "vv=get(ch(1),'vertices');vv(1,1)=NaN;set(ch(1),'vertices',vv);", { property: "vertices", detail: "holes_or_nonfinite" }]
    ];
    for (const [name, code, args] of mutated) await fallback(page, `contourf ${name} PNG`, ramp + "ch=findobj(h,'type','patch');" + code, "contour_filled", args);

    const forms = [
      ["indexed scaled", "[0;1;2]", "scaled", "double", "indexed", [0, 1, 2]],
      ["indexed direct double", "[1;2;3]", "direct", "double", "indexed", [1, 2, 3]],
      ["indexed direct uint8", "uint8([0;1;2])", "direct", "uint8", "indexed", [0, 1, 2]],
      ["RGB double", "eye(3)", "scaled", "double", "truecolor", [1, 0, 0, 0, 1, 0, 0, 0, 1]],
      ["RGB single", "single(eye(3))", "scaled", "single", "truecolor", [1, 0, 0, 0, 1, 0, 0, 0, 1]],
      ["RGB uint8", "uint8(255*eye(3))", "direct", "uint8", "truecolor", [255, 0, 0, 0, 255, 0, 0, 0, 255]],
      ["RGB uint16", "uint16(65535*eye(3))", "direct", "uint16", "truecolor", [65535, 0, 0, 0, 65535, 0, 0, 0, 65535]]
    ];
    for (const [name, raw, mapping, klass, encoding, values] of forms) {
      data = await supported(page, `patch ${name}`, patch(raw, mapping));
      const s = allSeries(data).find((s) => s.kind === "patch3d");assert(s);
      assert.deepEqual(rows(s.vertices), [[0, 0, 1], [1, 0, 1], [0, 1, 1]]);
      assert.deepEqual(rows(s.faces), [[0, 1, 2]]);assert.deepEqual(s.triangle_faces, [0]);
      for (const c of [s.face_color, s.edge_color]) {
        assert.equal(c.mode, "interp");assert.equal(c.association, "vertex");assert.equal(c.mapping, mapping);
        assert.equal(c.cdata_class, klass);assert.equal(c.encoding, encoding);assert.deepEqual(c.data.values, values);
        assert.deepEqual(c.data.shape, encoding === "truecolor" ? [3, 1, 3] : [3, 1]);
      }
      // Triangle barycentrics at (x=.2,y=.3) are (.5,.2,.3).
      await pixel(page, [.2, .3, 1], [128, 51, 77], 5);
      if (klass === "uint8" && encoding === "truecolor") {
        const rawTip = await pin(page, [.015, .015, 1]);
        await expect(rawTip).toContainText("Köşe: 1");
        await expect(rawTip).toContainText("Renk değeri: [255, 0, 0]");
      }
    }
    data = await supported(page, "patch edge only interp", patch("eye(3)", "scaled", "set(findobj(gca,'type','patch'),'facecolor','none');"));
    assert.equal(allSeries(data).find((s) => s.kind === "patch3d").face_color.mode, "none");
    data = await supported(page, "surface edge indexed interp", "axes('position',[0 0 1 1]);surf([0 1],[0 1],ones(2),[0 2;1 0],'facecolor',[1 1 1],'edgecolor','interp','linewidth',4);colormap([1 0 0;0 1 0;0 0 1]);caxis([0 2]);view(2);axis([0 1 0 1 0 2]);axis off;");
    const surface = allSeries(data).find((s) => s.kind === "surface");
    assert.equal(surface.edge_color.mode, "interp");assert.equal(surface.edge_color.association, "vertex");
    assert.deepEqual(surface.edge_color.data.values, [0, 1, 2, 0]);
    // Boundary edge red->blue; opaque white faces preserve raw edge RGB.
    await pixel(page, [.25, 0, 1], [191, 0, 64], 8);
    await supported(page, "surface edge RGB uint8 interp", "raw=uint8(255*cat(3,[1 0;0 1],[0 1;0 0],[0 0;1 0]));surf([0 1],[0 1],ones(2),raw,'facecolor','interp','edgecolor','interp');view(2);");

    // Scalar alpha remains supported for surface and patch faces/edges.
    data = await supported(page, "patch scalar alpha", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',[1 2 3 4],'facecolor',[.2 .6 .8],'facealpha',.5,'edgealpha',.25);view(2);axis([0 1 0 1 0 2]);axis off;");
    let alphaPatch = allSeries(data).find((s) => s.kind === "patch3d");
    assert.deepEqual(alphaPatch.face_alpha, { mode: "constant", association: "constant", value: .5 });
    assert.deepEqual(alphaPatch.edge_alpha, { mode: "constant", association: "constant", value: .25 });
    await waitTransparency(page);
    await pixel(page, [.5, .5, 1], [153, 204, 230], 8); // 50% RGB [51,153,204] over white.
    const alphaTip = await pin(page, [.4, .4, 1]);
    await expect(alphaTip).toContainText("Z: 1");
    await page.setViewportSize({ width: 1400, height: 900 });
    await settle(page);await waitTransparency(page);
    await pixel(page, [.5, .5, 1], [153, 204, 230], 8);
    await page.setViewportSize({ width: 1512, height: 982 });
    await settle(page);await waitTransparency(page);
    data = await supported(page, "surface scalar alpha", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);surf([0 1],[0 1],ones(2),'facealpha',.5,'facecolor',[.2 .6 .8],'edgecolor','none');view(2);axis([0 1 0 1 0 2]);axis off;");
    const alphaSurface = allSeries(data).find((s) => s.kind === "surface");
    assert.deepEqual(alphaSurface.face_alpha, { mode: "constant", association: "constant", value: .5 });
    assert(!("edge_alpha" in alphaSurface), "opaque default edge alpha is omitted");
    await waitTransparency(page);
    await pixel(page, [.5, .5, 1], [153, 204, 230], 8);
    data = await supported(page, "patch alpha data two quads", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1;2 0 1;3 0 1;3 1 1;2 1 1],'faces',[1 2 3 4;5 6 7 8],'facecolor',[.2 .6 .8],'facealpha',.5);view(2);axis([0 3 0 1 0 2]);axis off;");
    const alphaQuads = allSeries(data).filter((s) => s.kind === "patch3d");
    assert.equal(alphaQuads.length, 1);
    assert.deepEqual(alphaQuads[0].faces.shape, [2, 4]);
    assert(alphaQuads.every((s) => s.face_alpha.mode === "constant" && s.face_alpha.value === .5));
    await waitTransparency(page);
    await pixel(page, [.5, .5, 1], [153, 204, 230], 8);
    await pixel(page, [2.5, .5, 1], [153, 204, 230], 8);
    data = await supported(page, "patch scalar edge alpha", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',1:4,'facecolor','none','edgecolor',[0 0 0],'edgealpha',.5,'linewidth',5);view(2);axis([0 1 -.1 1.1 0 2]);axis off;");
    const edgePatch = allSeries(data).find((s) => s.kind === "patch3d");
    assert.deepEqual(edgePatch.edge_alpha, { mode: "constant", association: "constant", value: .5 });
    assert(!("face_alpha" in edgePatch));
    await waitTransparency(page);
    await pixel(page, [.5, 0, 1], [128, 128, 128], 8);
    data = await supported(page, "surface scalar edge alpha", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);surf([0 1],[0 1],ones(2),'facecolor','none','edgecolor',[0 0 0],'edgealpha',.5,'linewidth',5);view(2);axis([0 1 -.1 1.1 0 2]);axis off;");
    const edgeSurface = allSeries(data).find((s) => s.kind === "surface");
    assert.deepEqual(edgeSurface.edge_alpha, { mode: "constant", association: "constant", value: .5 });
    assert(!("face_alpha" in edgeSurface));
    await waitTransparency(page);
    await pixel(page, [.5, 0, 1], [128, 128, 128], 8);

    assert(await page.locator(".figure-3d").evaluate(node => node.figureTest.loseContext()), "Chromium context-loss extension required");
    await expect(page.locator("#plot-area img")).toBeVisible();
    await expect(page.locator(".figure-3d")).toHaveCount(0);
    await expect(page.locator("#figure-caption")).toContainText("saydamlık çerçeve arabelleği kullanılamıyor");

    // Nine depth-separated transparent layers exceed runtime peeling. The
    // serializer must still accept the scene; the PNG fallback is runtime-only.
    const layers = Array.from({ length: 9 }, (_, i) => `patch('vertices',[0 0 ${i};1 0 ${i};1 1 ${i};0 1 ${i}],'faces',1:4,'facecolor',[.2 .6 .8],'facealpha',.5,'edgecolor','none');`).join("");
    const overflowJob = await run(page, make("nine transparent layers", "set(gcf,'color',[1 1 1]);axes('position',[0 0 1 1],'color',[1 1 1]);hold on;" + layers + "view(2);axis([0 1 0 1 0 10]);axis off;"));
    await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir:");
    await expect(page.locator("#figure-caption")).toContainText("saydamlık 8 katmanı aşıyor (9)");
    await expect(page.locator("#plot-area img")).toBeVisible();
    await expect(page.locator(".figure-3d, .interactive-plot")).toHaveCount(0);
    const overflowData = await artifact(page, overflowJob);
    assert.equal(overflowData.supported, true, "runtime peeling fallback must keep serializer support");

    await supported(page, "deterministic timing fallback", "surf(ones(2),'facealpha',.5,'edgecolor','none');");
    await waitTransparency(page);
    assert(await page.locator(".figure-3d").evaluate(node =>
      node.figureTest.transparencyTimingSamples([900, 800, 250.8, 250.8, 250.8])));
    await expect(page.locator("#plot-area img")).toBeVisible();
    await expect(page.locator("#figure-caption")).toContainText("saydamlık 251 ms sürdü (sınır 250 ms)");
    await expect(page.locator(".figure-3d")).toHaveCount(0);

    const rejected = [
      ["patch per vertex flat PNG", "patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facevertexcdata',[0;1;2],'facecolor','flat');view(3);", "patch_colors"],
      ["patch quad interp PNG", "patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',1:4,'facevertexcdata',[0;1;0;1],'facecolor','interp');view(3);", "interpolated_color"],
      ["patch pentagon interp PNG", "patch('vertices',[0 0 1;1 0 1;.5 .4 1;1 1 1;0 1 1],'faces',1:5,'facevertexcdata',[0;1;.5;1;0],'facecolor','interp');view(3);", "interpolated_color"],
      ["patch wrong interp count PNG", "patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',[1 2 3],'facevertexcdata',[0;1],'facecolor','interp');view(3);", "patch_colors"],
      ["surface vertex alpha PNG", "surf(ones(2),'facealpha','interp','alphadata',[0 .25;.5 1],'alphadatamapping','none');", "transparency"],
      ["patch face alpha data PNG", "patch('vertices',[0 0 1;1 0 1;0 1 1;1 1 1],'faces',[1 2 3;2 4 3],'facecolor',[.2 .6 .8],'facealpha','flat','facevertexalphadata',[.25;.75]);view(3);", "transparency"],
      ["patch vertex alpha data PNG", "patch('vertices',[0 0 1;1 0 1;0 1 1],'faces',1:3,'facecolor',[.2 .6 .8],'facealpha','interp','facevertexalphadata',[0;.5;1]);view(3);", "transparency"],
      ["surface edge alpha data PNG", "surf(ones(2),'edgealpha','interp','alphadata',[0 .25;.5 1]);", "transparency"],
      ["scatter marker alpha PNG", "scatter3([0 1],[0 1],[1 1],40,'markerfacealpha',.5);", "transparency"],
      ["effective light PNG", "surf(ones(2),'facelighting','flat');light;", "lighting"],
      ["surface vertex budget PNG", "surf(zeros(201,200));", "budget_exceeded"],
      ["contourf total vertex budget PNG", ramp + "ch=findobj(h,'type','patch');q=(0:100000)'*2*pi/100001;set(ch(1),'vertices',[cos(q),sin(q)],'faces',1:100001);", "budget_exceeded"]
    ];
    for (const [name, code, reason] of rejected) await fallback(page, name, code, reason,
      name === "patch quad interp PNG" || name === "patch pentagon interp PNG" ? {detail:"polygon_tessellation"} : {});
    assert.deepEqual(errors, []);
    console.log("UI FIGURES D26 PASS: bounded contourf bands/tips, native 2D labels, face/edge vertex colors and analytical pixels, exact guarded PNG reasons.");
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exit(1); });
