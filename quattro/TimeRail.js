var DAY_MINUTES = 24 * 60

function wrapMinute(value) {
  return ((Number(value || 0) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES
}

function localDayPosition(localMinutes) {
  var minute = Number(localMinutes)
  if (!isFinite(minute)) return 0
  return Math.max(0, Math.min(DAY_MINUTES, minute)) / DAY_MINUTES
}

function localDaylightTrack(kind, sunriseMinutes, solarNoonMinutes,
    sunsetMinutes) {
  return {
    kind: kind,
    sunrise_minutes: sunriseMinutes,
    solar_noon_minutes: solarNoonMinutes,
    sunset_minutes: sunsetMinutes,
    daylight_intervals: [],
    curve_positions: [],
    curve_heights: [],
    curve_segments: [],
    peak_elevation_degrees: null,
    current_elevation_degrees: null,
    marker_light: 0.5
  }
}

function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(maximum, Number(value)))
}

function solarElevationDegrees(latitudeRadians, declination, solarNoon,
    localMinute) {
  var radians = Math.PI / 180
  var hourAngle = (Number(localMinute) - Number(solarNoon)) / 4 * radians
  var sineElevation = Math.sin(latitudeRadians) * Math.sin(declination)
    + Math.cos(latitudeRadians) * Math.cos(declination) * Math.cos(hourAngle)
  return Math.asin(clamp(sineElevation, -1, 1)) / radians
}

function solarCurveHeight(elevationDegrees) {
  // Apparent sunrise occurs with the solar center 0.833 degrees below the
  // geometric horizon. Square-root compression keeps low winter arcs visible
  // in a few pixels without letting a high tropical sun dominate the card.
  var apparentElevation = Number(elevationDegrees) + 0.833
  var daylight = Math.sin(Math.max(0, apparentElevation) * Math.PI / 180)
  return Math.sqrt(clamp(daylight, 0, 1))
}

function solarMarkerLight(elevationDegrees) {
  if (!isFinite(Number(elevationDegrees))) return 0.5
  var progress = clamp((Number(elevationDegrees) + 6) / 12, 0, 1)
  return progress * progress * (3 - 2 * progress)
}

function fallbackLocalDaylight(kind, solarNoonMinutes) {
  return localDaylightTrack(kind === "fallback" ? "fallback" : kind,
    null, solarNoonMinutes, null)
}

function utcDateMilliseconds(year, monthIndex, day) {
  // Date.UTC treats years 0–99 as 1900–1999. setUTCFullYear preserves the
  // proleptic four-digit year emitted by the backend instead.
  var date = new Date(0)
  date.setUTCFullYear(year, monthIndex, day)
  date.setUTCHours(0, 0, 0, 0)
  return date.getTime()
}

function parsedLocalDate(value) {
  var match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(value || ""))
  if (!match) return null
  var year = Number(match[1])
  var month = Number(match[2])
  var day = Number(match[3])
  if (month < 1 || month > 12 || day < 1 || day > 31) return null
  var milliseconds = utcDateMilliseconds(year, month - 1, day)
  var date = new Date(milliseconds)
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1
      || date.getUTCDate() !== day) return null
  var yearStart = utcDateMilliseconds(year, 0, 1)
  return {
    year: year,
    month: month,
    day: day,
    milliseconds: milliseconds,
    day_of_year: Math.floor((milliseconds - yearStart) / 86400000) + 1,
    days_in_year: utcDateMilliseconds(year + 1, 0, 1) - yearStart
      === 366 * 86400000
      ? 366 : 365
  }
}

function validUtcOffsetMinutes(value) {
  var seconds = Number(value)
  // Historical IANA local-mean-time offsets can exceed the modern civil
  // range. The backend's fixed offsets support every value within one day.
  if (!isFinite(seconds) || Math.abs(seconds) >= DAY_MINUTES * 60) return null
  return seconds / 60
}

function clockOffsetStateMinutes(clock, localMinute) {
  var states = clock && clock.utc_offset_states
  if (!Array.isArray(states) || states.length === 0) return null
  var minute = wrapMinute(localMinute)
  var selected = null
  for (var index = 0; index < states.length; index++) {
    var fromMinute = Number(states[index] && states[index].from_minute)
    var offset = validUtcOffsetMinutes(
      states[index] && states[index].utc_offset_seconds)
    if (!isFinite(fromMinute) || offset === null) continue
    if (fromMinute > minute) break
    selected = offset
  }
  if (selected !== null) return selected
  return validUtcOffsetMinutes(states[0] && states[0].utc_offset_seconds)
}

function clockUtcOffsetMinutes(clock, referenceUtc, date, localMinute) {
  var stateOffset = clockOffsetStateMinutes(clock, localMinute)
  if (stateOffset !== null) return stateOffset
  var offsetValue = clock && clock.utc_offset_seconds
  var directOffset = validUtcOffsetMinutes(offsetValue)
  if (offsetValue !== null && offsetValue !== undefined && directOffset !== null)
    return directOffset

  var localMinutes = Number(clock && clock.local_minutes)
  var referenceMilliseconds = Date.parse(String(referenceUtc || ""))
  if (!isFinite(localMinutes) || localMinutes < 0 || localMinutes >= DAY_MINUTES
      || !isFinite(referenceMilliseconds)) return null
  var localMilliseconds = date.milliseconds + Math.round(localMinutes) * 60000
  var offset = Math.floor(localMilliseconds / 60000)
    - Math.floor(referenceMilliseconds / 60000)
  return Math.abs(offset) < DAY_MINUTES ? offset : null
}

function solarEventLocalMinute(clock, referenceUtc, date, utcMinute,
    fallbackOffsetMinutes) {
  var candidate = wrapMinute(Number(utcMinute) + Number(fallbackOffsetMinutes))
  for (var iteration = 0; iteration < 3; iteration++) {
    var offset = clockUtcOffsetMinutes(clock, referenceUtc, date, candidate)
    if (offset === null) offset = fallbackOffsetMinutes
    var next = wrapMinute(Number(utcMinute) + Number(offset))
    if (Math.abs(next - candidate) < 0.001) return next
    candidate = next
  }
  return candidate
}

function solarElevationAtLocalMinute(clock, referenceUtc, date,
    latitudeRadians, declination, solarNoonUtc, localMinute,
    fallbackOffsetMinutes) {
  var offset = clockUtcOffsetMinutes(clock, referenceUtc, date, localMinute)
  if (offset === null) offset = fallbackOffsetMinutes
  return solarElevationDegrees(latitudeRadians, declination, solarNoonUtc,
    Number(localMinute) - Number(offset))
}

function daylightIntervals(sunriseMinutes, sunsetMinutes) {
  var sunrise = wrapMinute(sunriseMinutes)
  var sunset = wrapMinute(sunsetMinutes)
  if (sunrise < sunset) return [[sunrise, sunset]]
  if (sunrise > sunset) return [[0, sunset], [sunrise, DAY_MINUTES]]
  return []
}

function addSolarProfile(track, clock, referenceUtc, date, latitudeRadians,
    declination, solarNoonUtc, fallbackOffsetMinutes, curveIntervals) {
  var positions = []
  var heights = []
  var segments = []
  for (var intervalIndex = 0; intervalIndex < curveIntervals.length;
      intervalIndex++) {
    var start = Number(curveIntervals[intervalIndex][0])
    var end = Number(curveIntervals[intervalIndex][1])
    if (!isFinite(start) || !isFinite(end) || !(start < end)) continue
    var sampleCount = curveIntervals.length === 1 ? 33
      : Math.max(2, Math.round((end - start) / DAY_MINUTES * 32) + 1)
    var segmentPositions = []
    var segmentHeights = []
    for (var sampleIndex = 0; sampleIndex < sampleCount; sampleIndex++) {
      var progress = sampleIndex / (sampleCount - 1)
      var minute = start + (end - start) * progress
      var elevation = solarElevationAtLocalMinute(clock, referenceUtc, date,
        latitudeRadians, declination, solarNoonUtc, minute,
        fallbackOffsetMinutes)
      var position = minute / DAY_MINUTES
      var height = solarCurveHeight(elevation)
      segmentPositions.push(position)
      segmentHeights.push(height)
      positions.push(position)
      heights.push(height)
    }
    segments.push({ positions: segmentPositions, heights: segmentHeights })
  }
  track.curve_positions = positions
  track.curve_heights = heights
  track.curve_segments = segments
  track.peak_elevation_degrees = solarElevationDegrees(
    latitudeRadians, declination, solarNoonUtc, solarNoonUtc)

  var currentMinute = Number(clock && clock.local_minutes)
  if (isFinite(currentMinute)) {
    var currentElevation = solarElevationAtLocalMinute(clock, referenceUtc, date,
      latitudeRadians, declination, solarNoonUtc, wrapMinute(currentMinute),
      fallbackOffsetMinutes)
    track.current_elevation_degrees = currentElevation
    track.marker_light = solarMarkerLight(currentElevation)
  }
  return track
}

function localDaylight(clock, referenceUtc) {
  var date = parsedLocalDate(clock && clock.date)
  var latitudeValue = clock && clock.latitude
  var longitudeValue = clock && clock.longitude
  var latitude = Number(latitudeValue)
  var longitude = Number(longitudeValue)
  if (!date || latitudeValue === null || latitudeValue === undefined
      || longitudeValue === null || longitudeValue === undefined
      || !isFinite(latitude) || !isFinite(longitude)
      || latitude < -90 || latitude > 90
      || longitude < -180 || longitude > 180) return fallbackLocalDaylight("fallback")

  var utcOffsetMinutes = clockUtcOffsetMinutes(
    clock, referenceUtc, date, clock && clock.local_minutes)
  if (utcOffsetMinutes === null) return fallbackLocalDaylight("fallback")

  // NOAA's compact solar equations use 90.833 degrees for apparent sunrise
  // and sunset, accounting for atmospheric refraction and the solar disc.
  var radians = Math.PI / 180
  var gamma = 2 * Math.PI / date.days_in_year * (date.day_of_year - 1)
  var equationOfTime = 229.18 * (0.000075
    + 0.001868 * Math.cos(gamma) - 0.032077 * Math.sin(gamma)
    - 0.014615 * Math.cos(2 * gamma) - 0.040849 * Math.sin(2 * gamma))
  var declination = 0.006918 - 0.399912 * Math.cos(gamma)
    + 0.070257 * Math.sin(gamma) - 0.006758 * Math.cos(2 * gamma)
    + 0.000907 * Math.sin(2 * gamma) - 0.002697 * Math.cos(3 * gamma)
    + 0.00148 * Math.sin(3 * gamma)
  var latitudeRadians = latitude * radians
  var hourAngleCosine = Math.cos(90.833 * radians)
      / (Math.cos(latitudeRadians) * Math.cos(declination))
    - Math.tan(latitudeRadians) * Math.tan(declination)
  var solarNoonUtc = 720 - 4 * longitude - equationOfTime
  if (!isFinite(solarNoonUtc))
    return fallbackLocalDaylight("fallback")
  var solarNoon = solarEventLocalMinute(
    clock, referenceUtc, date, solarNoonUtc, utcOffsetMinutes)

  if (hourAngleCosine > 1) {
    var polarNight = fallbackLocalDaylight("polar-night", solarNoon)
    return addSolarProfile(polarNight, clock, referenceUtc, date,
      latitudeRadians, declination, solarNoonUtc, utcOffsetMinutes,
      [[0, DAY_MINUTES]])
  }
  if (hourAngleCosine < -1) {
    var polarDay = fallbackLocalDaylight("polar-day", solarNoon)
    polarDay.daylight_intervals = [{ start: 0, end: 1 }]
    return addSolarProfile(polarDay, clock, referenceUtc, date,
      latitudeRadians, declination, solarNoonUtc, utcOffsetMinutes,
      [[0, DAY_MINUTES]])
  }

  var hourAngleDegrees = Math.acos(hourAngleCosine) / radians
  var sunrise = solarEventLocalMinute(clock, referenceUtc, date,
    solarNoonUtc - 4 * hourAngleDegrees, utcOffsetMinutes)
  var sunset = solarEventLocalMinute(clock, referenceUtc, date,
    solarNoonUtc + 4 * hourAngleDegrees, utcOffsetMinutes)
  if (!isFinite(sunrise) || !isFinite(sunset) || sunrise === sunset)
    return fallbackLocalDaylight("fallback")

  var track = localDaylightTrack("solar", sunrise, solarNoon, sunset)
  var intervals = daylightIntervals(sunrise, sunset)
  track.daylight_intervals = intervals.map(function(interval) {
    return { start: interval[0] / DAY_MINUTES, end: interval[1] / DAY_MINUTES }
  })
  return addSolarProfile(track, clock, referenceUtc, date, latitudeRadians,
    declination, solarNoonUtc, utcOffsetMinutes, intervals)
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
  for (var index = 0; index < snapshotLocations.length; index++) {
    if (locationIdentity(payload.locations[index]) !== locationIdentity(snapshotLocations[index]))
      return false
    if (!Array.isArray(payload.locations[index].states)
        || payload.locations[index].states.length === 0) return false
  }
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

function paddedNumber(value, width) {
  var result = String(Math.max(0, Math.round(Number(value || 0))))
  while (result.length < width) result = "0" + result
  return result
}

function shiftedIsoDate(date, dayOffset) {
  var value = Date.parse(String(date || "") + "T00:00:00Z")
  if (!isFinite(value)) return ""
  var shifted = new Date(value + Number(dayOffset || 0) * 24 * 60 * 60 * 1000)
  return paddedNumber(shifted.getUTCFullYear(), 4) + "-"
    + paddedNumber(shifted.getUTCMonth() + 1, 2) + "-"
    + paddedNumber(shifted.getUTCDate(), 2)
}

function formattedClockTime(hour, minute, timeFormat) {
  if (String(timeFormat || "").toLowerCase() === "ampm") {
    var twelveHour = hour % 12 || 12
    return String(twelveHour) + ":" + paddedNumber(minute, 2)
      + (hour < 12 ? " AM" : " PM")
  }
  return paddedNumber(hour, 2) + ":" + paddedNumber(minute, 2)
}

function formattedCalendarDay(date) {
  var value = Date.parse(String(date || "") + "T00:00:00Z")
  if (!isFinite(value)) return ""
  var rendered = new Date(value)
  var weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
  var months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
  return weekdays[rendered.getUTCDay()] + ", " + months[rendered.getUTCMonth()]
    + " " + String(rendered.getUTCDate())
}

function relativeClockLabel(relativeMinutes) {
  var value = Math.round(Number(relativeMinutes || 0))
  if (value === 0) return "Same time"
  var absolute = Math.abs(value)
  var hours = Math.floor(absolute / 60)
  var minutes = absolute % 60
  var direction = value > 0 ? "ahead" : "behind"
  if (hours === 0) return String(minutes) + " min " + direction
  if (minutes === 0) return String(hours) + "h " + direction
  return String(hours) + "h " + paddedNumber(minutes, 2) + "m " + direction
}

function timezoneStateAt(location, slotIndex) {
  if (!location || !Array.isArray(location.states) || location.states.length === 0)
    return null
  var selected = location.states[0]
  for (var index = 1; index < location.states.length; index++) {
    if (Number(location.states[index].from_slot) > Number(slotIndex)) break
    selected = location.states[index]
  }
  var offset = Number(selected.utc_offset_seconds)
  if (!isFinite(offset)) return null
  return {
    utc_offset_seconds: Math.round(offset),
    notation: String(selected.notation || "")
  }
}

function scrubOffsetStatesForDate(payload, location, date) {
  if (!payload || !Array.isArray(payload.slots) || !location || !date) return []
  var offsetsByMinute = ({})
  for (var slotIndex = 0; slotIndex < payload.slots.length; slotIndex++) {
    var frame = payload.slots[slotIndex]
    if (!frame || !frame.reference_utc) continue
    var referenceMilliseconds = Date.parse(String(frame.reference_utc))
    var state = timezoneStateAt(location, slotIndex)
    if (!isFinite(referenceMilliseconds) || !state) continue
    var local = new Date(referenceMilliseconds + state.utc_offset_seconds * 1000)
    if (!isFinite(local.getTime())) continue
    var localDate = paddedNumber(local.getUTCFullYear(), 4) + "-"
      + paddedNumber(local.getUTCMonth() + 1, 2) + "-"
      + paddedNumber(local.getUTCDate(), 2)
    if (localDate !== String(date)) continue
    var localMinute = local.getUTCHours() * 60 + local.getUTCMinutes()
    var key = String(localMinute)
    if (offsetsByMinute[key] === undefined)
      offsetsByMinute[key] = state.utc_offset_seconds
  }

  var minutes = Object.keys(offsetsByMinute)
    .map(function(value) { return Number(value) })
    .sort(function(left, right) { return left - right })
  var result = []
  var previousOffset = null
  for (var minuteIndex = 0; minuteIndex < minutes.length; minuteIndex++) {
    var minute = minutes[minuteIndex]
    var offset = Number(offsetsByMinute[String(minute)])
    if (!isFinite(offset) || offset === previousOffset) continue
    result.push({ from_minute: minute, utc_offset_seconds: Math.round(offset) })
    previousOffset = offset
  }
  if (result.length > 0) result[0].from_minute = 0
  return result
}

function renderedScrubClock(baseClock, state, referenceMilliseconds,
    timeFormat, sourceDate, summaryDate, summaryOffsetSeconds) {
  var local = new Date(referenceMilliseconds + state.utc_offset_seconds * 1000)
  if (!isFinite(local.getTime())) return null
  var hour = local.getUTCHours()
  var minute = local.getUTCMinutes()
  var date = paddedNumber(local.getUTCFullYear(), 4) + "-"
    + paddedNumber(local.getUTCMonth() + 1, 2) + "-"
    + paddedNumber(local.getUTCDate(), 2)
  var summaryDayOffset = dateDayOffset(date, summaryDate)
  var day = summaryDayOffset === -1 ? "Yesterday"
    : (summaryDayOffset === 0 ? "Today"
      : (summaryDayOffset === 1 ? "Tomorrow" : formattedCalendarDay(date)))
  var relativeSeconds = state.utc_offset_seconds - summaryOffsetSeconds
  var relativeMinutes = relativeSeconds < 0
    ? Math.ceil(relativeSeconds / 60) : Math.floor(relativeSeconds / 60)
  return mergeRecord(baseClock, {
    time: formattedClockTime(hour, minute, timeFormat),
    date: date,
    day: day,
    notation: state.notation,
    local_minutes: hour * 60 + minute,
    utc_offset_seconds: state.utc_offset_seconds,
    source_day_offset: dateDayOffset(date, sourceDate),
    relative_minutes: relativeMinutes,
    relative_label: relativeClockLabel(relativeMinutes)
  })
}

function mergeSnapshot(base, payload, slotIndex) {
  if (!base || !base.summary || !Array.isArray(base.clocks)
      || !payload || !Array.isArray(payload.slots)
      || !Array.isArray(payload.locations)
      || payload.locations.length !== base.clocks.length + 1) return null
  var index = Math.max(0, Math.min(payload.slots.length - 1,
    Math.round(Number(slotIndex || 0))))
  var frame = payload.slots[index]
  if (!frame || !frame.reference_utc) return null
  var referenceMilliseconds = Date.parse(String(frame.reference_utc))
  if (!isFinite(referenceMilliseconds)) return null

  var states = []
  for (var locationIndex = 0; locationIndex < payload.locations.length; locationIndex++) {
    var state = timezoneStateAt(payload.locations[locationIndex], index)
    if (!state) return null
    states.push(state)
  }
  var sourceDate = shiftedIsoDate(payload.date, frame.day_offset)
  var summaryLocal = new Date(referenceMilliseconds
    + states[0].utc_offset_seconds * 1000)
  var summaryDate = paddedNumber(summaryLocal.getUTCFullYear(), 4) + "-"
    + paddedNumber(summaryLocal.getUTCMonth() + 1, 2) + "-"
    + paddedNumber(summaryLocal.getUTCDate(), 2)
  if (!sourceDate || !isFinite(summaryLocal.getTime())) return null

  var result = mergeRecord(base, ({ reference_utc: frame.reference_utc }))
  result.scrub_day_offset = Number(frame.day_offset || 0)
  result.summary = renderedScrubClock(base.summary, states[0], referenceMilliseconds,
    payload.time_format, sourceDate, summaryDate, states[0].utc_offset_seconds)
  if (!result.summary) return null
  var summaryOffsetStates = scrubOffsetStatesForDate(
    payload, payload.locations[0], result.summary.date)
  result.summary.utc_offset_states = summaryOffsetStates.length > 0
    ? summaryOffsetStates
    : [{ from_minute: 0, utc_offset_seconds: states[0].utc_offset_seconds }]
  result.clocks = []
  for (var clockIndex = 0; clockIndex < base.clocks.length; clockIndex++) {
    var rendered = renderedScrubClock(base.clocks[clockIndex], states[clockIndex + 1],
      referenceMilliseconds, payload.time_format, sourceDate, summaryDate,
      states[0].utc_offset_seconds)
    if (!rendered) return null
    var offsetStates = scrubOffsetStatesForDate(
      payload, payload.locations[clockIndex + 1], rendered.date)
    rendered.utc_offset_states = offsetStates.length > 0
      ? offsetStates
      : [{ from_minute: 0,
          utc_offset_seconds: states[clockIndex + 1].utc_offset_seconds }]
    result.clocks.push(rendered)
  }
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

function selectionLabelVisible(interacting, label) {
  return interacting === true && String(label || "").length > 0
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

function relativeMinuteForClock(clock, sourceClock, anchorMinute) {
  var minute = Math.round(Number(clock && clock.local_minutes))
  if (!isFinite(minute) || minute < 0 || minute >= DAY_MINUTES) return NaN
  var sourceMinute = Number(anchorMinute)
  if (!isFinite(sourceMinute))
    sourceMinute = Number(sourceClock && sourceClock.local_minutes || 0)
  return relativeDayOffset(clock, sourceClock) * DAY_MINUTES + minute - sourceMinute
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
    var relativeMinute = relativeMinuteForClock(clock, sourceClock, sourceMinute)
    if (!isFinite(relativeMinute)) continue
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
        identities: [],
        labels: [],
        notations: [],
        source: false,
        count: 0,
        lane: 0
      }
      groups[key] = group
    }
    var label = markerLabel(clock, sourceClock)
    var identity = locationIdentity(clock)
    if (identity && group.identities.indexOf(identity) === -1)
      group.identities.push(identity)
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
    marker.identities.sort()
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
