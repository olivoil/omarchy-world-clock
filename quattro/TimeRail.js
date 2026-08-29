function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(maximum, Number(value)))
}

var DAY_MINUTES = 24 * 60

function wrapMinute(value) {
  return ((Number(value || 0) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES
}

function signedMinuteDelta(minute, anchorMinute) {
  return wrapMinute(Number(minute || 0) - Number(anchorMinute || 0) + DAY_MINUTES / 2)
    - DAY_MINUTES / 2
}

function centeredMinutePosition(minute, anchorMinute, width) {
  var normalized = (signedMinuteDelta(minute, anchorMinute) + DAY_MINUTES / 2)
    / DAY_MINUTES
  return normalized * Math.max(0, Number(width || 0))
}

function framePosition(frame, anchorMinute, width) {
  if (!frame) return 0
  var start = Number(anchorMinute || 0) - DAY_MINUTES / 2
  var unwrapped = Number(frame.day_offset || 0) * DAY_MINUTES
    + Number(frame.minute || 0)
  return (unwrapped - start) / DAY_MINUTES * Math.max(0, Number(width || 0))
}

function slotsPerDay(payload) {
  var step = Math.max(1, Number(payload && payload.step_minutes || 15))
  return Math.round(DAY_MINUTES / step)
}

function slotIndexFor(payload, dayOffset, minute) {
  if (!payload || !Array.isArray(payload.slots) || payload.slots.length === 0) return 0
  var step = Math.max(1, Number(payload.step_minutes || 15))
  var firstDay = Number(payload.first_day_offset || 0)
  var snapped = Math.round(Number(minute || 0) / step) * step
  var normalizedDay = Number(dayOffset || 0) + Math.floor(snapped / DAY_MINUTES)
  var normalizedMinute = wrapMinute(snapped)
  var index = (normalizedDay - firstDay) * slotsPerDay(payload)
    + Math.round(normalizedMinute / step)
  return Math.max(0, Math.min(payload.slots.length - 1, index))
}

function centeredSlotIndexAt(position, width, payload, anchorMinute) {
  var extent = Math.max(1, Number(width || 0))
  var ratio = clamp(Number(position || 0) / extent, 0, 1)
  var unwrapped = Number(anchorMinute || 0) - DAY_MINUTES / 2
    + ratio * DAY_MINUTES
  var dayOffset = Math.floor(unwrapped / DAY_MINUTES)
  return slotIndexFor(payload, dayOffset, unwrapped - dayOffset * DAY_MINUTES)
}

function frameLocationAt(frame, locationIndex) {
  var index = Math.max(0, Math.floor(Number(locationIndex || 0)))
  if (!frame) return null
  if (index === 0) return frame.summary || null
  if (!Array.isArray(frame.clocks) || index > frame.clocks.length) return null
  return frame.clocks[index - 1] || null
}

function availabilitySlotIndexAt(position, width, payload, anchorMinute,
                                 locationIndex, currentIndex) {
  if (!payload || !Array.isArray(payload.slots) || payload.slots.length === 0)
    return 0
  var slots = payload.slots
  var current = Math.max(0, Math.min(slots.length - 1,
    Math.round(Number(currentIndex || 0))))
  var step = Math.max(1, Number(payload.step_minutes || 15))
  var maximumMinute = DAY_MINUTES - step
  var ratio = clamp(Number(position || 0) / Math.max(1, Number(width || 0)), 0, 1)
  var targetMinute = clamp(Math.round(ratio * maximumMinute / step) * step,
    0, maximumMinute)
  var bestIndex = current
  var bestWallDelta = Number.POSITIVE_INFINITY
  var bestIndexDelta = Number.POSITIVE_INFINITY

  for (var index = 0; index < slots.length; index++) {
    var frame = slots[index]
    var railPosition = framePosition(frame, anchorMinute, 1)
    if (!frame || !frame.reference_utc || railPosition < 0 || railPosition > 1)
      continue
    var location = frameLocationAt(frame, locationIndex)
    var minute = Number(location && location.local_minutes)
    if (!isFinite(minute)) continue
    var wallDelta = Math.abs(signedMinuteDelta(minute, targetMinute))
    var indexDelta = Math.abs(index - current)
    if (wallDelta < bestWallDelta
        || wallDelta === bestWallDelta && indexDelta < bestIndexDelta) {
      bestIndex = index
      bestWallDelta = wallDelta
      bestIndexDelta = indexDelta
    }
  }
  return bestIndex
}

function axisHourLabel(hour, timeFormat) {
  if (String(timeFormat || "").toLowerCase() === "ampm") {
    var twelveHour = hour % 12 || 12
    return String(twelveHour) + (hour < 12 ? " AM" : " PM")
  }
  return hour < 10 ? "0" + String(hour) : String(hour)
}

function axisTicks(anchorMinute, timeFormat) {
  var start = Number(anchorMinute || 0) - DAY_MINUTES / 2
  var end = start + DAY_MINUTES
  var firstHour = Math.ceil(start / 60) * 60
  var ticks = []
  for (var value = firstHour; value <= end; value += 60) {
    var minute = wrapMinute(value)
    var hour = Math.floor(minute / 60)
    var major = hour % 3 === 0
    ticks.push({
      position: (value - start) / DAY_MINUTES,
      major: major,
      label: major ? axisHourLabel(hour, timeFormat) : ""
    })
  }
  return ticks
}

function mergeRecord(base, dynamic) {
  var result = ({})
  var key
  for (key in (base || ({})))
    if (Object.prototype.hasOwnProperty.call(base, key)) result[key] = base[key]
  for (key in (dynamic || ({})))
    if (Object.prototype.hasOwnProperty.call(dynamic, key)) result[key] = dynamic[key]
  return result
}

function locationIdentity(location) {
  if (!location) return ""
  var label = location.label !== null && location.label !== undefined
    ? location.label : location.title
  return String(location.timezone || "") + "\u001f" + String(label || "")
}

function payloadMatchesSnapshot(payload, snapshot) {
  if (!payload || !Array.isArray(payload.locations)
      || !snapshot || !snapshot.summary || !Array.isArray(snapshot.clocks)) return false
  var snapshotLocations = [snapshot.summary].concat(snapshot.clocks)
  if (payload.locations.length !== snapshotLocations.length) return false
  for (var index = 0; index < snapshotLocations.length; index++)
    if (locationIdentity(payload.locations[index]) !== locationIdentity(snapshotLocations[index]))
      return false
  return true
}

function scrubPayloadReady(payload, snapshot, baseSnapshot, sourceTimezone, sourceKey) {
  if (!payload || payload.source_timezone !== sourceTimezone
      || !Array.isArray(payload.slots) || payload.slots.length === 0) return false

  // During a drag, rendered frames replace the displayed dates. Authenticate
  // the payload against the stable snapshot captured when the drag began.
  var validationSnapshot = baseSnapshot || snapshot
  if (!validationSnapshot || !validationSnapshot.summary
      || !Array.isArray(validationSnapshot.clocks)) return false
  var entries = [validationSnapshot.summary].concat(validationSnapshot.clocks)
  var sourceClock = null
  for (var index = 0; index < entries.length; index++) {
    if (locationIdentity(entries[index]) === sourceKey) {
      sourceClock = entries[index]
      break
    }
  }
  if (!sourceClock) {
    for (var timezoneIndex = 0; timezoneIndex < entries.length; timezoneIndex++) {
      if (String(entries[timezoneIndex].timezone || "") === sourceTimezone) {
        sourceClock = entries[timezoneIndex]
        break
      }
    }
  }
  if (!sourceClock) return false
  return String(payload.date || "") === String(sourceClock.date || "")
    && payloadMatchesSnapshot(payload, validationSnapshot)
    && String(payload.time_format || "")
      === String(validationSnapshot.time_format || "24h")
}

function mergeSnapshot(base, frame) {
  if (!base || !frame || !frame.summary || !Array.isArray(frame.clocks)
      || !Array.isArray(base.clocks) || frame.clocks.length !== base.clocks.length)
    return null

  var result = mergeRecord(base, ({ reference_utc: frame.reference_utc }))
  result.scrub_day_offset = Number(frame.day_offset || 0)
  result.summary = mergeRecord(base.summary, frame.summary)
  result.clocks = []
  for (var index = 0; index < base.clocks.length; index++)
    result.clocks.push(mergeRecord(base.clocks[index], frame.clocks[index]))
  return result
}

function daySuffix(offset) {
  var value = Number(offset || 0)
  if (value === 0) return ""
  return value > 0 ? "+" + String(value) : String(value)
}

function dateDayOffset(date, sourceDate) {
  var value = Date.parse(String(date || "") + "T00:00:00Z")
  var source = Date.parse(String(sourceDate || "") + "T00:00:00Z")
  if (!isFinite(value) || !isFinite(source)) return 0
  return Math.round((value - source) / (24 * 60 * 60 * 1000))
}

function relativeDayLabel(clock, sourceClock) {
  if (!clock) return ""
  var hasSourceOffset = clock.source_day_offset !== undefined
    && isFinite(Number(clock.source_day_offset))
  var hasDates = sourceClock
    && String(clock.date || "") !== ""
    && String(sourceClock.date || "") !== ""
  if (!hasSourceOffset && !hasDates) return ""

  var offset = hasSourceOffset
    ? Math.round(Number(clock.source_day_offset))
    : dateDayOffset(clock.date, sourceClock.date)
  if (offset === -1) return "Previous day"
  if (offset === 0) return "Same day"
  if (offset === 1) return "Next day"
  return String(Math.abs(offset)) + (offset < 0 ? " days earlier" : " days later")
}

function compactDateLabel(date, dayOffset) {
  var match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(date || ""))
  if (!match) return ""
  var timestamp = Date.UTC(
    Number(match[1]), Number(match[2]) - 1, Number(match[3]) + Number(dayOffset || 0))
  var value = new Date(timestamp)
  if (!isFinite(value.getTime())) return ""
  var weekdays = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
  return weekdays[value.getUTCDay()] + " " + String(value.getUTCDate())
}

function selectionLabel(payload, frame, unavailable) {
  if (!frame) return ""
  var date = compactDateLabel(payload && payload.date, frame.day_offset)
  var time = unavailable === true ? "CLOCK CHANGE" : String(frame.label || "")
  if (frame.ambiguous === true && unavailable !== true) time += "  ·  FIRST"
  if (date && time) return date + "  ·  " + time
  return date || time
}

function markerLabel(clock, sourceDate) {
  var notation = String(clock && clock.notation || "").toUpperCase()
  var offset = clock && clock.source_day_offset !== undefined
    ? clock.source_day_offset : dateDayOffset(clock && clock.date, sourceDate)
  var suffix = daySuffix(offset)
  return notation + (suffix ? " " + suffix : "")
}

function buildMarkers(snapshot, sourceTimezone, anchorMinute) {
  if (!snapshot || !snapshot.summary) return []
  var clocks = [snapshot.summary]
  if (Array.isArray(snapshot.clocks)) clocks = clocks.concat(snapshot.clocks)
  var groups = ({})
  var sourceDate = ""
  for (var sourceIndex = 0; sourceIndex < clocks.length; sourceIndex++) {
    if (String(clocks[sourceIndex].timezone || "") === String(sourceTimezone || "")) {
      sourceDate = String(clocks[sourceIndex].date || "")
      break
    }
  }

  for (var index = 0; index < clocks.length; index++) {
    var clock = clocks[index] || ({})
    var minute = Math.round(Number(clock.local_minutes))
    if (!isFinite(minute) || minute < 0 || minute >= 24 * 60) continue
    var key = String(minute)
    var group = groups[key]
    if (!group) {
      group = {
        minute: minute,
        position: centeredMinutePosition(minute, anchorMinute, 1),
        time: String(clock.time || ""),
        labels: [],
        source: false,
        count: 0,
        lane: 0
      }
      groups[key] = group
    }
    var label = markerLabel(clock, sourceDate)
    if (label && group.labels.indexOf(label) === -1) group.labels.push(label)
    if (String(clock.timezone || "") === String(sourceTimezone || "")) group.source = true
    group.count += 1
  }

  var markers = []
  for (var minuteKey in groups) {
    if (!Object.prototype.hasOwnProperty.call(groups, minuteKey)) continue
    var marker = groups[minuteKey]
    marker.labels.sort()
    marker.label = marker.labels.join(" / ")
    delete marker.labels
    markers.push(marker)
  }
  markers.sort(function(left, right) { return left.position - right.position })

  var laneEnds = [-100000, -100000]
  for (var markerIndex = 0; markerIndex < markers.length; markerIndex++) {
    var selectedLane = -1
    for (var lane = 0; lane < laneEnds.length; lane++) {
      if (markers[markerIndex].position * DAY_MINUTES - laneEnds[lane] >= 105) {
        selectedLane = lane
        break
      }
    }
    if (selectedLane < 0) selectedLane = laneEnds[0] <= laneEnds[1] ? 0 : 1
    markers[markerIndex].lane = selectedLane
    laneEnds[selectedLane] = markers[markerIndex].position * DAY_MINUTES
  }
  return markers
}
