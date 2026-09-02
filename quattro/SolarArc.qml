pragma ComponentBehavior: Bound

import QtQuick
import qs.Commons
import "TimeRail.js" as TimeRail

Item {
  id: root

  property var daylight: ({ curve_positions: [], curve_heights: [] })
  property real localMinutes: 0
  property color trackColor: Qt.rgba(0, 0, 0, 0)
  property color arcColor: trackColor
  property color markerColor: arcColor
  property real trackOpacity: 0.10
  property real curveFillOpacity: 0.12
  property real curveStrokeOpacity: 0.38
  property real markerDiameter: Style.space(14)
  property real markerOpacity: 0.96
  property real haloOpacity: 0.16
  property int motionDuration: 180
  property bool positionAnimationEnabled: true

  Behavior on markerColor { ColorAnimation { duration: 150 } }

  readonly property var curvePositions:
    daylight && daylight.curve_positions ? daylight.curve_positions : []
  readonly property var curveHeights:
    daylight && daylight.curve_heights ? daylight.curve_heights : []
  readonly property real dayPosition: TimeRail.localDayPosition(localMinutes)
  readonly property real solarHeight:
    root.curveHeightAt(dayPosition, curvePositions, curveHeights)

  Accessible.ignored: true

  function curveHeightAt(position, positions, heights) {
    if (!Array.isArray(positions) || !Array.isArray(heights)
        || positions.length < 2 || positions.length !== heights.length) return 0
    var target = Math.max(0, Math.min(1, Number(position)))
    if (target < Number(positions[0])
        || target > Number(positions[positions.length - 1])) return 0
    for (var index = 1; index < positions.length; index++) {
      var right = Number(positions[index])
      if (target > right) continue
      var left = Number(positions[index - 1])
      var span = Math.max(0.000001, right - left)
      var progress = Math.max(0, Math.min(1, (target - left) / span))
      var startHeight = Number(heights[index - 1])
      var endHeight = Number(heights[index])
      return Math.max(0, Math.min(1,
        startHeight + (endHeight - startHeight) * progress))
    }
    return 0
  }

  Canvas {
    id: arcCanvas
    anchors.fill: parent
    Accessible.ignored: true

    function canvasColor(color, alpha) {
      return "rgba(" + String(Math.round(color.r * 255)) + ","
        + String(Math.round(color.g * 255)) + ","
        + String(Math.round(color.b * 255)) + ","
        + String(Math.max(0, Math.min(1, Number(alpha)))) + ")"
    }

    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    Component.onCompleted: requestPaint()

    Connections {
      target: root
      function onCurvePositionsChanged() { arcCanvas.requestPaint() }
      function onCurveHeightsChanged() { arcCanvas.requestPaint() }
      function onTrackColorChanged() { arcCanvas.requestPaint() }
      function onArcColorChanged() { arcCanvas.requestPaint() }
      function onTrackOpacityChanged() { arcCanvas.requestPaint() }
      function onCurveFillOpacityChanged() { arcCanvas.requestPaint() }
      function onCurveStrokeOpacityChanged() { arcCanvas.requestPaint() }
      function onMarkerDiameterChanged() { arcCanvas.requestPaint() }
    }

    onPaint: {
      var context = getContext("2d")
      context.clearRect(0, 0, width, height)
      var markerRadius = Math.max(0, root.markerDiameter / 2)
      var baseline = Math.max(markerRadius, height - markerRadius)
      var amplitude = Math.max(0, baseline - markerRadius)

      context.save()
      context.beginPath()
      context.moveTo(0, baseline)
      context.lineTo(width, baseline)
      context.strokeStyle = canvasColor(root.trackColor, root.trackOpacity)
      context.lineWidth = Style.spacing.hairline
      context.stroke()
      context.restore()

      if (root.curvePositions.length < 2
          || root.curvePositions.length !== root.curveHeights.length) return

      var firstX = Math.max(0, Math.min(1,
        Number(root.curvePositions[0]))) * width
      var lastX = firstX
      context.save()
      context.beginPath()
      context.moveTo(firstX, baseline)
      for (var curveIndex = 0;
          curveIndex < root.curvePositions.length; curveIndex++) {
        var curveX = Math.max(0, Math.min(1,
          Number(root.curvePositions[curveIndex]))) * width
        var curveHeight = Math.max(0, Math.min(1,
          Number(root.curveHeights[curveIndex])))
        context.lineTo(curveX, baseline - curveHeight * amplitude)
        lastX = curveX
      }
      context.lineTo(lastX, baseline)
      context.closePath()
      var glow = context.createLinearGradient(0, markerRadius, 0, baseline)
      glow.addColorStop(0, canvasColor(root.arcColor, 0))
      glow.addColorStop(1,
        canvasColor(root.arcColor, root.curveFillOpacity))
      context.fillStyle = glow
      context.fill()
      context.restore()

      context.save()
      context.beginPath()
      for (var lineIndex = 0;
          lineIndex < root.curvePositions.length; lineIndex++) {
        var lineX = Math.max(0, Math.min(1,
          Number(root.curvePositions[lineIndex]))) * width
        var lineHeight = Math.max(0, Math.min(1,
          Number(root.curveHeights[lineIndex])))
        var lineY = baseline - lineHeight * amplitude
        if (lineIndex === 0) context.moveTo(lineX, lineY)
        else context.lineTo(lineX, lineY)
      }
      context.strokeStyle = canvasColor(
        root.arcColor, root.curveStrokeOpacity)
      context.lineWidth = Style.spacing.hairline
      context.stroke()
      context.restore()
    }
  }

  Item {
    id: solarMarker
    width: Math.max(0, root.markerDiameter)
    height: width
    x: Math.round(Math.max(0, Math.min(root.width - width,
      root.dayPosition * root.width - width / 2)))
    y: Math.round((1 - root.solarHeight)
      * Math.max(0, root.height - height))
    opacity: root.markerOpacity
    visible: width > 0 && root.width >= width && root.height >= height
    Accessible.ignored: true

    Rectangle {
      anchors.fill: parent
      radius: width / 2
      color: root.markerColor
      opacity: root.haloOpacity
    }

    Rectangle {
      anchors.centerIn: parent
      width: Math.max(Style.spacing.hairline,
        Math.round(parent.width * 0.38))
      height: width
      radius: width / 2
      color: root.markerColor
    }

    Behavior on x {
      enabled: root.positionAnimationEnabled
      NumberAnimation {
        duration: root.motionDuration
        easing.type: Easing.OutQuart
      }
    }
    Behavior on y {
      enabled: root.positionAnimationEnabled
      NumberAnimation {
        duration: root.motionDuration
        easing.type: Easing.OutQuart
      }
    }
    Behavior on opacity {
      NumberAnimation { duration: 150; easing.type: Easing.OutQuart }
    }
  }
}
