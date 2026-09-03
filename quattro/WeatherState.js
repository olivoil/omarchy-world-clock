function locationKey(item) {
  var id = Number(item ? item.id : 0)
  if (isFinite(id) && id > 0 && Math.floor(id) === id)
    return "id:" + String(id)
  if (!item) return ""
  var label = item.label !== null && item.label !== undefined
    ? item.label : item.title
  var timezone = String(item.timezone || "")
  return timezone ? timezone + "\u001f" + String(label || "") : ""
}

function mergePayload(previousPayload, nextPayload) {
  if (!nextPayload || nextPayload.partial !== true) return nextPayload

  var nextLocations = Array.isArray(nextPayload.locations)
    ? nextPayload.locations.slice() : []
  var failedKeys = ({})
  var failedLocations = Array.isArray(nextPayload.failed_locations)
    ? nextPayload.failed_locations : []
  for (var failedIndex = 0; failedIndex < failedLocations.length; failedIndex++) {
    var failedKey = locationKey(failedLocations[failedIndex])
    if (failedKey) failedKeys[failedKey] = true
  }

  var presentKeys = ({})
  for (var nextIndex = 0; nextIndex < nextLocations.length; nextIndex++) {
    var nextKey = locationKey(nextLocations[nextIndex])
    if (nextKey) presentKeys[nextKey] = true
  }

  var previousLocations = previousPayload
    && Array.isArray(previousPayload.locations) ? previousPayload.locations : []
  for (var previousIndex = 0;
      previousIndex < previousLocations.length; previousIndex++) {
    var previous = previousLocations[previousIndex]
    var previousKey = locationKey(previous)
    if (previousKey && failedKeys[previousKey] && !presentKeys[previousKey]) {
      nextLocations.push(previous)
      presentKeys[previousKey] = true
    }
  }

  var merged = ({})
  for (var key in nextPayload) {
    if (Object.prototype.hasOwnProperty.call(nextPayload, key))
      merged[key] = nextPayload[key]
  }
  merged.locations = nextLocations
  return merged
}
