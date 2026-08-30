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
assert.equal(context.localDayPosition(0), 0,
  "midnight starts the local-day ruler")
assert.equal(context.localDayPosition(6 * 60), 0.25,
  "06:00 sits at the first quarter of the local-day ruler")
assert.equal(context.localDayPosition(12 * 60), 0.5,
  "noon sits at the center of the local-day ruler")
assert.equal(context.localDayPosition(18 * 60), 0.75,
  "18:00 sits at the third quarter of the local-day ruler")
assert.equal(context.localDayPosition(24 * 60), 1,
  "the helper permits the conceptual 24:00 endpoint")
assert.equal(context.localDayPosition(-30), 0,
  "local-day positions stay inside the left edge")
assert.equal(context.localDayPosition(25 * 60), 1,
  "local-day positions stay inside the right edge")
assert.equal(context.localDayPosition("unknown"), 0,
  "invalid local minutes fail safely at the start of the ruler")
assert.equal(context.draggedSlotIndexAt(0, 960, payload, 135), 135,
  "pressing the fixed-center ruler does not jump the selected instant")
assert.equal(context.draggedSlotIndexAt(1, 960, payload, 135), 135,
  "sub-slot pointer jitter does not count as a changed selection")
assert.equal(context.draggedSlotIndexAt(480, 960, payload, 135), 87,
  "pulling the ruler right brings twelve-hours-earlier instants to center")
assert.equal(context.draggedSlotIndexAt(-480, 960, payload, 135), 183,
  "pulling the ruler left brings twelve-hours-later instants to center")
assert.equal(context.draggedSlotIndexAt(960, 960, payload, 5), 0,
  "fixed-center dragging remains bounded by the earliest precomputed frame")
assert.equal(context.draggedSlotIndexAt(-960, 960, payload, 280), 287,
  "fixed-center dragging remains bounded by the latest precomputed frame")
assert.equal(context.wheelSlotMotion(480, 0, 0, 0, 960, payload), -48,
  "a horizontal touchpad gesture follows the ruler's direct-manipulation direction")
assert.equal(context.wheelSlotMotion(-480, 0, 0, 0, 960, payload), 48,
  "a leftward touchpad gesture advances the selected time")
assert.equal(context.wheelSlotMotion(0, 20, 0, 0, 960, payload), -2,
  "vertical high-resolution scrolling up moves to earlier times")
assert.equal(context.wheelSlotMotion(0, 0, 0, 120, 960, payload), -4,
  "one mouse-wheel notch up moves one hour earlier")
assert.equal(context.wheelSlotMotion(0, 0, 0, -120, 960, payload), 4,
  "one mouse-wheel notch down moves one hour later")
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
  time_format: "24h",
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
    {
      timezone: "America/Cancun", label: "Cancun",
      states: [{ from_slot: 0, utc_offset_seconds: -18000, notation: "EST" }],
    },
    {
      timezone: "Asia/Tokyo", label: "Tokyo",
      states: [{ from_slot: 0, utc_offset_seconds: 32400, notation: "JST" }],
    },
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
const nextDayFrame = {
  day_offset: 1,
  reference_utc: "2026-08-29T05:00:00Z",
}
const scrubPayload = {
  ...matchingLocations,
  source_timezone: "America/Cancun",
  date: "2026-08-28",
  time_format: "24h",
  slots: [nextDayFrame],
}
const nextDayPreview = context.mergeSnapshot(base, scrubPayload, 0)
assert.equal(context.scrubPayloadReady(scrubPayload, nextDayPreview, base,
  "America/Cancun", "America/Cancun\u001fCancun"), true,
"the rail stays ready against its stable drag-start snapshot across midnight")
const frame = {
  day_offset: 0,
  reference_utc: "2026-08-28T16:00:00Z",
}
const framePayload = { ...scrubPayload, slots: [frame] }
const merged = context.mergeSnapshot(base, framePayload, 0)
assert.equal(merged.summary.title, "Cancun", "static summary identity is preserved")
assert.equal(merged.summary.time, "11:00", "summary time comes from the selected frame")
assert.equal(merged.clocks[0].title, "Tokyo", "static card identity is preserved")
assert.equal(merged.clocks[0].time, "01:00", "card time comes from the selected frame")
assert.deepEqual(merged.featured_cities, base.featured_cities,
  "large static map data is not duplicated by the scrub payload")
assert.equal(context.mergeSnapshot(base, {
  ...framePayload,
  locations: [matchingLocations.locations[0]],
}, 0), null,
  "a stale payload with different visible clocks is rejected")
assert.equal(context.relativeDayLabel(merged.clocks[0], merged.summary), "Next day",
  "scrub frames describe dates relative to the selected rail source")
assert.equal(context.relativeDayLabel({ source_day_offset: -1 }, {}), "Previous day")
assert.equal(context.relativeDayLabel({ source_day_offset: 0 }, {}), "Same day")
assert.equal(context.relativeMinuteForClock({
  local_minutes: 600, source_day_offset: 1,
}, {
  local_minutes: 600, source_day_offset: 0,
}, 600), 1440,
"equal wall-clock minutes on adjacent days retain distinct rail positions")
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

const dstPayload = {
  source_timezone: "UTC",
  date: "2026-03-08",
  time_format: "24h",
  locations: [{
    timezone: "UTC", label: "UTC",
    states: [{ from_slot: 0, utc_offset_seconds: 0, notation: "UTC" }],
  }, {
    timezone: "America/New_York", label: "New York",
    states: [
      { from_slot: 0, utc_offset_seconds: -18000, notation: "EST" },
      { from_slot: 1, utc_offset_seconds: -14400, notation: "EDT" },
    ],
  }],
  slots: [
    { day_offset: 0, reference_utc: "2026-03-08T06:30:00Z" },
    { day_offset: 0, reference_utc: "2026-03-08T07:30:00Z" },
  ],
}
const dstBase = {
  reference_utc: "2026-03-08T06:30:00Z",
  summary: { timezone: "UTC", title: "UTC" },
  clocks: [{ timezone: "America/New_York", title: "New York" }],
}
assert.equal(context.mergeSnapshot(dstBase, dstPayload, 0).clocks[0].time, "01:30")
const afterDst = context.mergeSnapshot(dstBase, dstPayload, 1)
assert.equal(afterDst.clocks[0].time, "03:30",
  "compact timezone states preserve a location's DST jump")
assert.equal(afterDst.clocks[0].notation, "EDT")
assert.equal(afterDst.clocks[0].relative_minutes, -240)

const historicalPayload = {
  source_timezone: "Asia/Kolkata",
  date: "1900-01-01",
  time_format: "24h",
  locations: [{
    timezone: "Asia/Kolkata", label: "Kolkata",
    states: [{ from_slot: 0, utc_offset_seconds: 19270, notation: "MMT" }],
  }],
  slots: [{
    day_offset: 0,
    reference_utc: "1900-01-01T06:38:50+00:00",
  }],
}
const historicalPreview = context.mergeSnapshot({
  reference_utc: "1900-01-01T06:38:50+00:00",
  summary: { timezone: "Asia/Kolkata", title: "Kolkata" },
  clocks: [],
}, historicalPayload, 0)
assert.equal(historicalPreview.summary.time, "12:00",
  "second-level historical UTC offsets do not scrub into the previous minute")

const markers = context.buildMarkers(merged, "America/Cancun", 660)
assert.equal(markers.length, 2)
assert.equal(markers[0].source, true)
assert.equal(markers[0].identities.join(), "America/Cancun\u001fCancun")
assert.equal(markers[0].position, 0.5,
  "the selected source timezone remains under the fixed playhead")
assert.equal(markers[1].minute, 60)
assert.equal(markers[1].identities.join(), "Asia/Tokyo\u001fTokyo")
assert.equal(markers[1].label, "JST · +14H")
assert.equal(markers[1].overflow, "next")
assert.ok(markers[1].position > 1,
  "a next-day timezone stays after the ruler instead of wrapping left")

const overflowMarkers = context.buildMarkers({
  summary: {
    timezone: "America/Cancun", time: "11:00", date: "2026-08-28",
    notation: "EST", local_minutes: 660,
  },
  clocks: [
    {
      timezone: "Asia/Tokyo", time: "01:00", date: "2026-08-29",
      notation: "JST", local_minutes: 60,
    },
    {
      timezone: "Australia/Sydney", time: "03:00", date: "2026-08-29",
      notation: "AEST", local_minutes: 180,
    },
  ],
}, "America/Cancun", 660)
assert.equal(overflowMarkers.length, 2,
  "multiple next-day zones share one readable overflow marker")
assert.equal(overflowMarkers[1].overflow, "next")
assert.equal(overflowMarkers[1].count, 2)
assert.equal(overflowMarkers[1].label, "2 ZONES · +14–16H")
assert.deepEqual(Array.from(overflowMarkers[1].minutes), [60, 180])
assert.deepEqual(Array.from(overflowMarkers[1].relative_minutes), [840, 960],
  "an overflow summary keeps the real wall-clock distances it represents")
assert.equal(context.overflowDistanceLabel([780, 900, 1020], "next"), "+13–17H",
  "whole-hour overflow ranges use the compact edge-marker notation")

const previousDayMarkers = context.buildMarkers({
  summary: {
    timezone: "Pacific/Auckland", time: "09:00", date: "2026-08-29",
    notation: "NZST", local_minutes: 540,
  },
  clocks: [{
    timezone: "America/Los_Angeles", time: "14:00", date: "2026-08-28",
    notation: "PDT", local_minutes: 840,
  }],
}, "Pacific/Auckland", 540)
assert.equal(previousDayMarkers[0].overflow, "previous")
assert.ok(previousDayMarkers[0].position < 0,
  "a previous-day timezone stays before the ruler")
assert.equal(previousDayMarkers[0].label, "PDT · −19H")

const fractionalOverflow = context.buildMarkers({
  summary: {
    timezone: "America/Cancun", time: "11:00", date: "2026-08-28",
    notation: "EST", local_minutes: 660,
  },
  clocks: [{
    timezone: "Pacific/Chatham", time: "05:45", date: "2026-08-29",
    notation: "CHAST", local_minutes: 345,
  }],
}, "America/Cancun", 660)
assert.equal(fractionalOverflow[1].label, "CHAST · +18H45M",
  "non-hour timezone distances remain exact at the edge")

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
assert.equal(grouped[0].source, true,
  "a same-time group retains its source identity for the fixed playhead")
assert.deepEqual(Array.from(grouped[0].minutes), [720],
  "the shared source marker retains the card minute used by hover linking")

console.log("Time rail tests passed.")
