pragma ComponentBehavior: Bound

import QtQuick
import qs.Commons
import "WeatherDetailLogic.js" as WeatherDetailLogic

Item {
  id: graph

  required property var controller
  property var hours: []
  property string metric: "uv"
  property color foreground: controller.contentForeground
  property color muted: Qt.darker(foreground, 1.48)
  property color temperatureColor: controller.mixColor(
    foreground, Qt.rgba(0.96, 0.72, 0.27, 1), 0.72)
  property color precipitationColor: controller.mixColor(
    foreground, Qt.rgba(0.34, 0.72, 0.91, 1), 0.72)
  property color uvColor: controller.mixColor(
    foreground, Qt.rgba(0.95, 0.48, 0.32, 1), 0.72)
  property color windColor: controller.mixColor(
    foreground, Qt.rgba(0.49, 0.82, 0.68, 1), 0.68)
  readonly property var labelHours: WeatherDetailLogic.sampleLabelHours(hours, 8)

  implicitHeight: Style.space(145)
  Accessible.role: Accessible.Chart
  Accessible.name: "24-hour " + metric + " forecast"

  function finite(value) {
    if (value === null || value === undefined || value === "") return NaN
    return Number(value)
  }

  function displayTemperature(value) {
    var celsius = finite(value)
    if (!isFinite(celsius)) return "—"
    return controller.weatherTemperatureCompact(celsius)
  }

  function displayWind(value) {
    var speed = finite(value)
    if (!isFinite(speed)) return "—"
    if (controller.weatherUseImperial) speed *= 0.621371
    return String(Math.round(speed)) + (controller.weatherUseImperial ? " mph" : " km/h")
  }

  function displayPrecipitation(item) {
    var amount = finite(item ? item.precipitation_mm : null)
    var probability = finite(item ? item.precipitation_probability_percent : null)
    var amountText = isFinite(amount) ? controller.weatherPrecipitation(amount) : "—"
    return isFinite(probability)
      ? amountText + " · " + String(Math.round(probability)) + "%"
      : amountText
  }

  function displayValue(item) {
    if (metric === "temperature")
      return displayTemperature(item ? item.temperature_celsius : null)
    if (metric === "precipitation") return displayPrecipitation(item)
    if (metric === "wind") return displayWind(WeatherDetailLogic.windValue(item))
    var uv = finite(item ? item.uv_index : null)
    return isFinite(uv) ? String(Math.round(uv)) : "—"
  }

  function valueFor(item, selectedMetric) {
    if (!item) return NaN
    if (selectedMetric === "temperature") {
      var temperature = finite(item.temperature_celsius)
      return controller.weatherUseImperial && isFinite(temperature)
        ? temperature * 9 / 5 + 32 : temperature
    }
    if (selectedMetric === "precipitation")
      return finite(item.precipitation_probability_percent)
    if (selectedMetric === "wind") {
      var wind = WeatherDetailLogic.windValue(item)
      return controller.weatherUseImperial && isFinite(wind)
        ? wind * 0.621371 : wind
    }
    return finite(item.uv_index)
  }

  function maximumFor(field) {
    var maximum = 0
    for (var i = 0; i < hours.length; i++) {
      var value = finite(hours[i] ? hours[i][field] : null)
      if (isFinite(value)) maximum = Math.max(maximum, value)
    }
    return maximum
  }

  function canvasColor(color, alpha) {
    return "rgba(" + String(Math.round(color.r * 255)) + ","
      + String(Math.round(color.g * 255)) + ","
      + String(Math.round(color.b * 255)) + ","
      + String(Math.max(0, Math.min(1, Number(alpha)))) + ")"
  }

  Canvas {
    id: plot
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.right: parent.right
    height: Style.space(110)
    renderTarget: Canvas.Image

    property var forecastHours: graph.hours
    property string selectedMetric: graph.metric
    property color lineForeground: graph.foreground
    property color temperatureLine: graph.temperatureColor
    property color precipitationLine: graph.precipitationColor
    property color uvLine: graph.uvColor
    property color windLine: graph.windColor

    onForecastHoursChanged: requestPaint()
    onSelectedMetricChanged: requestPaint()
    onLineForegroundChanged: requestPaint()
    onTemperatureLineChanged: requestPaint()
    onPrecipitationLineChanged: requestPaint()
    onUvLineChanged: requestPaint()
    onWindLineChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    Component.onCompleted: requestPaint()

    onPaint: {
      var context = getContext("2d")
      context.clearRect(0, 0, width, height)
      var top = Style.space(7)
      var bottom = height - Style.space(8)
      var plotHeight = Math.max(1, bottom - top)

      context.lineWidth = Style.spacing.hairline
      context.strokeStyle = graph.canvasColor(lineForeground, 0.08)
      for (var gridIndex = 0; gridIndex < 4; gridIndex++) {
        var gridY = top + plotHeight * gridIndex / 3
        context.beginPath()
        context.moveTo(0, gridY)
        context.lineTo(width, gridY)
        context.stroke()
      }

      if (!forecastHours || forecastHours.length < 1) return

      var selectedColor = selectedMetric === "temperature" ? temperatureLine
        : (selectedMetric === "precipitation" ? precipitationLine
          : (selectedMetric === "wind" ? windLine : uvLine))
      var values = []
      var minimum = Number.POSITIVE_INFINITY
      var maximum = Number.NEGATIVE_INFINITY
      var hasLineValues = false
      for (var valueIndex = 0; valueIndex < forecastHours.length; valueIndex++) {
        var value = graph.valueFor(forecastHours[valueIndex], selectedMetric)
        values.push(value)
        if (isFinite(value)) {
          hasLineValues = true
          minimum = Math.min(minimum, value)
          maximum = Math.max(maximum, value)
        }
      }

      if (selectedMetric === "precipitation") {
        minimum = 0
        maximum = 100
        var amountMaximum = Math.max(0.1, graph.maximumFor("precipitation_mm"))
        var barWidth = Math.max(Style.space(4), width / forecastHours.length * 0.13)
        context.fillStyle = graph.canvasColor(precipitationLine, 0.20)
        for (var barIndex = 0; barIndex < forecastHours.length; barIndex++) {
          var amount = graph.finite(forecastHours[barIndex]
            ? forecastHours[barIndex].precipitation_mm : null)
          if (!isFinite(amount) || amount <= 0) continue
          var barX = forecastHours.length === 1 ? width / 2
            : barIndex * width / (forecastHours.length - 1)
          var barHeight = amount / amountMaximum * plotHeight * 0.88
          context.fillRect(barX - barWidth / 2, bottom - barHeight,
            barWidth, barHeight)
        }
        // Open-Meteo can supply measured/forecast amounts even where the
        // probability series is unavailable. Keep those bars visible without
        // inventing a probability line.
        if (!hasLineValues) return
      } else {
        if (!hasLineValues) return
        if (selectedMetric === "uv") {
          minimum = 0
          maximum = Math.max(11, maximum)
          var thresholdY = bottom - 8 / maximum * plotHeight
          context.fillStyle = graph.canvasColor(uvLine, 0.065)
          context.fillRect(0, top, width, Math.max(0, thresholdY - top))
          context.fillStyle = graph.canvasColor(uvLine, 0.72)
          context.font = String(Style.font.caption) + "px "
            + graph.controller.contentFontFamily
          context.fillText("VERY HIGH · 8+", Style.space(7), top + Style.space(8))
        } else {
          var range = maximum - minimum
          if (range < 1) {
            minimum -= 0.5
            maximum += 0.5
          } else {
            minimum -= range * 0.12
            maximum += range * 0.12
          }
        }
      }

      function pointX(index) {
        return WeatherDetailLogic.plotFraction(index, forecastHours.length) * width
      }
      function pointY(value) {
        return bottom - (value - minimum) / Math.max(0.001,
          maximum - minimum) * plotHeight
      }

      var firstValid = -1
      for (var startIndex = 0; startIndex < values.length; startIndex++) {
        if (isFinite(values[startIndex])) {
          firstValid = startIndex
          break
        }
      }
      if (firstValid < 0) return

      context.lineWidth = Style.space(1.35)
      context.strokeStyle = graph.canvasColor(selectedColor, 0.92)
      context.beginPath()
      var previousX = pointX(firstValid)
      var previousY = pointY(values[firstValid])
      context.moveTo(previousX, previousY)
      for (var lineIndex = firstValid + 1; lineIndex < values.length; lineIndex++) {
        if (!isFinite(values[lineIndex])) continue
        var x = pointX(lineIndex)
        var y = pointY(values[lineIndex])
        var midpointX = (previousX + x) / 2
        context.quadraticCurveTo(previousX, previousY,
          midpointX, (previousY + y) / 2)
        previousX = x
        previousY = y
      }
      context.lineTo(previousX, previousY)
      context.stroke()

      for (var dotIndex = 0; dotIndex < values.length; dotIndex++) {
        if (!isFinite(values[dotIndex])) continue
        var dotX = pointX(dotIndex)
        var dotY = pointY(values[dotIndex])
        context.beginPath()
        context.arc(dotX, dotY, Style.space(2.25), 0, Math.PI * 2)
        context.fillStyle = graph.canvasColor(Color.popups.background, 1)
        context.fill()
        context.lineWidth = Style.space(1.15)
        context.strokeStyle = graph.canvasColor(selectedColor, 0.95)
        context.stroke()
      }

      var currentIndex = -1
      for (var currentSearch = 0; currentSearch < forecastHours.length;
          currentSearch++) {
        if (graph.controller.weatherHourlyIsCurrent(forecastHours[currentSearch])) {
          currentIndex = currentSearch
          break
        }
      }
      if (currentIndex >= 0) {
        var currentX = pointX(currentIndex)
        context.save()
        context.setLineDash([Style.space(2), Style.space(3)])
        context.lineWidth = Style.spacing.hairline
        context.strokeStyle = graph.canvasColor(lineForeground, 0.28)
        context.beginPath()
        context.moveTo(currentX, top)
        context.lineTo(currentX, bottom)
        context.stroke()
        context.restore()
      }
    }
  }

  Item {
    id: valueRow
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    height: Style.space(30)

    Repeater {
      model: graph.labelHours

      Item {
        id: valueCell
        required property var modelData
        width: valueRow.width / Math.max(1, graph.labelHours.length)
        height: valueRow.height
        x: WeatherDetailLogic.plotFraction(
          Number(modelData ? modelData.sourceIndex : 0),
          graph.hours ? graph.hours.length : 0) * valueRow.width - width / 2

        Column {
          anchors.horizontalCenter: parent.horizontalCenter
          spacing: Style.space(2)

          Text {
            textFormat: Text.PlainText
            anchors.horizontalCenter: parent.horizontalCenter
            text: graph.displayValue(valueCell.modelData
              ? valueCell.modelData.item : null)
            color: graph.foreground
            font.family: graph.controller.contentFontFamily
            font.pixelSize: Style.font.bodySmall
            font.bold: true
          }

          Text {
            textFormat: Text.PlainText
            anchors.horizontalCenter: parent.horizontalCenter
            text: graph.controller.weatherHourlyTime(valueCell.modelData
              ? valueCell.modelData.item : null)
            color: graph.muted
            font.family: graph.controller.contentFontFamily
            font.pixelSize: Style.font.caption
            font.letterSpacing: 0.25
          }
        }
      }
    }
  }
}
