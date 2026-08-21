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
  property real longitude: 0
  property real latitude: 18
  property real openingZoom: 2.18
  property real zoom: openingZoom
  property real minimumZoom: 0.94
  property real maximumZoom: 2.8
  property real diameterRatio: 0.57
  property bool interactive: true

  readonly property real baseDiameter: Math.max(1,
    Math.min(height - 2, width * diameterRatio))
  readonly property real sphereDiameter: baseDiameter * zoom
  readonly property real sphereRadius: sphereDiameter / 2
  readonly property real centerX: width / 2
  readonly property real centerY: height / 2
  readonly property bool shaderAvailable: globeEffect.status !== ShaderEffect.Error
  readonly property string shaderLog: globeEffect.log
  readonly property bool dragging: globeDrag.active

  signal locationPicked(real latitude, real longitude, real viewX, real viewY)
  signal viewInteractionStarted()
  signal viewInteractionFinished()

  function radians(value) {
    return Number(value) * Math.PI / 180
  }

  function degrees(value) {
    return Number(value) * 180 / Math.PI
  }

  function clampLatitude(value) {
    return Math.max(-72, Math.min(72, Number(value)))
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
    var delta = (Number(target) - longitude + 540) % 360 - 180
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
    focusMotion.stop()
    inertiaMotion.stop()
    latitude = clampLatitude(latitudeDegrees)
    longitude = Number(longitudeDegrees)
    zoom = clampZoom(zoomValue === undefined ? zoom : zoomValue)
  }

  function focusOn(latitudeDegrees, longitudeDegrees, zoomValue) {
    inertiaMotion.stop()
    latitudeFocus.from = latitude
    latitudeFocus.to = clampLatitude(latitudeDegrees)
    longitudeFocus.from = longitude
    longitudeFocus.to = nearestLongitude(longitudeDegrees)
    zoomFocus.from = zoom
    zoomFocus.to = clampZoom(zoomValue === undefined ? Math.max(1.18, zoom) : zoomValue)
    focusMotion.restart()
  }

  function settleOn(latitudeDegrees, longitudeDegrees) {
    setView(clampLatitude(Number(latitudeDegrees) + 1.5), Number(longitudeDegrees) - 7,
      openingZoom * 1.06)
    focusOn(latitudeDegrees, longitudeDegrees, openingZoom)
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
    source: Qt.resolvedUrl("../assets/world-map.png")
    sourceSize.width: 1800
    sourceSize.height: 900
    asynchronous: false
    cache: true
    smooth: true
    mipmap: true
  }

  Image {
    anchors.fill: parent
    visible: !root.shaderAvailable
    source: Qt.resolvedUrl("../assets/world-map.png")
    fillMode: Image.Stretch
    smooth: true
    mipmap: true
    opacity: 0.78
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
    enabled: root.interactive
    acceptedButtons: Qt.LeftButton
    gesturePolicy: TapHandler.DragThreshold
    onTapped: function(eventPoint) {
      var viewX = eventPoint.position.x
      var viewY = eventPoint.position.y
      var location = root.locationAt(viewX, viewY)
      if (location)
        root.locationPicked(location.latitude, location.longitude, viewX, viewY)
    }
  }

  WheelHandler {
    enabled: root.interactive && root.shaderAvailable
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    onWheel: function(event) {
      var delta = root.normalizedWheelDelta(event.angleDelta.y, event.pixelDelta.y)
      if (delta === 0) return
      focusMotion.stop()
      inertiaMotion.stop()
      root.zoomBy(delta)
      event.accepted = true
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
