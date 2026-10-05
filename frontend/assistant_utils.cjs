// Pure transcript operations. All displayed output remains untrusted plain text.
function splitCodeBlocks(text) {
  const parts = [];
  const pattern = /^```([^\n]*)\n([\s\S]*?)(?:^```[ \t]*(?:\n|(?![\s\S]))|(?![\s\S]))/gm;
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > cursor) parts.push({ type: "text", text: text.slice(cursor, match.index) });
    parts.push({ type: "code", language: match[1].trim(), text: match[2] });
    cursor = match.index + match[0].length;
  }
  if (cursor < text.length) parts.push({ type: "text", text: text.slice(cursor) });
  return parts;
}
// What each program calls its tools, reduced to five things a reader cares about.
const TOOL_KINDS = {
  read: ["Read", "view_file", "NotebookRead"],
  edit: ["Edit", "MultiEdit", "NotebookEdit", "replace_file_content", "multi_replace_file_content", "sed_file"],
  write: ["Write", "write_to_file"],
  shell: ["Bash", "Shell", "command_execution", "run_command"],
  search: ["Glob", "Grep", "find_by_name", "grep_search", "list_dir", "web_search"]
};
const TOOL_LABELS = { read: "Read file", edit: "Edit file", write: "Write file", shell: "Shell", search: "File search" };
const HIDDEN_TOOLS = ["ToolSearch", "TodoWrite", "ExitPlanMode"];
function toolKind(name) {
  return Object.keys(TOOL_KINDS).find((kind) => TOOL_KINDS[kind].includes(name)) || "";
}
function changesFiles(event) {
  return ["edit", "write"].includes(toolKind(event.rawName || event.name || ""));
}
function fileLabel(path) {
  return String(path || "").split("/").filter(Boolean).slice(-2).join("/");
}
function toolActivity(event) {
  const rawName = event.name || "";
  const name = rawName.replace("mcp__indymat__", "indymat · ");
  let text = event.text || "";
  let values = null;
  try {
    values = JSON.parse(text);
  } catch {}
  if (name === "indymat · run_code") {
    if (values && typeof values.code === "string") text = values.code;
    return { ...event, name, rawName, text };
  }
  const kind = toolKind(rawName);
  if (!kind) return { ...event, name, rawName, text };
  if (values && typeof values === "object") {
    const path = values.file_path || values.notebook_path || values.AbsolutePath || values.TargetFile || values.path || "";
    if (kind === "shell") text = String(values.command || values.CommandLine || text);
    else if (kind === "search") text = String(values.pattern || values.Pattern || values.Query || values.query || values.DirectoryPath || values.path || "");
    else if (kind === "edit") text = fileLabel(path) + (Number.isFinite(values.added) ? "  +" + values.added + " −" + (values.removed || 0) : "");
    else if (kind === "write") text = fileLabel(path) + (Number.isFinite(values.content_lines) ? "  +" + values.content_lines : Number.isFinite(values.CodeContent_lines) ? "  +" + values.CodeContent_lines : "");
    else text = fileLabel(path);
  }
  // Codex wraps every command in a login shell; the command itself is what matters.
  if (kind === "shell") text = text.replace(/^\/bin\/(?:zsh|bash|sh) -lc (['"])([\s\S]*)\1$/, "$2");
  return { ...event, name, rawName, label: TOOL_LABELS[kind], kind, text };
}
function fileChange(event) {
  // Codex reports changed files as a list; show names, not JSON.
  try {
    const changes = JSON.parse(event.text || "");
    if (Array.isArray(changes) && changes.length) return { ...event, type: "tool", name: "file_change", rawName: "Edit", label: TOOL_LABELS.edit, kind: "edit", text: changes.map((change) => fileLabel(change?.path)).filter(Boolean).join(", ") };
  } catch {}
  return event;
}
function approvalActivity(event) {
  const activity = toolActivity({ name: event.tool || "", text: "" });
  return { role: "approval", id: event.id, turn: event.turn, tool: event.tool, label: activity.label || "Tool activity", summary: event.summary || "", detail: event.detail || {}, text: event.detail?.text || "", decision: null };
}
function approvalLines(text) {
  return String(text || "").split("\n").map((text) => ({ text, kind: text.startsWith("+") && !text.startsWith("+++") ? "added" : text.startsWith("-") && !text.startsWith("---") ? "removed" : "context" }));
}
function pendingApprovals(messages) {
  return messages.filter((message) => message.role === "approval" && !message.decision);
}
function reduceTranscript(messages, event) {
  if (event.type === "approval") {
    if (messages.some((message) => message.role === "approval" && message.id === event.id)) return messages;
    return boundTranscript([...messages, approvalActivity(event)]);
  }
  if (event.type === "approval-resolved") return messages.map((message) => message.role === "approval" && message.id === event.id && message.turn === event.turn ? { ...message, decision: message.decision || event.decision, detail: {}, text: "" } : message);
  if (event.type === "turn-end") messages = messages.map((message) => message.role === "approval" && !message.decision && message.turn === event.turn ? { ...message, decision: "deny", detail: {}, text: "" } : message);
  if (event.type === "tool" && HIDDEN_TOOLS.includes(event.name)) return messages;
  if (event.type === "file") event = fileChange(event);
  if (event.label && event.type === "tool" && event.name === "file_change") {
    const last = messages.at(-1);
    if (last?.role === "tool" && last.name === "file_change" && last.text === event.text && last.turn === event.turn) return messages;
    return [...messages.map((message) => ({ ...message })), { role: "tool", name: event.name, text: event.text, turn: event.turn, label: event.label, kind: event.kind }];
  }
  if (event.type === "tool") event = toolActivity(event);
  const result = messages.map((message) => ({ ...message }));
  if (event.type === "text") {
    const last = result.at(-1);
    if (last?.role === "assistant" && last.turn === event.turn) last.text += event.text || "";
    else result.push({ role: "assistant", turn: event.turn, text: event.text || "" });
  } else if (event.type === "turn-end") {
    result.push({ role: "state", text: event.state, turn: event.turn });
  } else if (event.type !== "conversation") {
    if (event.type === "tool" && event.name === "indymat · run_code" && result.some((message) => message.turn === event.turn && message.name === event.name && message.text === event.text)) return result;
    result.push({ role: event.type, name: event.name, text: event.text || "", turn: event.turn, ...(event.steered ? { steered: true } : {}), ...(event.label ? { label: event.label, kind: event.kind } : {}) });
  }
  return boundTranscript(result);
}
function boundTranscript(result) {
  // Bound browser memory as well as server memory; signal every eviction.
  let size = result.reduce((total, item) => total + item.text.length, 0);
  let truncated = false;
  while (result.length > 1 && (result.length > 1000 || size > 1_000_000)) {
    // Pending cards remain reachable even during a flood of provider output.
    const index = result.findIndex((item) => item.role !== "approval" || item.decision);
    if (index < 0) break;
    size -= result.splice(index, 1)[0].text.length;
    truncated = true;
  }
  const last = result.at(-1);
  if (last?.text.length > 1_000_000) {
    last.text = last.text.slice(-1_000_000);
    truncated = true;
  }
  if (truncated) result.unshift({ role: "notice", text: "Earlier assistant messages were truncated.", source: true });
  return result;
}
module.exports = { splitCodeBlocks, reduceTranscript, toolActivity, changesFiles, approvalActivity, approvalLines, pendingApprovals };
