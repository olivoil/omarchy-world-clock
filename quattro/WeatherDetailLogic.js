function finite(value) {
  if (value === null || value === undefined || value === "") return NaN
  return Number(value)
}

function listLength(values) {
  if (!values || values.length === undefined) return 0
  var length = Math.floor(Number(values.length))
  return isFinite(length) && length > 0 ? length : 0
}

function sunProgress(sunriseMinutes, sunsetMinutes, currentMinutes) {
  var sunrise = finite(sunriseMinutes)
  var sunset = finite(sunsetMinutes)
  var current = finite(currentMinutes)
  if (!isFinite(sunrise) || !isFinite(sunset) || !isFinite(current)
      || sunset <= sunrise) return NaN
  return Math.max(0, Math.min(1, (current - sunrise) / (sunset - sunrise)))
}

function weatherUpdateLabel(updatedAt, currentTime, unavailable, loading) {
  if (unavailable === true) return "UPDATE UNAVAILABLE"
  var updated = finite(updatedAt)
  var current = finite(currentTime)
  if (!isFinite(updated) || updated <= 0 || !isFinite(current))
    return loading === true ? "UPDATING" : "AWAITING UPDATE"

  var minutes = Math.floor(Math.max(0, current - updated) / 60000)
  if (minutes < 1) return "UPDATED NOW"
  if (minutes < 60)
    return "UPDATED " + String(minutes) + " MIN AGO"
  var hours = Math.floor(minutes / 60)
  if (hours < 24)
    return "UPDATED " + String(hours) + " HR AGO"
  var days = Math.floor(hours / 24)
  return "UPDATED " + String(days) + (days === 1 ? " DAY AGO" : " DAYS AGO")
}

function weatherFamily(item) {
  var code = Number(item ? item.weather_code : -1)
  if (code === 95 || code === 96 || code === 99) return "storm"
  if (code === 71 || code === 73 || code === 75 || code === 77
      || code === 85 || code === 86) return "snow"
  if (code === 56 || code === 57 || code === 66 || code === 67)
    return "ice"
  if ((code >= 51 && code <= 55) || (code >= 61 && code <= 65)
      || (code >= 80 && code <= 82)) return "rain"
  if (code === 45 || code === 48) return "fog"
  if (code >= 0 && code <= 1) return "clear"
  return "cloud"
}

function wetFamily(family) {
  return family === "rain" || family === "storm" || family === "snow"
    || family === "ice"
}

function clearSkiesHold(source, limit) {
  var count = Math.min(listLength(source),
    Math.max(0, Math.floor(Number(limit) || 4)))
  // "The next few hours" is a forecast claim, so require the current slot
  // plus at least two future observations instead of extrapolating from now.
  if (count < 3) return false
  for (var index = 0; index < count; index++) {
    var item = source[index]
    var code = finite(item ? item.weather_code : null)
    var probability = finite(item
      ? item.precipitation_probability_percent : null)
    var amount = finite(item ? item.precipitation_mm : null)
    if (!isFinite(code) || weatherFamily(item) !== "clear"
        || (isFinite(probability) && probability >= 20)
        || (isFinite(amount) && amount >= 0.1)) return false
  }
  return true
}

function comfortNote(apparentTemperature, relativeHumidity) {
  var feels = finite(apparentTemperature)
  var humidity = finite(relativeHumidity)
  if (isFinite(feels) && feels >= 35) {
    return isFinite(humidity) && humidity >= 60
      ? "It feels hot and humid right now." : "It feels hot right now."
  }
  if (isFinite(feels) && feels >= 28) {
    return isFinite(humidity) && humidity >= 60
      ? "It feels warm and humid right now." : "It feels warm right now."
  }
  if (isFinite(feels) && feels <= 0) return "It feels freezing right now."
  if (isFinite(feels) && feels <= 10) return "It feels cool right now."
  if (isFinite(humidity) && humidity >= 75)
    return "Humidity is high right now."
  if (isFinite(humidity) && humidity >= 55)
    return "Humidity is elevated right now."
  if (isFinite(humidity) && humidity < 30) return "The air is dry right now."
  if (isFinite(feels) && isFinite(humidity))
    return "Temperature and humidity are moderate right now."
  if (isFinite(humidity)) return "Humidity is moderate right now."
  if (isFinite(feels)) return "The apparent temperature is moderate right now."
  return ""
}

function humidityDescription(relativeHumidity, temperature) {
  var humidity = finite(relativeHumidity)
  var currentTemperature = finite(temperature)
  if (!isFinite(humidity)) return "Unavailable"
  if (humidity >= 75) return "Very humid"
  if (humidity >= 55) {
    return isFinite(currentTemperature) && currentTemperature >= 22
      ? "Warm and humid" : "Elevated humidity"
  }
  return humidity >= 30 ? "Moderate humidity" : "Dry"
}

function familyPriority(family) {
  if (family === "storm") return 5
  if (family === "ice" || family === "snow") return 4
  if (family === "rain") return 3
  if (family === "fog") return 2
  if (family === "cloud") return 1
  return 0
}

function phaseRepresentative(items) {
  var length = listLength(items)
  if (!length) return null
  var counts = ({})
  for (var index = 0; index < length; index++) {
    var family = weatherFamily(items[index])
    counts[family] = Number(counts[family] || 0) + 1
  }
  var dominant = weatherFamily(items[Math.floor(length / 2)])
  for (var candidate in counts) {
    if (!Object.prototype.hasOwnProperty.call(counts, candidate)) continue
    if (counts[candidate] > counts[dominant]
        || (counts[candidate] === counts[dominant]
          && familyPriority(candidate) > familyPriority(dominant)))
      dominant = candidate
  }

  var representative = null
  var bestScore = Number.NEGATIVE_INFINITY
  var midpoint = (length - 1) / 2
  for (var itemIndex = 0; itemIndex < length; itemIndex++) {
    var item = items[itemIndex]
    if (weatherFamily(item) !== dominant) continue
    var probability = finite(item
      ? item.precipitation_probability_percent : null)
    var code = finite(item ? item.weather_code : null)
    var score = (isFinite(probability) ? probability : 0)
      + (isFinite(code) ? code * 0.1 : 0)
      - Math.abs(itemIndex - midpoint) * 0.01
    if (score > bestScore) {
      bestScore = score
      representative = item
    }
  }
  return representative || items[Math.floor(length / 2)]
}

function probabilityBand(item) {
  var probability = finite(item
    ? item.precipitation_probability_percent : null)
  if (!isFinite(probability)) return wetFamily(weatherFamily(item)) ? 1 : 0
  if (probability >= 60) return 2
  if (probability >= 30) return 1
  return 0
}

function persistentTransition(values, index, accessor) {
  if (index < 2 || index + 1 >= values.length) return false
  var before = accessor(values[index - 1])
  var after = accessor(values[index])
  return before !== after
    && accessor(values[index - 2]) === before
    && accessor(values[index + 1]) === after
}

function familyTransitionScore(before, after) {
  if (before === after) return 0
  if (wetFamily(before) !== wetFamily(after)) return 100
  if (before === "storm" || after === "storm") return 86
  if (wetFamily(before) && wetFamily(after)) return 76
  if (before === "fog" || after === "fog") return 64
  return 56
}

function bandTransitionScore(before, after) {
  if (before === after) return 0
  if (Math.abs(before - after) === 2) return 90
  return before === 0 || after === 0 ? 66 : 52
}

function boundaryScore(values, index) {
  var score = 0
  if (persistentTransition(values, index, weatherFamily)) {
    score = Math.max(score, familyTransitionScore(
      weatherFamily(values[index - 1]), weatherFamily(values[index])))
  }
  if (persistentTransition(values, index, probabilityBand)) {
    score = Math.max(score, bandTransitionScore(
      probabilityBand(values[index - 1]), probabilityBand(values[index])))
  }
  var beforeProbability = finite(values[index - 1]
    ? values[index - 1].precipitation_probability_percent : null)
  var afterProbability = finite(values[index]
    ? values[index].precipitation_probability_percent : null)
  if (score > 0 && isFinite(beforeProbability) && isFinite(afterProbability)
      && Math.abs(afterProbability - beforeProbability) >= 25)
    score += Math.min(24, Math.abs(afterProbability - beforeProbability) * 0.4)
  return score
}

function segmentMinimumLength(valuesLength) {
  return valuesLength >= 6 ? 2 : 1
}

function segmentationBalance(boundaries, valuesLength) {
  var previous = 0
  var minimum = valuesLength
  for (var index = 0; index <= boundaries.length; index++) {
    var end = index < boundaries.length ? boundaries[index] : valuesLength
    minimum = Math.min(minimum, end - previous)
    previous = end
  }
  return minimum * 8
}

function segmentPhases(source, limit, maximumPhases) {
  var sourceLength = listLength(source)
  var count = Math.min(sourceLength, Math.max(0, Math.floor(Number(limit) || 12)))
  var values = []
  for (var sourceIndex = 0; sourceIndex < count; sourceIndex++)
    values.push(source[sourceIndex])
  if (!values.length) return []

  var phaseLimit = Math.max(1, Math.min(3,
    Math.floor(Number(maximumPhases) || 3)))
  var minimumLength = segmentMinimumLength(values.length)
  var candidates = []
  for (var boundary = minimumLength;
      boundary <= values.length - minimumLength; boundary++) {
    var score = boundaryScore(values, boundary)
    if (score >= 50) candidates.push({ boundary: boundary, score: score })
  }

  var bestBoundaries = []
  var bestScore = 0
  function consider(boundaries, score) {
    var sorted = boundaries.slice().sort(function(left, right) {
      return left - right
    })
    var previous = 0
    for (var index = 0; index <= sorted.length; index++) {
      var end = index < sorted.length ? sorted[index] : values.length
      if (end - previous < minimumLength) return
      previous = end
    }
    var balancedScore = score + segmentationBalance(sorted, values.length)
    if (balancedScore > bestScore) {
      bestScore = balancedScore
      bestBoundaries = sorted
    }
  }

  if (phaseLimit >= 2) {
    for (var first = 0; first < candidates.length; first++)
      consider([candidates[first].boundary], candidates[first].score)
  }
  if (phaseLimit >= 3) {
    for (var left = 0; left < candidates.length; left++) {
      for (var right = left + 1; right < candidates.length; right++) {
        consider([candidates[left].boundary, candidates[right].boundary],
          candidates[left].score + candidates[right].score)
      }
    }
  }

  var groups = []
  var start = 0
  for (var split = 0; split <= bestBoundaries.length; split++) {
    var end = split < bestBoundaries.length
      ? bestBoundaries[split] : values.length
    groups.push(values.slice(start, end))
    start = end
  }
  return groups
}

function maximumField(source, field, limit) {
  var count = Math.min(listLength(source), limit || listLength(source))
  var maximum = Number.NEGATIVE_INFINITY
  for (var index = 0; index < count; index++) {
    var value = finite(source[index] ? source[index][field] : null)
    if (isFinite(value)) maximum = Math.max(maximum, value)
  }
  return maximum
}

function minimumField(source, field, limit) {
  var count = Math.min(listLength(source), limit || listLength(source))
  var minimum = Number.POSITIVE_INFINITY
  for (var index = 0; index < count; index++) {
    var value = finite(source[index] ? source[index][field] : null)
    if (isFinite(value)) minimum = Math.min(minimum, value)
  }
  return minimum
}

function temperatureScore(source) {
  var high = maximumField(source, "temperature_celsius")
  var low = minimumField(source, "temperature_celsius")
  var score = 20
  if (isFinite(high)) {
    if (high >= 38) score = Math.max(score, 94)
    else if (high >= 35) score = Math.max(score, 82)
    else if (high >= 32) score = Math.max(score, 68)
  }
  if (isFinite(low)) {
    if (low <= -15) score = Math.max(score, 94)
    else if (low <= -5) score = Math.max(score, 80)
    else if (low <= 0) score = Math.max(score, 64)
  }
  if (isFinite(high) && isFinite(low) && high - low >= 10)
    score = Math.max(score, 58)
  return score
}

function precipitationScore(source) {
  var probability = maximumField(source,
    "precipitation_probability_percent", 12)
  var amount = maximumField(source, "precipitation_mm", 12)
  var score = Number.NEGATIVE_INFINITY
  if (isFinite(probability)) {
    if (probability >= 80) score = Math.max(score, 100)
    else if (probability >= 60) score = Math.max(score, 88)
    else if (probability >= 35) score = Math.max(score, 72)
    else if (probability >= 20) score = Math.max(score, 45)
  }
  if (isFinite(amount)) {
    if (amount >= 5) score = Math.max(score, 100)
    else if (amount >= 2) score = Math.max(score, 86)
    else if (amount >= 0.5) score = Math.max(score, 66)
    else if (amount >= 0.1) score = Math.max(score, 45)
  }
  return score
}

function uvScore(source) {
  var ultraviolet = maximumField(source, "uv_index")
  if (!isFinite(ultraviolet)) return Number.NEGATIVE_INFINITY
  if (ultraviolet >= 11) return 98
  if (ultraviolet >= 8) return 84
  if (ultraviolet >= 6) return 70
  if (ultraviolet >= 3) return 48
  return 18
}

function windScore(source) {
  var speed = maximumField(source, "wind_speed_kmh")
  var gust = maximumField(source, "wind_gusts_kmh")
  var wind = Math.max(speed, gust)
  if (!isFinite(wind)) return Number.NEGATIVE_INFINITY
  if (wind >= 85) return 100
  if (wind >= 65) return 92
  if (wind >= 45) return 76
  if (wind >= 30) return 58
  return 22
}

function defaultMetric(source) {
  var scores = [
    { key: "temperature", score: temperatureScore(source) },
    { key: "precipitation", score: precipitationScore(source) },
    { key: "uv", score: uvScore(source) },
    { key: "wind", score: windScore(source) }
  ]
  var best = scores[0]
  for (var index = 1; index < scores.length; index++) {
    if (scores[index].score > best.score) best = scores[index]
  }
  return best.key
}

function wheelDistance(pixelY, angleY, viewportHeight, baseStep) {
  var pixels = finite(pixelY)
  if (isFinite(pixels) && pixels !== 0) return pixels * 1.5
  var angle = finite(angleY)
  if (!isFinite(angle) || angle === 0) return 0
  var viewport = Math.max(0, finite(viewportHeight))
  var minimumStep = Math.max(0, finite(baseStep))
  var step = Math.max(minimumStep, viewport * 0.46)
  return angle / 120 * step
}

function nextScrollTarget(contentY, pendingTargetY, hasPending,
    distance, maximum) {
  var current = finite(contentY)
  var pending = finite(pendingTargetY)
  var origin = hasPending && isFinite(pending) ? pending : current
  var limit = Math.max(0, finite(maximum))
  var next = origin - finite(distance)
  return Math.max(0, Math.min(limit, next))
}
