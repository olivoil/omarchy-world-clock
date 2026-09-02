pragma ComponentBehavior: Bound

import QtQuick

// Orthographic world globe backed by a single scene-graph shader. Geography,
// markers, and hit testing share the same projection math so the surface stays
// physically coherent while it rotates and zooms.
Item {
  id: root

  property color oceanColor: "transparent"
  property color landColor: "transparent"
  property color boundaryColor: "transparent"
  property color rimColor: "transparent"
  property real longitude: -105
  property real latitude: 14
  property real openingZoom: 2.2
  property real openingOverviewZoom: 0.94
  property real openingSpinDegrees: 104
  property int openingFlightDuration: 1080
  property real zoom: openingOverviewZoom
  property real minimumZoom: 0.94
  property real maximumZoom: 4.8
  property real diameterRatio: 0.86
  property bool interactive: true
  property bool highResolutionEnabled: true
  property bool previewReady: false

  readonly property real baseDiameter: Math.max(1,
    Math.min(width, height) * diameterRatio)
  readonly property real sphereDiameter: baseDiameter * zoom
  readonly property real sphereRadius: sphereDiameter / 2
  readonly property real centerX: width / 2
  readonly property real centerY: height / 2
  readonly property bool shaderAvailable: globeEffect.status !== ShaderEffect.Error
  readonly property string shaderLog: globeEffect.log
  readonly property bool dragging: globeDrag.active
  readonly property bool openingFlightRunning: openingMotion.running
  readonly property url previewTextureSource:
    Qt.resolvedUrl("../assets/world-map-preview.png")
  readonly property url textureSource: highResolutionEnabled
    ? Qt.resolvedUrl("../assets/world-map.png")
    : previewTextureSource
  readonly property int textureWidth: highResolutionEnabled ? 8192 : 2048
  readonly property int textureHeight: highResolutionEnabled ? 4096 : 1024

  signal locationPicked(real latitude, real longitude, real viewX, real viewY)
  signal viewInteractionStarted()
  signal viewInteractionFinished()

  function updatePreviewReadiness() {
    if (highResolutionEnabled
        || mapTexture.status !== Image.Ready
        || fallbackMap.status !== Image.Ready
        || String(mapTexture.source) !== String(previewTextureSource)
        || String(fallbackMap.source) !== String(previewTextureSource)) return
    previewReady = true
  }

  onHighResolutionEnabledChanged: {
    if (highResolutionEnabled) return
    previewReady = false
    Qt.callLater(root.updatePreviewReadiness)
  }

  function radians(value) {
    return Number(value) * Math.PI / 180
  }

  function degrees(value) {
    return Number(value) * 180 / Math.PI
  }

  function clampLatitude(value) {
    return Math.max(-85, Math.min(85, Number(value)))
  }

  function clampZoom(value) {
    return Math.max(minimumZoom, Math.min(maximumZoom, Number(value)))
  }

  function normalizedWheelDelta(angleDelta, pixelDelta) {
    var pixels = Number(pixelDelta)
    if (isFinite(pixels) && pixels !== 0)
      return Math.max(-240, Math.min(240, pixels * 8))
    var angle = Number(angleDelta)
    if (!isFinite(angle)) return 0
    return Math.max(-240, Math.min(240, angle))
  }

  function nearestLongitude(target) {
    var rawDelta = Number(target) - longitude
    var delta = ((rawDelta + 180) % 360 + 360) % 360 - 180
    return longitude + delta
  }

  function project(latitudeDegrees, longitudeDegrees) {
    if (!shaderAvailable) {
      var flatX = (Number(longitudeDegrees) + 180) / 360 * width
      var flatY = (90 - Number(latitudeDegrees)) / 180 * height
      return {
        x: flatX,
        y: flatY,
        depth: 1,
        visible: flatX >= -24 && flatX <= width + 24
          && flatY >= -24 && flatY <= height + 24
      }
    }

    var latitudeValue = radians(latitudeDegrees)
    var longitudeValue = radians(longitudeDegrees)
    var centerLatitudeValue = radians(latitude)
    var centerLongitudeValue = radians(longitude)
    var longitudeDelta = longitudeValue - centerLongitudeValue
    var cosLatitude = Math.cos(latitudeValue)
    var sinLatitude = Math.sin(latitudeValue)
    var cosCenterLatitude = Math.cos(centerLatitudeValue)
    var sinCenterLatitude = Math.sin(centerLatitudeValue)
    var cosLongitudeDelta = Math.cos(longitudeDelta)
    var east = cosLatitude * Math.sin(longitudeDelta)
    var north = cosCenterLatitude * sinLatitude
      - sinCenterLatitude * cosLatitude * cosLongitudeDelta
    var depth = sinCenterLatitude * sinLatitude
      + cosCenterLatitude * cosLatitude * cosLongitudeDelta
    var pointX = centerX + sphereRadius * east
    var pointY = centerY - sphereRadius * north
    var insideViewport = pointX >= -24 && pointX <= width + 24
      && pointY >= -24 && pointY <= height + 24
    return {
      x: pointX,
      y: pointY,
      depth: depth,
      visible: depth > 0.025 && insideViewport
    }
  }

  function locationAt(viewX, viewY) {
    if (!shaderAvailable) {
      if (viewX < 0 || viewX > width || viewY < 0 || viewY > height) return null
      return {
        latitude: Math.max(-89.999999, Math.min(89.999999, 90 - viewY / height * 180)),
        longitude: Math.max(-179.999999, Math.min(179.999999, viewX / width * 360 - 180))
      }
    }

    var eastValue = (Number(viewX) - centerX) / sphereRadius
    var northValue = (centerY - Number(viewY)) / sphereRadius
    var radiusSquared = eastValue * eastValue + northValue * northValue
    if (radiusSquared > 1) return null
    var depth = Math.sqrt(Math.max(0, 1 - radiusSquared))
    var centerLatitudeValue = radians(latitude)
    var centerLongitudeValue = radians(longitude)
    var sinLatitude = Math.sin(centerLatitudeValue)
    var cosLatitude = Math.cos(centerLatitudeValue)
    var sinLongitude = Math.sin(centerLongitudeValue)
    var cosLongitude = Math.cos(centerLongitudeValue)

    var worldX = eastValue * -sinLongitude
      + northValue * -sinLatitude * cosLongitude
      + depth * cosLatitude * cosLongitude
    var worldY = eastValue * cosLongitude
      + northValue * -sinLatitude * sinLongitude
      + depth * cosLatitude * sinLongitude
    var worldZ = northValue * cosLatitude + depth * sinLatitude
    return {
      latitude: degrees(Math.asin(Math.max(-1, Math.min(1, worldZ)))),
      longitude: degrees(Math.atan2(worldY, worldX))
    }
  }

  function setView(latitudeDegrees, longitudeDegrees, zoomValue) {
    openingMotion.stop()
    focusMotion.stop()
    inertiaMotion.stop()
    latitude = clampLatitude(latitudeDegrees)
    longitude = Number(longitudeDegrees)
    zoom = clampZoom(zoomValue === undefined ? zoom : zoomValue)
  }

  function focusOn(latitudeDegrees, longitudeDegrees, zoomValue) {
    openingMotion.stop()
    inertiaMotion.stop()
    latitudeFocus.from = latitude
    latitudeFocus.to = clampLatitude(latitudeDegrees)
    longitudeFocus.from = longitude
    longitudeFocus.to = nearestLongitude(longitudeDegrees)
    zoomFocus.from = zoom
    zoomFocus.to = clampZoom(zoomValue === undefined ? Math.max(1.18, zoom) : zoomValue)
    focusMotion.restart()
  }

  function normalizedVector(x, y, z) {
    var length = Math.sqrt(x * x + y * y + z * z)
    if (!isFinite(length) || length < 0.0000001) return null
    return { x: x / length, y: y / length, z: z / length }
  }

  function appendCenterCandidate(candidates, x, y, z) {
    var candidate = normalizedVector(x, y, z)
    if (candidate) candidates.push(candidate)
  }

  function minimumDepthForCenter(center, vectors) {
    var minimumDepth = 1
    for (var index = 0; index < vectors.length; index++) {
      var point = vectors[index]
      minimumDepth = Math.min(minimumDepth,
        center.x * point.x + center.y * point.y + center.z * point.z)
    }
    return minimumDepth
  }

  // The smallest spherical cap enclosing a finite set is supported by one,
  // two, or three boundary points. Evaluate those candidate centers and pick
  // the one with the greatest worst-case depth. Unlike a vector average, this
  // is not biased by duplicate results clustered on one side of the globe.
  function minimumAngularCenter(vectors) {
    var centerCandidates = []
    var sumX = 0
    var sumY = 0
    var sumZ = 0
    for (var sumIndex = 0; sumIndex < vectors.length; sumIndex++) {
      sumX += vectors[sumIndex].x
      sumY += vectors[sumIndex].y
      sumZ += vectors[sumIndex].z
    }
    appendCenterCandidate(centerCandidates, sumX, sumY, sumZ)

    for (var pointIndex = 0; pointIndex < vectors.length; pointIndex++) {
      var point = vectors[pointIndex]
      appendCenterCandidate(centerCandidates, point.x, point.y, point.z)
    }

    for (var leftIndex = 0; leftIndex < vectors.length; leftIndex++) {
      var left = vectors[leftIndex]
      for (var rightIndex = leftIndex + 1; rightIndex < vectors.length; rightIndex++) {
        var right = vectors[rightIndex]
        var beforePairCount = centerCandidates.length
        appendCenterCandidate(centerCandidates,
          left.x + right.x, left.y + right.y, left.z + right.z)
        if (centerCandidates.length === beforePairCount) {
          // Antipodal pairs have infinitely many optimal midpoints. Seed two
          // opposite orthogonal candidates so the remaining points can break
          // the tie deterministically.
          var axisX = Math.abs(left.z) < 0.9 ? 0 : 1
          var axisZ = Math.abs(left.z) < 0.9 ? 1 : 0
          var crossX = left.y * axisZ
          var crossY = left.z * axisX - left.x * axisZ
          var crossZ = -left.y * axisX
          appendCenterCandidate(centerCandidates, crossX, crossY, crossZ)
          appendCenterCandidate(centerCandidates, -crossX, -crossY, -crossZ)
        }
      }
    }

    for (var firstIndex = 0; firstIndex < vectors.length; firstIndex++) {
      var first = vectors[firstIndex]
      for (var secondIndex = firstIndex + 1;
           secondIndex < vectors.length; secondIndex++) {
        var second = vectors[secondIndex]
        var differenceAX = first.x - second.x
        var differenceAY = first.y - second.y
        var differenceAZ = first.z - second.z
        for (var thirdIndex = secondIndex + 1;
             thirdIndex < vectors.length; thirdIndex++) {
          var third = vectors[thirdIndex]
          var differenceBX = first.x - third.x
          var differenceBY = first.y - third.y
          var differenceBZ = first.z - third.z
          var crossCenterX = differenceAY * differenceBZ
            - differenceAZ * differenceBY
          var crossCenterY = differenceAZ * differenceBX
            - differenceAX * differenceBZ
          var crossCenterZ = differenceAX * differenceBY
            - differenceAY * differenceBX
          appendCenterCandidate(centerCandidates,
            crossCenterX, crossCenterY, crossCenterZ)
          appendCenterCandidate(centerCandidates,
            -crossCenterX, -crossCenterY, -crossCenterZ)
        }
      }
    }

    var bestCenter = centerCandidates[0]
    var bestMinimumDepth = -2
    for (var candidateIndex = 0;
         candidateIndex < centerCandidates.length; candidateIndex++) {
      var centerCandidate = centerCandidates[candidateIndex]
      var candidateDepth = minimumDepthForCenter(centerCandidate, vectors)
      if (candidateDepth > bestMinimumDepth) {
        bestCenter = centerCandidate
        bestMinimumDepth = candidateDepth
      }
    }
    return {
      x: bestCenter.x,
      y: bestCenter.y,
      z: bestCenter.z,
      minimumDepth: bestMinimumDepth
    }
  }

  function fittedView(locations) {
    var vectors = []
    var candidates = Array.isArray(locations) ? locations : []
    for (var i = 0; i < candidates.length; i++) {
      var candidate = candidates[i]
      if (!candidate || candidate.latitude === null
          || candidate.latitude === undefined || candidate.longitude === null
          || candidate.longitude === undefined) continue
      var latitudeValue = radians(candidate.latitude)
      var longitudeValue = radians(candidate.longitude)
      var cosLatitude = Math.cos(latitudeValue)
      var vector = {
        x: cosLatitude * Math.cos(longitudeValue),
        y: cosLatitude * Math.sin(longitudeValue),
        z: Math.sin(latitudeValue)
      }
      vectors.push(vector)
    }
    if (vectors.length === 0) return null

    var fittedCenter = minimumAngularCenter(vectors)
    var centerLatitude = radians(clampLatitude(degrees(
      Math.asin(Math.max(-1, Math.min(1, fittedCenter.z))))))
    var centerLongitude = Math.atan2(fittedCenter.y, fittedCenter.x)
    var sinLatitude = Math.sin(centerLatitude)
    var cosLatitude = Math.cos(centerLatitude)
    var sinLongitude = Math.sin(centerLongitude)
    var cosLongitude = Math.cos(centerLongitude)
    var maxEast = 0
    var maxNorth = 0
    var minimumDepth = 1
    for (var vectorIndex = 0; vectorIndex < vectors.length; vectorIndex++) {
      var point = vectors[vectorIndex]
      var east = point.x * -sinLongitude + point.y * cosLongitude
      var north = point.x * -sinLatitude * cosLongitude
        + point.y * -sinLatitude * sinLongitude + point.z * cosLatitude
      var depth = point.x * cosLatitude * cosLongitude
        + point.y * cosLatitude * sinLongitude + point.z * sinLatitude
      maxEast = Math.max(maxEast, Math.abs(east))
      maxNorth = Math.max(maxNorth, Math.abs(north))
      minimumDepth = Math.min(minimumDepth, depth)
    }

    var baseRadius = Math.max(1, baseDiameter / 2)
    var horizontalFit = maxEast > 0.001
      ? width * 0.39 / (baseRadius * maxEast) : maximumZoom
    var verticalFit = maxNorth > 0.001
      ? height * 0.38 / (baseRadius * maxNorth) : maximumZoom
    return {
      latitude: degrees(centerLatitude),
      longitude: degrees(centerLongitude),
      zoom: clampZoom(Math.min(2.75, horizontalFit, verticalFit)),
      minimumDepth: minimumDepth
    }
  }

  function focusOnLocations(locations) {
    var view = fittedView(locations)
    if (!view) return
    var targetZoom = locations.length === 1 ? Math.max(zoom, view.zoom) : view.zoom
    focusOn(view.latitude, view.longitude, targetZoom)
  }

  function settleOn(latitudeDegrees, longitudeDegrees) {
    // Match Hurricane Tracker's opening-flight contract exactly: a complete
    // globe, 104 degrees of travel, and a comfortable 2.2x regional landing.
    var targetLatitude = Math.max(-82, Math.min(82, Number(latitudeDegrees)))
    var targetLongitude = Number(longitudeDegrees)
    if (!isFinite(targetLatitude) || !isFinite(targetLongitude)) return

    stopMotion()
    // Treat rotation and zoom as one camera flight. Matching their duration
    // and curve keeps the destination on one continuous approach instead of
    // making the zoom read as a second movement.
    latitude = Math.max(-10, Math.min(10, targetLatitude * 0.16))
    longitude = targetLongitude - openingSpinDegrees
    zoom = clampZoom(openingOverviewZoom)
    openingLatitude.from = latitude
    openingLatitude.to = targetLatitude
    openingLongitude.from = longitude
    openingLongitude.to = targetLongitude
    openingZoomMotion.from = zoom
    openingZoomMotion.to = clampZoom(openingZoom)
    openingMotion.restart()
  }

  function stopMotion() {
    openingMotion.stop()
    focusMotion.stop()
    inertiaMotion.stop()
  }

  function zoomBy(wheelDelta) {
    var multiplier = Math.pow(1.00145, Number(wheelDelta))
    zoom = clampZoom(zoom * multiplier)
  }

  function startInertia(horizontalVelocity, verticalVelocity) {
    var horizontal = Number(horizontalVelocity)
    var vertical = Number(verticalVelocity)
    if (!isFinite(horizontal) || !isFinite(vertical)) return
    var speed = Math.sqrt(horizontal * horizontal + vertical * vertical)
    if (speed < 80) return
    var longitudeTravel = Math.max(-34, Math.min(34,
      -horizontal / Math.max(1, sphereRadius) * 12))
    var latitudeTravel = Math.max(-18, Math.min(18,
      vertical / Math.max(1, sphereRadius) * 10))
    longitudeInertia.from = longitude
    longitudeInertia.to = longitude + longitudeTravel
    latitudeInertia.from = latitude
    latitudeInertia.to = clampLatitude(latitude + latitudeTravel)
    inertiaMotion.restart()
  }

  Image {
    id: mapTexture
    visible: false
    source: root.textureSource
    sourceSize.width: root.textureWidth
    sourceSize.height: root.textureHeight
    asynchronous: true
    retainWhileLoading: true
    cache: true
    smooth: true
    mipmap: true
    onStatusChanged: root.updatePreviewReadiness()
  }

  Image {
    id: fallbackMap
    anchors.fill: parent
    visible: !root.shaderAvailable
    source: root.textureSource
    sourceSize.width: root.textureWidth
    sourceSize.height: root.textureHeight
    asynchronous: true
    retainWhileLoading: true
    fillMode: Image.Stretch
    smooth: true
    mipmap: true
    opacity: 0.78
    onStatusChanged: root.updatePreviewReadiness()
  }

  Item {
    id: sphere
    x: root.centerX - width / 2
    y: root.centerY - height / 2
    width: root.sphereDiameter
    height: width
    visible: root.shaderAvailable

    ShaderEffect {
      id: globeEffect
      anchors.fill: parent
      property var source: mapTexture
      property real centerLongitude: root.radians(root.longitude)
      property real centerLatitude: root.radians(root.latitude)
      property color oceanColor: root.oceanColor
      property color landColor: root.landColor
      property color boundaryColor: root.boundaryColor
      property color rimColor: root.rimColor
      fragmentShader: Qt.resolvedUrl("../assets/globe.frag.qsb")
      blending: true
    }

    Rectangle {
      anchors.fill: parent
      radius: width / 2
      color: "transparent"
      border.width: 1
      border.color: root.rimColor
      opacity: 0.44
    }
  }

  DragHandler {
    id: globeDrag
    parent: sphere
    target: null
    enabled: root.interactive && root.shaderAvailable
    acceptedButtons: Qt.LeftButton
    cursorShape: active ? Qt.ClosedHandCursor : Qt.OpenHandCursor
    property real startLongitude: 0
    property real startLatitude: 0

    onActiveChanged: {
      if (active) {
        openingMotion.stop()
        focusMotion.stop()
        inertiaMotion.stop()
        startLongitude = root.longitude
        startLatitude = root.latitude
        root.viewInteractionStarted()
      } else {
        root.startInertia(centroid.velocity.x, centroid.velocity.y)
        root.viewInteractionFinished()
      }
    }
    onActiveTranslationChanged: {
      if (!active) return
      root.longitude = startLongitude
        - activeTranslation.x / Math.max(1, root.sphereRadius) * 74
      root.latitude = root.clampLatitude(startLatitude
        + activeTranslation.y / Math.max(1, root.sphereRadius) * 66)
    }
  }

  TapHandler {
    parent: root.shaderAvailable ? sphere : fallbackMap
    enabled: root.interactive
    acceptedButtons: Qt.LeftButton
    gesturePolicy: TapHandler.DragThreshold
    onTapped: function(eventPoint) {
      var rootPoint = parent.mapToItem(root, eventPoint.position)
      var viewX = rootPoint.x
      var viewY = rootPoint.y
      var location = root.locationAt(viewX, viewY)
      if (location)
        root.locationPicked(location.latitude, location.longitude, viewX, viewY)
    }
  }

  WheelHandler {
    parent: root.shaderAvailable ? sphere : fallbackMap
    enabled: root.interactive && root.shaderAvailable
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    onWheel: function(event) {
      var delta = root.normalizedWheelDelta(event.angleDelta.y, event.pixelDelta.y)
      if (delta === 0) return
      openingMotion.stop()
      focusMotion.stop()
      inertiaMotion.stop()
      root.zoomBy(delta)
      event.accepted = true
    }
  }

  ParallelAnimation {
    id: openingMotion
    NumberAnimation {
      id: openingLatitude
      target: root
      property: "latitude"
      duration: root.openingFlightDuration
      easing.type: Easing.InOutSine
    }
    NumberAnimation {
      id: openingLongitude
      target: root
      property: "longitude"
      duration: root.openingFlightDuration
      easing.type: Easing.InOutSine
    }
    NumberAnimation {
      id: openingZoomMotion
      target: root
      property: "zoom"
      duration: root.openingFlightDuration
      easing.type: Easing.InOutSine
    }
  }

  ParallelAnimation {
    id: focusMotion
    NumberAnimation {
      id: latitudeFocus
      target: root
      property: "latitude"
      duration: 240
      easing.type: Easing.OutQuint
    }
    NumberAnimation {
      id: longitudeFocus
      target: root
      property: "longitude"
      duration: 240
      easing.type: Easing.OutQuint
    }
    NumberAnimation {
      id: zoomFocus
      target: root
      property: "zoom"
      duration: 240
      easing.type: Easing.OutQuint
    }
  }

  ParallelAnimation {
    id: inertiaMotion
    NumberAnimation {
      id: latitudeInertia
      target: root
      property: "latitude"
      duration: 280
      easing.type: Easing.OutQuint
    }
    NumberAnimation {
      id: longitudeInertia
      target: root
      property: "longitude"
      duration: 280
      easing.type: Easing.OutQuint
    }
  }
}
