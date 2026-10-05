// Native window only (macos/App.swift marks <html class="native-window"> before the page loads):
// the title row hosts the Current Folder address bar, like the full-width address bar MATLAB
// users expect, instead of leaving that row empty. The same elements are moved, so ids,
// listeners and focus handling stay as they are; the breadcrumb remains in the side panel.
if (document.documentElement.classList.contains("native-window")) {
  const brand = document.querySelector(".toolbar .brand");
  const buttons = document.querySelector(".folder-buttons");
  const form = document.querySelector("#folder-form");
  if (brand && buttons && form) {
    const bar = document.createElement("div");
    bar.className = "title-folder";
    bar.append(buttons, form);
    brand.after(bar);
  }
}
