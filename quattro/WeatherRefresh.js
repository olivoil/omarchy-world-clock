function timestampWithin(now, timestamp, duration) {
  var current = Number(now)
  var previous = Number(timestamp)
  var window = Number(duration)
  if (!isFinite(current) || !isFinite(previous) || previous <= 0
      || !isFinite(window) || window <= 0) return false
  // Treat a wall-clock adjustment into the past as age zero. This keeps a
  // clock correction from turning a freshness check into a request burst.
  return current < previous || current - previous < window
}

function requestNeeded(state) {
  var value = state || ({})
  var signature = String(value.signature || "")
  if (!signature) return false

  var recentlyAttempted = signature === String(value.attemptedSignature || "")
    && timestampWithin(value.now, value.lastAttemptAt,
      value.attemptCooldownMilliseconds)
  if (recentlyAttempted) return false

  var fresh = value.failed !== true
    && signature === String(value.loadedSignature || "")
    && timestampWithin(value.now, value.lastUpdatedAt,
      value.freshnessMilliseconds)
  return value.manual === true || !fresh
}
