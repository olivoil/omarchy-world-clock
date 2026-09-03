import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/WeatherDetailLogic.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

function hour(code, probability, extras = {}) {
  return {
    weather_code: code,
    precipitation_probability_percent: probability,
    temperature_celsius: 27,
    ...extras,
  }
}

const viewportHeight = 412
const scrollMaximum = 560
const oneNotchDown = context.wheelDistance(0, -120, viewportHeight, 64)
assert.ok(Math.abs(oneNotchDown) >= 180,
  "one mouse-wheel notch covers a useful fraction of the panel")
let scrollTarget = 0
for (let index = 0; index < 3; index++) {
  scrollTarget = context.nextScrollTarget(
    scrollTarget, scrollTarget, true, oneNotchDown, scrollMaximum)
}
assert.equal(scrollTarget, scrollMaximum,
  "three accumulated wheel notches can reach the bottom of a typical detail page")
assert.equal(context.wheelDistance(-18, 0, viewportHeight, 64), -27,
  "touchpad pixels are accelerated without being converted to coarse notches")

const steadyRain = Array.from({ length: 12 }, (_, index) =>
  hour(61, 78 + index % 3))
assert.deepEqual(Array.from(context.segmentPhases(steadyRain, 12, 3), group =>
  group.length), [12], "steady rain is one full-width pattern")

const clearThenRain = [
  ...Array.from({ length: 6 }, () => hour(1, 5)),
  ...Array.from({ length: 6 }, () => hour(61, 82)),
]
assert.deepEqual(Array.from(context.segmentPhases(clearThenRain, 12, 3), group =>
  group.length), [6, 6], "a persistent dry-to-rain transition creates two phases")

const clearStormClear = [
  ...Array.from({ length: 4 }, () => hour(1, 4)),
  ...Array.from({ length: 4 }, () => hour(95, 92)),
  ...Array.from({ length: 4 }, () => hour(1, 8)),
]
assert.deepEqual(Array.from(context.segmentPhases(clearStormClear, 12, 3), group =>
  group.length), [4, 4, 4], "a persistent storm window produces three calm columns")

const oneHourNoise = Array.from({ length: 12 }, () => hour(1, 6))
oneHourNoise[5] = hour(80, 72)
assert.deepEqual(Array.from(context.segmentPhases(oneHourNoise, 12, 3), group =>
  group.length), [12], "a one-hour forecast blip does not fragment the day")
assert.equal(context.weatherFamily(context.phaseRepresentative(oneHourNoise)),
  "clear", "a one-hour rain blip cannot rename an otherwise clear pattern")

const buildingThenEasing = [3, 10, 20, 35, 55, 75, 90, 96, 69, 50, 30, 24]
  .map(probability => hour(probability >= 35 ? 61 : 2, probability))
const rampGroups = context.segmentPhases(buildingThenEasing, 12, 3)
assert.equal(rampGroups.length, 3,
  "a material precipitation ramp and later easing can use all three phases")
assert.ok(rampGroups.every(group => group.length >= 2),
  "detected phases stay wide enough to read")

const dryHighUv = Array.from({ length: 24 }, (_, index) =>
  hour(1, 3, { uv_index: index === 12 ? 9 : 0, wind_speed_kmh: 8 }))
assert.equal(context.defaultMetric(dryHighUv), "uv",
  "high UV is the most relevant graph on an otherwise quiet day")

const imminentStorm = dryHighUv.map((item, index) => ({
  ...item,
  weather_code: index < 8 ? 95 : item.weather_code,
  precipitation_probability_percent: index < 8 ? 94 : 3,
}))
assert.equal(context.defaultMetric(imminentStorm), "precipitation",
  "an imminent storm outranks even very high UV")

const highWind = Array.from({ length: 24 }, (_, index) =>
  hour(2, 4, {
    uv_index: 2,
    wind_speed_kmh: index === 4 ? 78 : 24,
    wind_gusts_kmh: index === 4 ? 92 : 32,
  }))
assert.equal(context.defaultMetric(highWind), "wind",
  "dangerously high wind becomes the default graph")

const ordinaryDay = Array.from({ length: 24 }, (_, index) =>
  hour(2, 4, { temperature_celsius: 20 + Math.sin(index / 4), uv_index: 1 }))
assert.equal(context.defaultMetric(ordinaryDay), "temperature",
  "temperature remains the calm fallback when no signal dominates")

const clearFewHours = Array.from({ length: 4 }, () =>
  hour(0, 4, { precipitation_mm: 0 }))
assert.equal(context.clearSkiesHold(clearFewHours, 4), true,
  "a clear persistence claim requires several consistently clear forecast slots")
assert.equal(context.clearSkiesHold([
  hour(0, null),
  hour(1, null),
  hour(61, null, { precipitation_mm: 0.4 }),
  hour(61, null, { precipitation_mm: 0.7 }),
], 4), false,
  "incoming rain codes prevent a clear-skies claim when probabilities are missing")
assert.equal(context.clearSkiesHold([
  hour(0, 2), hour(1, 4), hour(3, 5), hour(3, 6),
], 4), false, "incoming cloud cover prevents a clear-skies persistence claim")
assert.equal(context.clearSkiesHold(clearFewHours.slice(0, 2), 4), false,
  "too little forecast evidence cannot support a next-few-hours claim")

assert.equal(context.comfortNote(38, 60),
  "It feels hot and humid right now.",
  "humid heat retains the combined description when humidity supports it")
assert.equal(context.comfortNote(38, 24), "It feels hot right now.",
  "dry heat is not described as humid")
assert.equal(context.comfortNote(38, null), "It feels hot right now.",
  "missing humidity cannot produce a humidity claim")
assert.equal(context.comfortNote(29, 82),
  "It feels warm and humid right now.",
  "warm humid conditions use present-observation wording")
assert.equal(context.comfortNote(21, 45),
  "Temperature and humidity are moderate right now.",
  "the ordinary fallback describes only the current observation")

assert.equal(context.humidityDescription(62, 26), "Warm and humid",
  "warm temperatures can support the combined atmosphere label")
assert.equal(context.humidityDescription(62, 4), "Elevated humidity",
  "cold weather is never labeled warm from humidity alone")
assert.equal(context.humidityDescription(62, null), "Elevated humidity",
  "missing temperature cannot support a warm atmosphere label")
assert.equal(context.humidityDescription(42, -8), "Moderate humidity",
  "midrange humidity uses temperature-neutral wording")

console.log("Weather detail logic tests passed.")
