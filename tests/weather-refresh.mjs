import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/WeatherRefresh.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

const minute = 60 * 1000
const freshness = 15 * minute
const cooldown = 2 * minute
const now = 20 * minute

function requestNeeded(overrides = {}) {
  return context.requestNeeded({
    manual: false,
    signature: "places:a",
    loadedSignature: "places:a",
    failed: false,
    lastUpdatedAt: now - 5 * minute,
    attemptedSignature: "places:a",
    lastAttemptAt: now - 5 * minute,
    now,
    freshnessMilliseconds: freshness,
    attemptCooldownMilliseconds: cooldown,
    ...overrides,
  })
}

assert.equal(requestNeeded({ signature: "" }), false,
  "weather is not requested before a snapshot provides a location signature")
assert.equal(requestNeeded(), false,
  "automatic checks reuse a successful response for the full freshness window")
assert.equal(requestNeeded({ lastUpdatedAt: now - freshness }), true,
  "automatic checks refresh at the freshness boundary")
assert.equal(requestNeeded({ manual: true }), true,
  "manual refresh bypasses a warm 15-minute cache")
assert.equal(requestNeeded({ manual: true, lastAttemptAt: now - minute }), false,
  "manual refresh still coalesces rapid repeat requests")
assert.equal(requestNeeded({ failed: true, lastAttemptAt: now - minute }), false,
  "failed requests observe the same short retry cooldown")
assert.equal(requestNeeded({ failed: true, lastAttemptAt: now - cooldown }), true,
  "failed requests retry at the cooldown boundary")
assert.equal(requestNeeded({
  loadedSignature: "places:b",
  attemptedSignature: "places:b",
  lastUpdatedAt: now,
  lastAttemptAt: now,
}), true, "a changed location set bypasses cache state for the previous signature")

console.log("Weather refresh tests passed.")
