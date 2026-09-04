import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/WeatherState.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

const previous = {
  locations: [
    { id: 1, temperature_celsius: 10 },
    { id: 17, temperature_celsius: 17 },
    { id: 99, temperature_celsius: 99 },
  ],
}
const partial = {
  partial: true,
  failed_locations: [{ id: 17 }],
  locations: [{ id: 1, temperature_celsius: 20 }],
}
const merged = context.mergePayload(previous, partial)

assert.deepEqual(Array.from(merged.locations, item => Number(item.id)), [1, 17],
  "only cached locations from failed batches are retained")
assert.deepEqual(Array.from(merged.locations,
  item => Number(item.temperature_celsius)), [20, 17],
  "fresh locations win while a failed batch keeps its cached reading")

const complete = {
  partial: false,
  locations: [{ id: 1, temperature_celsius: 21 }],
}
assert.equal(context.mergePayload(previous, complete), complete,
  "a complete response replaces the previous payload")

const duplicate = context.mergePayload(previous, {
  partial: true,
  failed_locations: [{ id: 17 }],
  locations: [{ id: 17, temperature_celsius: 22 }],
})
assert.deepEqual(Array.from(duplicate.locations,
  item => Number(item.temperature_celsius)), [22],
  "a fresh location is never duplicated by its cached counterpart")

const fallbackIdentity = context.mergePayload({
  locations: [{
    id: 0,
    timezone: "Arctic/Longyearbyen",
    label: "Longyearbyen",
    temperature_celsius: -8,
  }],
}, {
  partial: true,
  failed_locations: [{
    timezone: "Arctic/Longyearbyen",
    label: "Longyearbyen",
  }],
  locations: [],
})
assert.equal(fallbackIdentity.locations[0].temperature_celsius, -8,
  "an unconfigured local summary is retained by timezone and label")

console.log("Weather state tests passed.")
