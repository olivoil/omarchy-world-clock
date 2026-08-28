import assert from "node:assert/strict"
import fs from "node:fs"
import vm from "node:vm"

const sourceUrl = new URL("../quattro/TimeRail.js", import.meta.url)
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(sourceUrl, "utf8"), context, {
  filename: sourceUrl.pathname,
})

const payload = {
  step_minutes: 15,
  first_day_offset: -1,
  day_count: 3,
  slots: Array.from({ length: 288 }, (_, index) => ({
    day_offset: Math.floor(index / 96) - 1,
    minute: index % 96 * 15,
  })),
}
assert.equal(context.centeredSlotIndexAt(480, 960, payload, 589), 135,
  "the rail center snaps the 09:49 anchor to 09:45 on the current day")
assert.equal(context.centeredSlotIndexAt(0, 960, payload, 589), 87,
  "the left edge resolves to the previous source date")
assert.equal(context.centeredSlotIndexAt(960, 960, payload, 589), 183,
  "the right edge resolves to the current source date")
assert.equal(context.centeredMinutePosition(589, 589, 960), 480,
  "the current source minute is centered")
assert.equal(context.centeredMinutePosition(469, 589, 960), 400,
  "other wall times rotate around the centered source minute")
assert.ok(context.framePosition({ day_offset: -1, minute: 1305 }, 589, 960) < 0,
  "the previous-day snapped frame sits just outside the exact left boundary")
const ticks = context.axisTicks(589, "24h")
assert.equal(ticks.filter(tick => tick.major).map(tick => tick.label).join(","),
  "00,03,06,09,12,15,18,21")
assert.ok(ticks.find(tick => tick.label === "09").position < 0.5,
  "the 09:00 tick sits just before a 09:49 center anchor")
const ampmTicks = context.axisTicks(589, "ampm")
assert.equal(ampmTicks.filter(tick => tick.major).map(tick => tick.label).join(","),
  "12 AM,3 AM,6 AM,9 AM,12 PM,3 PM,6 PM,9 PM")
assert.deepEqual(ampmTicks.map(tick => tick.position), ticks.map(tick => tick.position),
  "changing the display format does not move the ticks")

const base = {
  reference_utc: "live",
  summary: {
    timezone: "America/Cancun", title: "Cancun", time: "10:00", date: "2026-08-28",
  },
  clocks: [{
    timezone: "Asia/Tokyo", title: "Tokyo", time: "00:00", date: "2026-08-29",
  }],
  featured_cities: [{ title: "Paris" }],
}
const matchingLocations = {
  locations: [
    { timezone: "America/Cancun", label: "Cancun" },
    { timezone: "Asia/Tokyo", label: "Tokyo" },
  ],
}
assert.equal(context.payloadMatchesSnapshot(matchingLocations, base), true,
  "ordered backend identities authenticate against the displayed snapshot")
assert.equal(context.payloadMatchesSnapshot({
  locations: matchingLocations.locations.slice().reverse(),
}, base), false, "reordered backend clocks are rejected before positional merging")
assert.equal(context.payloadMatchesSnapshot({
  locations: [matchingLocations.locations[0], { timezone: "Asia/Tokyo", label: "Osaka" }],
}, base), false, "a backend clock with a different identity is rejected")
const frame = {
  day_offset: 0,
  reference_utc: "2026-08-28T16:00:00Z",
  summary: {
    time: "11:00", notation: "EST", local_minutes: 660, source_day_offset: 0,
  },
  clocks: [{
    time: "01:00", notation: "JST", local_minutes: 60, source_day_offset: 1,
  }],
}
const merged = context.mergeSnapshot(base, frame)
assert.equal(merged.summary.title, "Cancun", "static summary identity is preserved")
assert.equal(merged.summary.time, "11:00", "summary time comes from the selected frame")
assert.equal(merged.clocks[0].title, "Tokyo", "static card identity is preserved")
assert.equal(merged.clocks[0].time, "01:00", "card time comes from the selected frame")
assert.deepEqual(merged.featured_cities, base.featured_cities,
  "large static map data is not duplicated by the scrub payload")
assert.equal(context.mergeSnapshot(base, { summary: {}, clocks: [] }), null,
  "a stale payload with different visible clocks is rejected")
assert.equal(context.relativeDayLabel(merged.clocks[0], merged.summary), "Next day",
  "scrub frames describe dates relative to the selected rail source")
assert.equal(context.relativeDayLabel({ source_day_offset: -1 }, {}), "Previous day")
assert.equal(context.relativeDayLabel({ source_day_offset: 0 }, {}), "Same day")
assert.equal(context.relativeDayLabel({
  date: "2026-08-30",
}, {
  date: "2026-08-28",
}), "2 days later", "locked snapshots fall back to their absolute dates")
assert.equal(context.compactDateLabel("2026-08-28", 1), "SAT 29")
assert.equal(context.compactDateLabel("2026-08-31", 1), "TUE 1",
  "compact dates cross month boundaries")
assert.equal(context.selectionLabel({ date: "2026-08-28" }, {
  day_offset: 1, label: "00:45", ambiguous: false,
}, false), "SAT 29  ·  00:45", "the selected time includes its source date")
assert.equal(context.selectionLabel({ date: "2026-11-01" }, {
  day_offset: 0, label: "01:30", ambiguous: true,
}, false), "SUN 1  ·  01:30  ·  FIRST")

const markers = context.buildMarkers(merged, "America/Cancun", 660)
assert.equal(markers.length, 2)
assert.equal(markers[0].minute, 60)
assert.equal(markers[0].label, "JST +1")
assert.equal(markers[1].source, true)
assert.equal(markers[1].label, "EST")

const grouped = context.buildMarkers({
  summary: { timezone: "UTC", time: "12:00", notation: "UTC", local_minutes: 720 },
  clocks: [
    { timezone: "Etc/UTC", time: "12:00", notation: "UTC", local_minutes: 720 },
    { timezone: "Europe/London", time: "12:00", notation: "GMT", local_minutes: 720 },
  ],
}, "UTC", 720)
assert.equal(grouped.length, 1)
assert.equal(grouped[0].count, 3)
assert.equal(grouped[0].label, "GMT / UTC")

console.log("Time rail tests passed.")
