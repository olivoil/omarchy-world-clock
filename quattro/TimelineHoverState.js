function copyOwners(owners) {
  var source = owners || ({})
  var copy = ({})
  for (var owner in source)
    if (Object.prototype.hasOwnProperty.call(source, owner)) copy[owner] = source[owner]
  return copy
}

function updateOwners(owners, owner, relativeMinutes, hovered) {
  var normalizedOwner = String(owner || "")
  var next = copyOwners(owners)
  if (hovered) next[normalizedOwner] = Number(relativeMinutes || 0)
  else delete next[normalizedOwner]
  return next
}

function matchesMinutes(owners, relativeMinutes) {
  var active = owners || ({})
  var target = Number(relativeMinutes || 0)
  for (var owner in active)
    if (Object.prototype.hasOwnProperty.call(active, owner)
        && Number(active[owner]) === target) return true
  return false
}

function markerHovered(owners, marker) {
  if (!marker) return false
  var minutes = Array.isArray(marker.minutes) ? marker.minutes : [marker.minute]
  for (var index = 0; index < minutes.length; index++)
    if (matchesMinutes(owners, minutes[index])) return true
  return false
}

function sourceMarkerHovered(owners, markers) {
  if (!Array.isArray(markers)) return false
  for (var index = 0; index < markers.length; index++)
    if (markers[index] && markers[index].source === true
        && markerHovered(owners, markers[index])) return true
  return false
}
