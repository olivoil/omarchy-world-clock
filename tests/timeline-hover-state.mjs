import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/TimelineHoverState.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

let owners = {}
owners = context.updateOwners(owners, "timeline:0", -60, true)
owners = context.updateOwners(owners, "timeline:1", 0, true)
assert.equal(context.matchesMinutes(owners, -60), true,
  "overlapping hover targets keep the first point active")
assert.equal(context.matchesMinutes(owners, 0), true,
  "overlapping hover targets activate the second point")

owners = context.updateOwners(owners, "timeline:1", 0, false)
assert.equal(context.matchesMinutes(owners, -60), true,
  "leaving one target preserves another target that is still hovered")
assert.equal(context.matchesMinutes(owners, 0), false,
  "leaving a target removes only that target")

owners = context.updateOwners(owners, "timeline:0", -120, true)
assert.equal(context.matchesMinutes(owners, -60), false,
  "refreshing an owner removes its stale offset")
assert.equal(context.matchesMinutes(owners, -120), true,
  "refreshing an owner registers its current offset")

owners = context.updateOwners(owners, "timeline:0", -120, false)
assert.deepEqual(Object.keys(owners), [], "the final departure clears all hover state")

console.log("Timeline hover state tests passed.")
