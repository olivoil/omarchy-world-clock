function parseStatus(raw) {
  var payload = JSON.parse(String(raw || ""))
  if (!payload || typeof payload !== "object" || Array.isArray(payload)
      || Number(payload.status_version) !== 1
      || typeof payload.supported !== "boolean"
      || typeof payload.installed !== "boolean"
      || typeof payload.shortcut_installed !== "boolean"
      || typeof payload.agent_installed !== "boolean"
      || typeof payload.default_shortcut !== "string")
    throw new Error("Unsupported integration status")

  var installed = payload.shortcut_installed && payload.agent_installed
  if (payload.installed !== installed)
    throw new Error("Inconsistent integration status")

  return {
    supported: payload.supported,
    installed: installed,
    shortcutInstalled: payload.shortcut_installed,
    agentInstalled: payload.agent_installed,
    defaultShortcut: payload.default_shortcut,
    reason: typeof payload.reason === "string" ? payload.reason : ""
  }
}

function actionDescription(shortcutInstalled, agentInstalled, defaultShortcut) {
  var names = { SUPER: "Super", SHIFT: "Shift", CTRL: "Ctrl", ALT: "Alt" }
  var parts = String(defaultShortcut || "Super+Shift+T").split(/\s*\+\s*/)
  for (var index = 0; index < parts.length; index++) {
    var normalized = parts[index].toUpperCase()
    parts[index] = names[normalized] || parts[index]
  }
  var shortcut = parts.join("+")
  if (!shortcutInstalled && !agentInstalled)
    return "Add " + shortcut + " and let agents query your saved places"
  if (!shortcutInstalled) return "Add " + shortcut + " to open World Clock"
  if (!agentInstalled) return "Let agents query your saved places by label"
  return "World Clock integrations are enabled"
}

function failureMessage(raw) {
  var detail = String(raw || "").trim()
  if (detail.indexOf("already in use") !== -1)
    return "That shortcut is already in use. Run the installer manually to choose another."
  if (detail.indexOf("Refusing to replace existing command") !== -1)
    return "An existing omarchy-world-clock command was preserved."
  if (detail.indexOf("Refusing to replace existing generic skill") !== -1)
    return "An existing World Clock agent skill was preserved."
  if (detail.indexOf("Hyprland already reports configuration errors") !== -1)
    return "Fix the existing Hyprland configuration error, then try again."
  if (detail.indexOf("review build") !== -1)
    return "Review builds do not change persistent shortcuts or agent skills."
  return "Could not enable the shortcut and agent access. Nothing unrelated was replaced."
}
