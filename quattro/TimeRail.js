var DAY_MINUTES = 24 * 60

function wrapMinute(value) {
  return ((Number(value || 0) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES
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

function draggedSlotIndexAt(delta, width, payload, startIndex) {
  if (!payload || !Array.isArray(payload.slots) || payload.slots.length === 0)
    return 0
  var step = Math.max(1, Number(payload.step_minutes || 15))
  var extent = Math.max(1, Number(width || 0))
  var start = Math.max(0, Math.min(payload.slots.length - 1,
    Math.round(Number(startIndex || 0))))
  // The playhead stays fixed, so dragging manipulates the ruler itself:
  // pulling it right brings earlier instants under the center, and vice versa.
  var slotDelta = -Math.round(Number(delta || 0) / extent * DAY_MINUTES / step)
  return Math.max(0, Math.min(payload.slots.length - 1, start + slotDelta))
}

function wheelSlotMotion(pixelX, pixelY, angleX, angleY, width, payload) {
  var perDay = slotsPerDay(payload)
  var extent = Math.max(1, Number(width || 0))
  var horizontalPixels = Number(pixelX || 0)
  var verticalPixels = Number(pixelY || 0)
  if (!isFinite(horizontalPixels)) horizontalPixels = 0
  if (!isFinite(verticalPixels)) verticalPixels = 0
  if (horizontalPixels !== 0 || verticalPixels !== 0) {
    var pixels = Math.abs(horizontalPixels) >= Math.abs(verticalPixels)
      ? horizontalPixels : verticalPixels
    return -pixels / extent * perDay
  }

  var horizontalAngle = Number(angleX || 0)
  var verticalAngle = Number(angleY || 0)
  if (!isFinite(horizontalAngle)) horizontalAngle = 0
  if (!isFinite(verticalAngle)) verticalAngle = 0
  var angle = Math.abs(horizontalAngle) >= Math.abs(verticalAngle)
    ? horizontalAngle : verticalAngle
  // A conventional wheel notch covers one hour. Pixel deltas from touchpads
  // remain proportional to the visible ruler for direct manipulation.
  return -angle / 120 * Math.max(1, Math.round(60
    / Math.max(1, Number(payload && payload.step_minutes || 15))))
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

function relativeDayOffset(clock, sourceClock) {
  if (!clock) return 0
  var clockHasOffset = clock.source_day_offset !== undefined
    && isFinite(Number(clock.source_day_offset))
  var sourceHasOffset = sourceClock && sourceClock.source_day_offset !== undefined
    && isFinite(Number(sourceClock.source_day_offset))
  if (clockHasOffset && sourceHasOffset)
    return Math.round(Number(clock.source_day_offset)
      - Number(sourceClock.source_day_offset))
  if (clockHasOffset) return Math.round(Number(clock.source_day_offset))
  if (sourceClock) return dateDayOffset(clock.date, sourceClock.date)
  return 0
}

function markerLabel(clock, sourceClock) {
  var notation = String(clock && clock.notation || "").toUpperCase()
  var offset = relativeDayOffset(clock, sourceClock)
  var suffix = daySuffix(offset)
  return notation + (suffix ? " " + suffix : "")
}

function distanceMagnitudeLabel(minutes) {
  var total = Math.round(Math.abs(Number(minutes || 0)))
  var hours = Math.floor(total / 60)
  var remainder = total % 60
  if (remainder === 0) return String(hours) + "H"
  return String(hours) + "H" + (remainder < 10 ? "0" : "")
    + String(remainder) + "M"
}

function overflowDistanceLabel(relativeMinutes, direction) {
  if (!Array.isArray(relativeMinutes) || relativeMinutes.length === 0) return ""
  var magnitudes = []
  for (var index = 0; index < relativeMinutes.length; index++)
    magnitudes.push(Math.abs(Math.round(Number(relativeMinutes[index] || 0))))
  magnitudes.sort(function(left, right) { return left - right })
  var nearest = magnitudes[0]
  var farthest = magnitudes[magnitudes.length - 1]
  var sign = direction === "previous" ? "−" : "+"
  if (nearest === farthest) return sign + distanceMagnitudeLabel(nearest)
  if (nearest % 60 === 0 && farthest % 60 === 0)
    return sign + String(nearest / 60) + "–" + String(farthest / 60) + "H"
  return sign + distanceMagnitudeLabel(nearest)
    + "–" + distanceMagnitudeLabel(farthest)
}

function overflowMarkerLabel(marker) {
  var distance = overflowDistanceLabel(marker.relative_minutes, marker.overflow)
  if (marker.count > 1) return String(marker.count) + " ZONES · " + distance
  var notation = marker.notations.length > 0 ? marker.notations[0] : "1 ZONE"
  return notation + " · " + distance
}

function buildMarkers(snapshot, sourceTimezone, anchorMinute) {
  if (!snapshot || !snapshot.summary) return []
  var clocks = [snapshot.summary]
  if (Array.isArray(snapshot.clocks)) clocks = clocks.concat(snapshot.clocks)
  var groups = ({})
  var sourceClock = null
  for (var sourceIndex = 0; sourceIndex < clocks.length; sourceIndex++) {
    if (String(clocks[sourceIndex].timezone || "") === String(sourceTimezone || "")) {
      sourceClock = clocks[sourceIndex]
      break
    }
  }
  if (!sourceClock) sourceClock = snapshot.summary
  var sourceMinute = Number(anchorMinute)
  if (!isFinite(sourceMinute)) sourceMinute = Number(sourceClock.local_minutes || 0)

  for (var index = 0; index < clocks.length; index++) {
    var clock = clocks[index] || ({})
    var minute = Math.round(Number(clock.local_minutes))
    if (!isFinite(minute) || minute < 0 || minute >= 24 * 60) continue
    var dayOffset = relativeDayOffset(clock, sourceClock)
    var relativeMinute = dayOffset * DAY_MINUTES + minute - sourceMinute
    var position = (relativeMinute + DAY_MINUTES / 2) / DAY_MINUTES
    var overflow = position < 0 ? "previous" : (position > 1 ? "next" : "")
    var key = overflow || String(relativeMinute)
    var group = groups[key]
    if (!group) {
      group = {
        minute: minute,
        minutes: [],
        relative_minutes: [],
        position: position,
        overflow: overflow,
        time: String(clock.time || ""),
        labels: [],
        notations: [],
        source: false,
        count: 0,
        lane: 0
      }
      groups[key] = group
    }
    var label = markerLabel(clock, sourceClock)
    if (label && group.labels.indexOf(label) === -1) group.labels.push(label)
    var notation = String(clock.notation || "").toUpperCase()
    if (notation && group.notations.indexOf(notation) === -1)
      group.notations.push(notation)
    if (group.minutes.indexOf(minute) === -1) group.minutes.push(minute)
    if (group.relative_minutes.indexOf(relativeMinute) === -1)
      group.relative_minutes.push(relativeMinute)
    if (String(clock.timezone || "") === String(sourceTimezone || "")) group.source = true
    group.count += 1
    if (overflow === "previous") group.position = Math.min(group.position, position)
    else if (overflow === "next") group.position = Math.max(group.position, position)
  }

  var markers = []
  for (var minuteKey in groups) {
    if (!Object.prototype.hasOwnProperty.call(groups, minuteKey)) continue
    var marker = groups[minuteKey]
    marker.labels.sort()
    marker.notations.sort()
    marker.minutes.sort(function(left, right) { return left - right })
    marker.relative_minutes.sort(function(left, right) { return left - right })
    marker.label = marker.overflow
      ? overflowMarkerLabel(marker) : marker.labels.join(" / ")
    delete marker.labels
    delete marker.notations
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
