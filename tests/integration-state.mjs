import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/IntegrationState.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

const complete = context.parseStatus(JSON.stringify({
  status_version: 1,
  supported: true,
  installed: true,
  shortcut_installed: true,
  agent_installed: true,
  default_shortcut: "SUPER + SHIFT + T",
  reason: null,
}))
assert.equal(complete.installed, true)
assert.equal(complete.shortcutInstalled, true)
assert.equal(complete.agentInstalled, true)
assert.equal(complete.defaultShortcut, "SUPER + SHIFT + T")

const partial = context.parseStatus(JSON.stringify({
  status_version: 1,
  supported: true,
  installed: false,
  shortcut_installed: true,
  agent_installed: false,
  default_shortcut: "SUPER + SHIFT + T",
}))
assert.equal(partial.installed, false)
assert.equal(context.actionDescription(
  partial.shortcutInstalled, partial.agentInstalled, partial.defaultShortcut),
"Let agents query your saved places by label")
assert.equal(context.actionDescription(false, false, "SUPER + SHIFT + T"),
  "Add Super+Shift+T and let agents query your saved places")
assert.equal(context.actionDescription(false, true, "SUPER + ALT + W"),
  "Add Super+Alt+W to open World Clock")

const review = context.parseStatus(JSON.stringify({
  status_version: 1,
  supported: false,
  installed: false,
  shortcut_installed: false,
  agent_installed: false,
  default_shortcut: "SUPER + SHIFT + T",
  reason: "review_build",
}))
assert.equal(review.supported, false)
assert.equal(review.reason, "review_build")

assert.throws(() => context.parseStatus("not json"))
assert.throws(() => context.parseStatus(JSON.stringify({
  status_version: 1,
  supported: true,
  installed: true,
  shortcut_installed: false,
  agent_installed: true,
  default_shortcut: "SUPER + SHIFT + T",
})), /Inconsistent integration status/)

assert.match(context.failureMessage("Shortcut SUPER + SHIFT + T is already in use"),
  /already in use/)
assert.match(context.failureMessage("Refusing to replace existing command: /tmp/example"),
  /preserved/)
assert.match(context.failureMessage("Refusing to replace existing generic skill: /tmp/example"),
  /skill was preserved/)
assert.match(context.failureMessage("unrecognized failure"),
  /Nothing unrelated was replaced/)

console.log("Integration state tests passed.")
