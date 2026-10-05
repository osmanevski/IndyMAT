import shared from "./state.js";
import registry from "./registry.js";
import { t } from "./i18n.js";
import { showAdaptation, showSourceErrors } from "./source_adapter.js";

async function poll() {
  if (shared.polling) return;
  shared.polling = true;
  const generation = shared.uiGeneration;
  const commandJob = shared.lastJob;
  try {
    let s = await registry.api("state");
    if (generation !== shared.uiGeneration) return;
    shared.wasDisconnected = false;
    shared.engine = s;
    registry.syncAssistantJobs?.(s, commandJob);
    registry.setStatus(s);
    let nextBreakpointKey = JSON.stringify(s.breakpoints || []);
    if (nextBreakpointKey !== shared.breakpointKey) {
      shared.breakpointKey = nextBreakpointKey;
      shared.breakpoints = new Map((s.breakpoints || []).map((item) => [item.file, new Map((item.breakpoints || item.lines.map((line) => ({ line, enabled: true, condition: "" }))).map((point) => [point.line, point]))]));
      registry.refreshDebugEditor();
    }
    registry.renderDebuggerPanel(s);
    registry.$("#workspace-path").textContent = s.current || s.workspace;
    registry.$("#workspace-path").title = s.current || s.workspace;
    registry.$("#cwd-status").textContent = s.cwd === (s.current || s.workspace) ? "" : s.cwd;
    registry.$("#cwd-status").title = s.cwd || "";
    registry.$("#engine-version").textContent = "GNU Octave " + s.version;
    registry.$("#engine-packages").textContent = (s.packages || []).map((p) => p.name).join(", ");
    if (shared.lastEpoch !== null && shared.lastEpoch !== s.epoch) {
      registry.variableEpochChanged(s.epoch);
      shared.publishRenders.clear();
      shared.lastFiguresKey = "";
      shared.variables = [];
      registry.renderVariables();
      shared.debugSnapshotKey = "";
    }
    shared.lastEpoch = s.epoch;
    // Remove capabilities even for failed/dead jobs, non-active tabs, or a
    // terminal result already seen by poll.completed. Do not prune OTHER jobs:
    // an older in-flight poll may arrive after a new submit response.
    let publishRender;
    if (s.status === "idle" || s.status === "dead") {
      publishRender = shared.publishRenders.get(s.job);
      shared.publishRenders.delete(s.job);
    }
    if (s.job === shared.lastJob && shared.activeOutput) {
      if (shared.activeOutput.sourceContext) {
        showAdaptation(shared.activeOutput, s.source_adapter);
        showSourceErrors(shared.activeOutput, s);
      }
      registry.applyConsoleClear(s);
      if (shared.activeOutput.out.textContent !== (s.output || "")) {
        shared.activeOutput.out.textContent = s.output || "";
        registry.scrollConsole();
      }
      if (shared.activeOutput.err.textContent !== (s.error || "")) shared.activeOutput.err.textContent = s.error || "";
      if (!shared.busy && !shared.starting) shared.activeOutput.time.textContent = `${s.elapsed?.toFixed(3) || 0} s`;
    }
    if (s.status === "paused" && s.debug?.ready) {
      let key = s.job + ":" + s.debug.serial;
      if (key !== shared.debugSnapshotKey) {
        shared.debugSnapshotKey = key;
        shared.variables = s.variables || [];
        registry.renderVariables();
        try {
          await registry.openDebugLocation(s.debug.file, s.debug.line);
        } catch (e) {
          registry.toast(e.message);
        }
        registry.refreshDebugEditor();
        if (s.debug.detail) registry.showDetail(s.debug.detail);
      }
    }
    if (!shared.busy && !shared.starting && s.status !== "paused" && s.job !== poll.completed) {
      poll.completed = s.job;
      shared.debugSnapshotKey = "";
      shared.variables = s.variables || [];
      registry.renderVariables();
      registry.refreshDebugEditor();
      if (s.job === shared.lastJob && s.error && !shared.activeOutput) registry.toast(s.error);
      if (s.kind === "inspect" && s.detail && !s.error && s.job === shared.lastJob) registry.showDetail(s.detail);
      if (s.kind?.startsWith("variable-") && s.job === shared.lastJob) await registry.variableJobCompleted(s);
      if (s.kind === "profile" && s.profile && !s.error && s.job === shared.lastJob) registry.showProfile(s.profile);
      if (s.kind?.startsWith("workspace-") && s.job === shared.lastJob) await registry.workspaceJobCompleted(s);
      if (s.kind === "publish" && s.publish && !s.error && s.job === shared.lastJob) {
        await registry.showPublished(s.publish, s.job, publishRender);
      }
      try {
        await registry.updateFigures(s);
      } catch (e) {
        registry.toast(e.message);
        shared.lastFiguresKey = "";
      }
      await registry.refreshFiles();
    }
  } catch (e) {
    if (!shared.wasDisconnected) {
      registry.toast(e.message);
      shared.wasDisconnected = true;
    }
    registry.$("#status-text").textContent = t("Connection lost");
    registry.$("#status-dot").style.background = "var(--danger)";
  } finally {
    shared.polling = false;
  }
}
Object.assign(registry, { poll });
