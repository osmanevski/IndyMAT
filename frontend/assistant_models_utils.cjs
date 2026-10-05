// Stored choices are still validated against the server catalog before execution.
function modelSettings(value) {
  const result = {};
  for (const provider of ["claude", "codex", "agy"]) {
    const choice = value?.[provider];
    const valid = (text) => typeof text === "string" && (text === "" || /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.exec(text)?.[0] === text);
    if (choice && valid(choice.model) && valid(choice.effort)) result[provider] = { model: choice.model, effort: provider === "agy" ? "" : choice.effort };
  }
  return result;
}
function modelChoice(catalog, model = "", effort = "") {
  const selected = catalog.models.find((item) => item.id === model) || catalog.models[0];
  return { model: selected.id, effort: selected.efforts.includes(effort) ? effort : "" };
}
module.exports = { modelSettings, modelChoice };
