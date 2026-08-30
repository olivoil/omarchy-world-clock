function copyOwners(owners) {
  var source = owners || ({})
  var copy = ({})
  for (var owner in source)
    if (Object.prototype.hasOwnProperty.call(source, owner)) copy[owner] = source[owner]
  return copy
}

function updateOwners(owners, owner, locationIdentity, hovered) {
  var normalizedOwner = String(owner || "")
  var next = copyOwners(owners)
  if (hovered) next[normalizedOwner] = String(locationIdentity || "")
  else delete next[normalizedOwner]
  return next
}

function matchesIdentity(owners, locationIdentity) {
  var active = owners || ({})
  var target = String(locationIdentity || "")
  for (var owner in active)
    if (Object.prototype.hasOwnProperty.call(active, owner)
        && String(active[owner]) === target) return true
  return false
}

function listLength(values) {
  if (!values || values.length === undefined) return 0
  var length = Math.floor(Number(values.length))
  return isFinite(length) && length > 0 ? length : 0
}

function markerHovered(owners, marker) {
  if (!marker) return false
  var identities = marker.identities
  for (var index = 0; index < listLength(identities); index++)
    if (matchesIdentity(owners, identities[index])) return true
  return false
}

function sourceMarkerHovered(owners, markers) {
  for (var index = 0; index < listLength(markers); index++)
    if (markers[index] && markers[index].source === true
        && markerHovered(owners, markers[index])) return true
  return false
}
