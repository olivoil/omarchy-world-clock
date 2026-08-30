import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/TimelineHoverState.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

let owners = {}
owners = context.updateOwners(owners, "timeline:0", "zone:a", true)
owners = context.updateOwners(owners, "timeline:1", "zone:b", true)
assert.equal(context.matchesIdentity(owners, "zone:a"), true,
  "overlapping hover targets keep the first point active")
assert.equal(context.matchesIdentity(owners, "zone:b"), true,
  "overlapping hover targets activate the second point")

owners = context.updateOwners(owners, "timeline:1", "", false)
assert.equal(context.matchesIdentity(owners, "zone:a"), true,
  "leaving one target preserves another target that is still hovered")
assert.equal(context.matchesIdentity(owners, "zone:b"), false,
  "leaving a target removes only that target")

owners = context.updateOwners(owners, "timeline:0", "zone:c", true)
assert.equal(context.matchesIdentity(owners, "zone:a"), false,
  "refreshing an owner removes its stale identity")
assert.equal(context.matchesIdentity(owners, "zone:c"), true,
  "refreshing an owner registers its current identity")

owners = context.updateOwners(owners, "timeline:0", "", false)
assert.deepEqual(Object.keys(owners), [], "the final departure clears all hover state")

let sharedSourceOwners = {}
sharedSourceOwners = context.updateOwners(
  sharedSourceOwners, "card:austin", "America/Chicago\u001fAustin", true)
const sharedSourceMarkers = [
  {
    source: true,
    identities: ["America/Cancun\u001fHome", "America/Chicago\u001fAustin"],
    count: 2,
  },
  { source: false, identities: ["America/New_York\u001fNew York"], count: 1 },
]
assert.equal(context.sourceMarkerHovered(sharedSourceOwners, sharedSourceMarkers), true,
  "hovering a card grouped with the source reaches the fixed playhead")

const qmlBridgedMarker = {
  source: false,
  identities: { 0: "America/Vancouver\u001fSan Francisco", length: 1 },
  count: 1,
}
let qmlBridgedOwners = {}
qmlBridgedOwners = context.updateOwners(qmlBridgedOwners, "card:san-francisco",
  "America/Vancouver\u001fSan Francisco", true)
assert.equal(context.markerHovered(qmlBridgedOwners, qmlBridgedMarker), true,
  "a QVariantList identity collection still links a card to its marker")
assert.equal(context.sourceMarkerHovered(sharedSourceOwners, {
  0: sharedSourceMarkers[0], 1: sharedSourceMarkers[1], length: 2,
}), true, "a QVariantList marker collection still reaches the fixed playhead")

let nextDayOwners = {}
nextDayOwners = context.updateOwners(nextDayOwners, "card:kiritimati",
  "Pacific/Kiritimati\u001fKiritimati", true)
const sameMinuteDifferentDayMarkers = [
  {
    source: true, minutes: [600], relative_minutes: [0],
    identities: ["Etc/GMT+10\u001fHonolulu"], count: 1,
  },
  {
    source: false, minutes: [600], relative_minutes: [1440],
    identities: ["Pacific/Kiritimati\u001fKiritimati"], count: 1,
  },
]
assert.equal(context.markerHovered(nextDayOwners, sameMinuteDifferentDayMarkers[1]), true,
  "a next-day card reaches its calendar-aware marker")
assert.equal(context.sourceMarkerHovered(nextDayOwners, sameMinuteDifferentDayMarkers), false,
  "sharing a wrapped clock minute does not highlight the source on another day")

console.log("Timeline hover state tests passed.")
