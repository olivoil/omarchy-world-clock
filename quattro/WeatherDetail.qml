pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC
import qs.Commons
import qs.Ui

Item {
  id: detail

  required property var controller
  property string selectedMetric: "uv"
  property bool metricWasChosen: false
  readonly property string detailKey: String(controller.weatherDetailKey || "")
  readonly property var weatherData: controller.weatherDetailData
  readonly property var clockData: controller.weatherDetailClock
  readonly property var today: controller.weatherDetailToday
  readonly property var hourly: controller.weatherDetailHourlyForecast || []
  readonly property var forecast: controller.weatherDetailForecast || []
  readonly property var graphHours: hourly
  readonly property var phases: buildPhases(hourly)
  readonly property var metricOptions: buildMetricOptions()
  readonly property color foreground: controller.contentForeground
  readonly property color muted: Qt.darker(foreground, 1.46)
  readonly property color dim: Qt.darker(foreground, 1.65)
  readonly property color amber: controller.mixColor(
    foreground, Qt.rgba(0.96, 0.72, 0.27, 1), 0.72)
  readonly property color blue: controller.mixColor(
    foreground, Qt.rgba(0.34, 0.72, 0.91, 1), 0.72)
  readonly property color coral: controller.mixColor(
    foreground, Qt.rgba(0.95, 0.48, 0.32, 1), 0.72)
  readonly property color violet: controller.mixColor(
    foreground, Qt.rgba(0.66, 0.43, 0.87, 1), 0.72)
  readonly property string proseFontFamily: "sans-serif"
  readonly property real sectionInset: Style.space(34)

  signal backRequested()
  signal attributionRequested()

  height: Style.space(460)

  function finite(value) {
    if (value === null || value === undefined || value === "") return NaN
    return Number(value)
  }

  function precipitationRelevant() {
    for (var index = 0; index < hourly.length; index++) {
      var probability = finite(hourly[index].precipitation_probability_percent)
      var amount = finite(hourly[index].precipitation_mm)
      if ((isFinite(probability) && probability >= 20)
          || (isFinite(amount) && amount >= 0.1)) return true
    }
    return false
  }

  function hasHourlyField(field) {
    for (var index = 0; index < hourly.length; index++) {
      if (isFinite(finite(hourly[index] ? hourly[index][field] : null))) return true
    }
    return false
  }

  function buildMetricOptions() {
    var options = [{ key: "temperature", label: "TEMP" }]
    if (precipitationRelevant())
      options.push({ key: "precipitation", label: "RAIN" })
    if (hasHourlyField("uv_index")) options.push({ key: "uv", label: "UV INDEX" })
    if (hasHourlyField("wind_speed_kmh")) options.push({ key: "wind", label: "WIND" })
    return options
  }

  function metricAvailable(metric) {
    for (var index = 0; index < metricOptions.length; index++) {
      if (metricOptions[index].key === metric) return true
    }
    return false
  }

  function chooseDefaultMetric() {
    if (hasHourlyField("uv_index")) selectedMetric = "uv"
    else if (precipitationRelevant()) selectedMetric = "precipitation"
    else selectedMetric = "temperature"
  }

  function phaseProbability(items, maximum) {
    var result = maximum ? 0 : 100
    var found = false
    for (var index = 0; index < items.length; index++) {
      var value = finite(items[index].precipitation_probability_percent)
      if (!isFinite(value)) continue
      found = true
      result = maximum ? Math.max(result, value) : Math.min(result, value)
    }
    return found ? result : NaN
  }

  function phaseRepresentative(items) {
    if (!items.length) return null
    var representative = items[Math.floor(items.length / 2)]
    var bestScore = -1
    for (var index = 0; index < items.length; index++) {
      var code = Number(items[index].weather_code)
      var probability = finite(items[index].precipitation_probability_percent)
      var score = (code >= 95 ? 300 : (code >= 51 ? 200 : code))
        + (isFinite(probability) ? probability : 0)
      if (score > bestScore) {
        bestScore = score
        representative = items[index]
      }
    }
    return representative
  }

  function phasePrecipitationTitle(kind, state) {
    if (state === "easing") {
      if (kind === "storm") return "Storms easing"
      if (kind === "snow") return "Snow easing"
      if (kind === "ice") return "Icy mix easing"
      return "Clearing slowly"
    }
    if (state === "building") {
      if (kind === "storm") return "Storms building"
      if (kind === "snow") return "Snow increasing"
      if (kind === "ice") return "Icy mix increasing"
      return "Rain gathering"
    }
    if (state === "likely") {
      if (kind === "storm") return "Storms likely"
      if (kind === "snow") return "Snow likely"
      if (kind === "ice") return "Icy mix likely"
      return "Rain likely"
    }
    if (kind === "storm") return "Passing storms"
    if (kind === "snow") return "Periods of snow"
    if (kind === "ice") return "Icy mix possible"
    return "Passing showers"
  }

  function phaseTitle(items) {
    if (!items.length) return "Conditions unavailable"
    var firstProbability = finite(items[0].precipitation_probability_percent)
    var lastProbability = finite(items[items.length - 1]
      .precipitation_probability_percent)
    var representative = phaseRepresentative(items)
    var maximum = phaseProbability(items, true)
    var precipitation = precipitationKind(representative)
    if (isFinite(firstProbability) && isFinite(lastProbability)
        && firstProbability >= 35 && lastProbability <= firstProbability - 15)
      return phasePrecipitationTitle(precipitation, "easing")
    if (isFinite(firstProbability) && isFinite(lastProbability)
        && lastProbability >= firstProbability + 15)
      return phasePrecipitationTitle(precipitation, "building")
    if (precipitation === "storm") return "Storm window"
    if (isFinite(maximum) && maximum >= 60)
      return phasePrecipitationTitle(precipitation, "likely")
    if (isFinite(maximum) && maximum >= 30)
      return phasePrecipitationTitle(precipitation, "possible")
    return String(representative && representative.condition
      ? representative.condition : "Conditions steady")
  }

  function phaseRange(items, phaseIndex) {
    if (!items.length) return ""
    var start = phaseIndex === 0 ? "NOW"
      : controller.weatherHourlyTime(items[0])
    var end = controller.weatherHourlyTime(items[items.length - 1])
    return start + " → " + end
  }

  function phaseNote(items) {
    if (!items.length) return ""
    var startTemperature = controller.weatherTemperatureCompact(
      items[0].temperature_celsius)
    var endTemperature = controller.weatherTemperatureCompact(
      items[items.length - 1].temperature_celsius)
    var minimum = phaseProbability(items, false)
    var maximum = phaseProbability(items, true)
    var rain = isFinite(minimum) && isFinite(maximum)
      ? controller.weatherProbability(minimum) + " to "
        + controller.weatherProbability(maximum) : "rain unavailable"
    return startTemperature + " → " + endTemperature + " · " + rain
  }

  function buildPhases(source) {
    var values = source && source.length ? source.slice(0, 12) : []
    if (!values.length) return []
    var count = Math.min(3, values.length)
    var chunkSize = Math.ceil(values.length / count)
    var result = []
    for (var phaseIndex = 0; phaseIndex < count; phaseIndex++) {
      var start = phaseIndex * chunkSize
      var items = values.slice(start, Math.min(values.length, start + chunkSize))
      if (!items.length) continue
      result.push({
        range: phaseRange(items, phaseIndex),
        title: phaseTitle(items),
        note: phaseNote(items),
        representative: phaseRepresentative(items),
        points: items
      })
    }
    return result
  }

  function phaseColor(phase) {
    var representative = phase ? phase.representative : null
    var code = Number(representative ? representative.weather_code : 3)
    var probability = finite(representative
      ? representative.precipitation_probability_percent : null)
    if (code >= 95) return violet
    if (code >= 51 || (isFinite(probability) && probability >= 35)) return blue
    return amber
  }

  function dayPart(item) {
    var match = String(item && item.time ? item.time : "").match(/T(\d{2}):/)
    var hour = match ? Number(match[1]) : 12
    if (hour < 6) return "before dawn"
    if (hour < 11) return "morning"
    if (hour < 15) return "early afternoon"
    if (hour < 18) return "late afternoon"
    if (hour < 21) return "dusk"
    return "tonight"
  }

  function precipitationKind(item) {
    var code = Number(item ? item.weather_code : -1)
    if (code === 95 || code === 96 || code === 99) return "storm"
    if (code === 71 || code === 73 || code === 75 || code === 77
        || code === 85 || code === 86) return "snow"
    if (code === 56 || code === 57 || code === 66 || code === 67)
      return "ice"
    return "rain"
  }

  function precipitationBuildTitle(kind, peakItem, endItem) {
    var peakPart = dayPart(peakItem)
    var endPart = dayPart(endItem)
    if (kind === "storm")
      return "Storms build through " + peakPart
        + ", then soften toward " + endPart + "."
    if (kind === "snow")
      return "Snow builds through " + peakPart
        + ", then eases toward " + endPart + "."
    if (kind === "ice")
      return "Icy precipitation builds through " + peakPart
        + ", then eases toward " + endPart + "."
    return "Rain builds through " + peakPart
      + ", then eases toward " + endPart + "."
  }

  function precipitationPeakTitle(kind, item) {
    var part = dayPart(item)
    var timing = part === "tonight" ? "tonight" : "in the " + part
    if (kind === "storm") return "Storms are most likely " + timing + "."
    if (kind === "snow") return "Snow is most likely " + timing + "."
    if (kind === "ice")
      return "Icy precipitation is most likely " + timing + "."
    return "Rain is most likely " + timing + "."
  }

  function precipitationPassingTitle(kind, item) {
    var part = dayPart(item)
    var timing = part === "tonight" ? "tonight" : "in the " + part
    if (kind === "storm") return "A passing storm may arrive " + timing + "."
    if (kind === "snow") return "A spell of snow may arrive " + timing + "."
    if (kind === "ice")
      return "Icy precipitation may arrive " + timing + "."
    return "A passing shower may arrive " + timing + "."
  }

  function peakHourly(field, limit) {
    var count = Math.min(hourly.length, limit || hourly.length)
    var peak = null
    var peakValue = Number.NEGATIVE_INFINITY
    for (var index = 0; index < count; index++) {
      var value = finite(hourly[index] ? hourly[index][field] : null)
      if (isFinite(value) && value > peakValue) {
        peakValue = value
        peak = hourly[index]
      }
    }
    return { item: peak, value: peakValue, index: peak ? hourly.indexOf(peak) : -1 }
  }

  function clearWindowAfter(startIndex) {
    for (var index = Math.max(0, startIndex + 1); index < hourly.length; index++) {
      var probability = finite(hourly[index].precipitation_probability_percent)
      var code = Number(hourly[index].weather_code)
      if ((!isFinite(probability) || probability < 30) && code <= 3)
        return hourly[index]
    }
    return null
  }

  function narrativeTitle() {
    if (!weatherData) return "Current conditions are unavailable."
    var rainPeak = peakHourly("precipitation_probability_percent", 12)
    var currentProbability = hourly.length
      ? finite(hourly[0].precipitation_probability_percent) : NaN
    var lastProbability = hourly.length >= 12
      ? finite(hourly[11].precipitation_probability_percent) : NaN
    var precipitation = precipitationKind(rainPeak.item)
    if (isFinite(rainPeak.value) && rainPeak.value >= 70) {
      if (isFinite(currentProbability) && isFinite(lastProbability)
          && rainPeak.value >= currentProbability + 15
          && rainPeak.value >= lastProbability + 15)
        return precipitationBuildTitle(precipitation, rainPeak.item,
          hourly[Math.min(11, hourly.length - 1)])
      return precipitationPeakTitle(precipitation, rainPeak.item)
    }
    if (isFinite(rainPeak.value) && rainPeak.value >= 35)
      return precipitationPassingTitle(precipitation, rainPeak.item)
    var temperaturePeak = peakHourly("temperature_celsius", 12)
    if (isFinite(temperaturePeak.value) && temperaturePeak.value >= 32)
      return "Heat holds through " + dayPart(temperaturePeak.item)
        + ", then eases later."
    var code = Number(weatherData.weather_code)
    if (code <= 1) return "Clear skies hold for the next few hours."
    return String(weatherData.condition || "Conditions stay steady") + " for now."
  }

  function narrativeNote() {
    if (!weatherData) return ""
    var feels = finite(weatherData.apparent_temperature_celsius)
    var humidity = finite(weatherData.relative_humidity_percent)
    var lead = isFinite(feels) && feels >= 35 ? "It will feel hot and humid."
      : (isFinite(humidity) && humidity >= 75 ? "Humidity stays high."
        : "Conditions stay close to the current reading.")
    var rainPeak = peakHourly("precipitation_probability_percent", 12)
    var clearWindow = clearWindowAfter(rainPeak.index)
    return clearWindow
      ? lead + " The clearest window begins around "
        + controller.weatherLocalTime(clearWindow.time) + "."
      : lead
  }

  function uvLevel(value) {
    var uv = finite(value)
    if (!isFinite(uv)) return "Unavailable"
    return uv >= 11 ? "Extreme" : (uv >= 8 ? "Very high"
      : (uv >= 6 ? "High" : (uv >= 3 ? "Moderate" : "Low")))
  }

  function graphTitle() {
    if (selectedMetric === "temperature") {
      var temperaturePeak = peakHourly("temperature_celsius", hourly.length)
      return temperaturePeak.item
        ? "Warmest around " + controller.weatherLocalTime(temperaturePeak.item.time)
          + " · " + controller.weatherTemperature(temperaturePeak.value)
        : "Temperature trend"
    }
    if (selectedMetric === "precipitation") {
      var rainPeak = peakHourly("precipitation_probability_percent", hourly.length)
      return rainPeak.item
        ? "Rain peaks around " + controller.weatherLocalTime(rainPeak.item.time)
          + " · " + controller.weatherProbability(rainPeak.value)
        : "Precipitation outlook"
    }
    if (selectedMetric === "wind") {
      var windPeak = peakHourly("wind_speed_kmh", hourly.length)
      return windPeak.item
        ? "Wind peaks around " + controller.weatherLocalTime(windPeak.item.time)
          + " · " + controller.weatherWind(windPeak.value)
        : "Wind trend"
    }
    var uvPeak = peakHourly("uv_index", hourly.length)
    return uvPeak.item
      ? "UV reaches " + String(Math.round(uvPeak.value)) + " · "
        + uvLevel(uvPeak.value)
      : "UV index"
  }

  function precipitationTotal() {
    var total = 0
    var found = false
    for (var index = 0; index < hourly.length; index++) {
      var amount = finite(hourly[index].precipitation_mm)
      if (!isFinite(amount)) continue
      found = true
      total += amount
    }
    return found ? total : NaN
  }

  function graphNote() {
    if (selectedMetric === "precipitation") {
      var total = precipitationTotal()
      return isFinite(total) ? controller.weatherPrecipitation(total)
        + " expected across the next 24 hours" : "Probability over 24 hours"
    }
    if (selectedMetric === "wind")
      return weatherData ? controller.weatherWindDirection(
        weatherData.wind_direction_degrees) + " flow, gusts "
        + controller.weatherWind(weatherData.wind_gusts_kmh) : ""
    if (selectedMetric === "temperature")
      return "The full rise and fall, sampled across 24 hours"
    var uvPeak = peakHourly("uv_index", hourly.length)
    return uvPeak.item ? "Highest around "
      + controller.weatherLocalTime(uvPeak.item.time) : ""
  }

  function dailyMinimum() {
    var minimum = Number.POSITIVE_INFINITY
    for (var index = 0; index < forecast.length; index++) {
      var value = finite(forecast[index].temperature_min_celsius)
      if (isFinite(value)) minimum = Math.min(minimum, value)
    }
    return isFinite(minimum) ? Math.floor(minimum) : 0
  }

  function dailyMaximum() {
    var maximum = Number.NEGATIVE_INFINITY
    for (var index = 0; index < forecast.length; index++) {
      var value = finite(forecast[index].temperature_max_celsius)
      if (isFinite(value)) maximum = Math.max(maximum, value)
    }
    return isFinite(maximum) ? Math.ceil(maximum) : 1
  }

  function dailyPosition(value) {
    var minimum = dailyMinimum()
    var maximum = dailyMaximum()
    var number = finite(value)
    if (!isFinite(number) || maximum <= minimum) return 0
    return Math.max(0, Math.min(1, (number - minimum) / (maximum - minimum)))
  }

  function minutesFromTime(value) {
    var match = String(value || "").match(/T(\d{2}):(\d{2})/)
    return match ? Number(match[1]) * 60 + Number(match[2]) : NaN
  }

  function formattedMinutes(value) {
    if (!isFinite(value)) return "—"
    var rounded = Math.round(value)
    var hours = Math.floor(rounded / 60) % 24
    var minutes = rounded % 60
    var stamp = "2000-01-01T" + (hours < 10 ? "0" : "") + String(hours)
      + ":" + (minutes < 10 ? "0" : "") + String(minutes)
    return controller.weatherLocalTime(stamp)
  }

  function solarNoon() {
    var sunrise = minutesFromTime(today ? today.sunrise : null)
    var sunset = minutesFromTime(today ? today.sunset : null)
    return isFinite(sunrise) && isFinite(sunset)
      ? formattedMinutes((sunrise + sunset) / 2) : "—"
  }

  function sunProgress() {
    var sunrise = minutesFromTime(today ? today.sunrise : null)
    var sunset = minutesFromTime(today ? today.sunset : null)
    var current = finite(clockData ? clockData.local_minutes : null)
    if (!isFinite(sunrise) || !isFinite(sunset) || !isFinite(current)
        || sunset <= sunrise) return 0
    return Math.max(0, Math.min(1, (current - sunrise) / (sunset - sunrise)))
  }

  function humidityNote() {
    var humidity = finite(weatherData ? weatherData.relative_humidity_percent : null)
    if (!isFinite(humidity)) return "Unavailable"
    return humidity >= 75 ? "Very humid" : (humidity >= 55 ? "Warm and humid"
      : (humidity >= 30 ? "Comfortable" : "Dry"))
  }

  function visibilityNote() {
    var meters = finite(weatherData ? weatherData.visibility_meters : null)
    if (!isFinite(meters)) return "Unavailable"
    return meters >= 20000 ? "Excellent" : (meters >= 10000 ? "Good"
      : (meters >= 4000 ? "Moderate" : "Limited"))
  }

  function localWeekday() {
    var dateText = String(clockData && clockData.date ? clockData.date : "")
    var date = new Date(dateText + "T12:00:00")
    return !dateText || isNaN(date.getTime())
      ? controller.clockDayLabel(clockData)
      : Qt.formatDate(date, "dddd")
  }

  function compactWind(speedValue, directionValue, gustValue) {
    var speed = finite(speedValue)
    if (!isFinite(speed)) return "—"
    var displaySpeed = controller.weatherUseImperial ? speed * 0.621371 : speed
    var direction = controller.weatherWindDirection(directionValue)
    var result = (direction ? direction + " " : "")
      + String(Math.round(displaySpeed))
    var gust = finite(gustValue)
    if (isFinite(gust) && gust > speed + 1) {
      var displayGust = controller.weatherUseImperial ? gust * 0.621371 : gust
      result += "  ·  G" + String(Math.round(displayGust))
    }
    return result
  }

  function wheelDistance(event) {
    var pixelY = Number(event.pixelDelta.y)
    if (isFinite(pixelY) && pixelY !== 0) return pixelY
    var angleY = Number(event.angleDelta.y)
    return isFinite(angleY) ? angleY / 120 * Style.space(64) : 0
  }

  function handleScrollWheel(event) {
    var distance = wheelDistance(event)
    var maximum = Math.max(0, detailScroll.contentHeight - detailScroll.height)
    if (!isFinite(distance) || distance === 0 || maximum <= 0) {
      event.accepted = false
      return
    }
    detailScroll.cancelFlick()
    detailScroll.contentY = Math.max(0,
      Math.min(maximum, detailScroll.contentY - distance))
    event.accepted = true
  }

  function scrollByDirection(direction) {
    var delta = Number(direction)
    if (!isFinite(delta) || delta === 0) return
    var maximum = Math.max(0, detailScroll.contentHeight - detailScroll.height)
    detailScroll.cancelFlick()
    detailScroll.contentY = Math.max(0, Math.min(maximum,
      detailScroll.contentY + delta * Style.space(64)))
  }

  onDetailKeyChanged: {
    metricWasChosen = false
    chooseDefaultMetric()
    detailScroll.contentY = 0
  }
  onHourlyChanged: {
    if (!metricWasChosen || !metricAvailable(selectedMetric)) chooseDefaultMetric()
  }
  Component.onCompleted: chooseDefaultMetric()

  Item {
    id: header
    anchors.top: parent.top
    anchors.left: parent.left
    anchors.right: parent.right
    height: Style.space(48)

    Button {
      anchors.left: parent.left
      anchors.leftMargin: Style.space(20)
      anchors.verticalCenter: parent.verticalCenter
      iconText: "󰅁"
      foreground: detail.foreground
      background: "transparent"
      focusable: true
      horizontalPadding: Style.space(8)
      verticalPadding: Style.space(5)
      tooltipText: "Back to world clock"
      Accessible.name: tooltipText
      Accessible.role: Accessible.Button
      onClicked: detail.backRequested()
    }

    Text {
      textFormat: Text.PlainText
      anchors.centerIn: parent
      width: Math.max(0, parent.width - Style.space(320))
      horizontalAlignment: Text.AlignHCenter
      elide: Text.ElideRight
      text: detail.clockData ? String(detail.clockData.title
        || detail.clockData.label || "Weather") : "Weather"
      color: detail.foreground
      font.family: detail.controller.contentFontFamily
      font.pixelSize: Style.fontPx(1.5)
      font.bold: true
    }

    Button {
      anchors.right: parent.right
      anchors.rightMargin: Style.space(20)
      anchors.verticalCenter: parent.verticalCenter
      text: "OPEN-METEO  ·  " + (detail.controller.weatherError
        ? "UPDATE UNAVAILABLE" : "UPDATED NOW")
      foreground: Qt.darker(detail.foreground, 1.35)
      background: "transparent"
      fontFamily: detail.controller.contentFontFamily
      fontSize: Style.font.caption
      focusable: true
      horizontalPadding: Style.space(4)
      verticalPadding: Style.space(1)
      tooltipText: "Weather data by Open-Meteo"
      Accessible.name: tooltipText
      Accessible.role: Accessible.Button
      onClicked: detail.attributionRequested()
    }

  }

  Flickable {
    id: detailScroll
    anchors.top: header.bottom
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    contentWidth: width
    contentHeight: contentColumn.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    interactive: contentHeight > height
    pixelAligned: true
    maximumFlickVelocity: Style.space(5200)
    flickDeceleration: Style.space(3000)

    WheelHandler {
      target: null
      enabled: detailScroll.interactive
      acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
      orientation: Qt.Vertical
      onWheel: function(event) { detail.handleScrollWheel(event) }
    }

    QQC.ScrollBar.vertical: QQC.ScrollBar {
      policy: QQC.ScrollBar.AsNeeded
      width: Style.space(5)
    }

    Column {
      id: contentColumn
      width: detailScroll.width
      spacing: 0

      Item {
        id: hero
        width: parent.width
        height: Style.space(184)

        Item {
          id: currentReading
          anchors.left: parent.left
          anchors.leftMargin: detail.sectionInset
          anchors.top: parent.top
          anchors.bottom: parent.bottom
          width: Math.round((parent.width - detail.sectionInset * 2) * 0.42)

          Row {
            anchors.centerIn: parent
            spacing: Style.space(14)

            Text {
              textFormat: Text.PlainText
              anchors.verticalCenter: parent.verticalCenter
              text: detail.controller.weatherGlyph(detail.weatherData) || "—"
              color: detail.controller.weatherGlyphColor(detail.weatherData)
              font.family: detail.controller.contentFontFamily
              font.pixelSize: Style.space(51)
            }

            Column {
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(4)

              Row {
                spacing: Style.space(2)

                Text {
                  id: heroTemperature
                  textFormat: Text.PlainText
                  text: detail.weatherData
                    ? detail.controller.weatherTemperatureCompact(
                      detail.weatherData.temperature_celsius) : "—"
                  color: detail.foreground
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.space(56)
                  font.bold: true
                }

                Text {
                  textFormat: Text.PlainText
                  visible: detail.weatherData !== null
                  anchors.top: heroTemperature.top
                  anchors.topMargin: Style.space(8)
                  text: detail.controller.weatherUseImperial ? "F" : "C"
                  color: detail.foreground
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.body
                  font.bold: true
                }
              }

              Row {
                spacing: Style.space(8)

                Text {
                  textFormat: Text.PlainText
                  text: detail.weatherData
                    ? String(detail.weatherData.condition || "")
                    : "Weather unavailable"
                  color: detail.foreground
                  opacity: 0.82
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                }

                Text {
                  textFormat: Text.PlainText
                  text: detail.weatherData && detail.clockData
                    ? "·  " + String(detail.clockData.time || "")
                      + (detail.clockData.notation
                        ? "  ·  " + String(detail.clockData.notation) : "")
                    : ""
                  color: detail.muted
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.letterSpacing: 0.18
                }
              }
            }
          }
        }

        Rectangle {
          anchors.left: currentReading.right
          anchors.top: parent.top
          anchors.topMargin: 0
          anchors.bottom: parent.bottom
          anchors.bottomMargin: 0
          width: Style.spacing.hairline
          color: detail.foreground
          opacity: 0.10
        }

        Item {
          anchors.left: currentReading.right
          anchors.leftMargin: Style.space(36)
          anchors.right: parent.right
          anchors.rightMargin: detail.sectionInset
          anchors.top: parent.top
          anchors.topMargin: Style.space(27)
          anchors.bottom: parent.bottom
          anchors.bottomMargin: Style.space(23)

          Text {
            id: localNoteLabel
            textFormat: Text.PlainText
            anchors.left: parent.left
            anchors.top: parent.top
            text: "LOCAL NOTE  ·  " + detail.localWeekday().toUpperCase()
            color: detail.coral
            font.family: detail.controller.contentFontFamily
            font.pixelSize: Style.font.caption
            font.bold: true
            font.letterSpacing: 0.8
          }

          Text {
            id: narrativeHeadline
            textFormat: Text.PlainText
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: localNoteLabel.bottom
            anchors.topMargin: Style.space(10)
            text: detail.narrativeTitle()
            color: detail.foreground
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            font.family: detail.proseFontFamily
            font.pixelSize: Style.fontPx(1.8)
            font.bold: true
          }

          Text {
            id: narrativeBody
            textFormat: Text.PlainText
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: narrativeHeadline.bottom
            anchors.topMargin: Style.space(7)
            text: detail.narrativeNote()
            color: detail.muted
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            font.family: detail.proseFontFamily
            font.pixelSize: Style.font.bodySmall
          }

          Row {
            id: heroFacts
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Style.space(36)

            Repeater {
              model: [
                { label: "FEELS", value: detail.weatherData
                  ? detail.controller.weatherTemperatureOrDash(
                    detail.weatherData.apparent_temperature_celsius) : "—" },
                { label: "RAIN NEXT HOUR", value:
                  detail.controller.weatherNextHourProbability() },
                { label: "WIND", value: detail.weatherData
                  ? detail.compactWind(
                    detail.weatherData.wind_speed_kmh,
                    detail.weatherData.wind_direction_degrees,
                    detail.weatherData.wind_gusts_kmh) : "—" },
                { label: "SUNSET", value: detail.today
                  ? detail.controller.weatherLocalTime(detail.today.sunset) : "—" }
              ]

              Item {
                id: heroFact
                required property var modelData
                width: heroFacts.width / 4
                height: heroFacts.height

                Column {
                  spacing: Style.space(5)

                  Text {
                    textFormat: Text.PlainText
                    text: String(heroFact.modelData.label || "")
                    color: detail.dim
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.font.caption
                    font.letterSpacing: 0.65
                  }

                  Text {
                    textFormat: Text.PlainText
                    width: heroFact.width - Style.space(5)
                    text: String(heroFact.modelData.value || "—")
                    color: detail.foreground
                    elide: Text.ElideRight
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.font.bodySmall
                    font.bold: true
                  }
                }
              }
            }
          }
        }
      }

      Rectangle {
        x: detail.sectionInset
        width: parent.width - detail.sectionInset * 2
        height: Style.spacing.hairline
        color: detail.foreground
        opacity: 0.10
      }

      Item {
        id: phaseSection
        visible: detail.phases.length > 0
        width: parent.width
        height: visible ? Style.space(132) : 0

        Text {
          textFormat: Text.PlainText
          x: detail.sectionInset
          y: Style.space(18)
          text: "TODAY BY PHASE"
          color: detail.muted
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.75
        }

        Row {
          id: phaseRow
          x: detail.sectionInset
          y: Style.space(48)
          width: parent.width - detail.sectionInset * 2
          height: Style.space(68)

          Repeater {
            model: detail.phases

            Item {
              id: phaseCell
              required property var modelData
              required property int index
              width: phaseRow.width / Math.max(1, detail.phases.length)
              height: phaseRow.height

              Rectangle {
                visible: phaseCell.index > 0
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: Style.spacing.hairline
                color: detail.foreground
                opacity: 0.07
              }

              Item {
                anchors.fill: parent
                anchors.leftMargin: phaseCell.index === 0 ? 0 : Style.space(24)
                anchors.rightMargin: phaseCell.index === detail.phases.length - 1
                  ? 0 : Style.space(24)

                Text {
                  id: phaseRange
                  textFormat: Text.PlainText
                  anchors.left: parent.left
                  anchors.top: parent.top
                  text: String(phaseCell.modelData.range || "")
                  color: detail.dim
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.letterSpacing: 0.45
                }

                Row {
                  id: phaseTitle
                  anchors.left: parent.left
                  anchors.top: phaseRange.bottom
                  anchors.topMargin: Style.space(7)
                  spacing: Style.space(9)

                  Text {
                    textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    text: detail.controller.weatherGlyph(
                      phaseCell.modelData.representative)
                    color: detail.controller.weatherGlyphColor(
                      phaseCell.modelData.representative)
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.space(18)
                  }

                  Text {
                    textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    text: String(phaseCell.modelData.title || "")
                    color: detail.foreground
                    font.family: detail.proseFontFamily
                    font.pixelSize: Style.font.body
                    font.bold: true
                  }
                }

                Text {
                  id: phaseNote
                  textFormat: Text.PlainText
                  anchors.left: parent.left
                  anchors.top: phaseTitle.bottom
                  anchors.topMargin: Style.space(2)
                  text: String(phaseCell.modelData.note || "")
                  color: detail.muted
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                }

                Row {
                  id: phaseBars
                  anchors.left: parent.left
                  anchors.right: parent.right
                  anchors.bottom: parent.bottom
                  height: Style.space(14)
                  spacing: Style.space(2)

                  Repeater {
                    model: phaseCell.modelData.points || []

                    Rectangle {
                      id: phaseBar
                      required property var modelData
                      width: (phaseBars.width - phaseBars.spacing
                        * Math.max(0, (phaseCell.modelData.points || []).length - 1))
                        / Math.max(1, (phaseCell.modelData.points || []).length)
                      anchors.bottom: phaseBars.bottom
                      readonly property real probability: detail.finite(
                        phaseBar.modelData
                          ? phaseBar.modelData.precipitation_probability_percent
                          : null)
                      height: Math.max(Style.space(2), phaseBars.height
                        * (isFinite(probability) ? 0.18 + probability / 100 * 0.82
                          : 0.36))
                      color: detail.phaseColor(phaseCell.modelData)
                      opacity: 0.72
                    }
                  }
                }
              }
            }
          }
        }
      }

      Rectangle {
        visible: phaseSection.visible
        x: detail.sectionInset
        width: parent.width - detail.sectionInset * 2
        height: visible ? Style.spacing.hairline : 0
        color: detail.foreground
        opacity: 0.08
      }

      Item {
        id: outlookSection
        visible: detail.forecast.length > 0
        width: parent.width
        height: visible ? Style.space(244) : 0
        readonly property real contentLeft: detail.sectionInset
        readonly property real usableWidth: width - detail.sectionInset * 2
        readonly property real conditionX: Style.space(66)
        readonly property real rainX: Style.space(244)
        readonly property real rangeX: Style.space(306)
        readonly property real uvX: usableWidth - Style.space(82)

        Text {
          textFormat: Text.PlainText
          x: outlookSection.contentLeft
          y: Style.space(18)
          text: "4-DAY OUTLOOK"
          color: detail.muted
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.75
        }

        Text {
          textFormat: Text.PlainText
          x: outlookSection.contentLeft
          y: Style.space(18)
          width: outlookSection.usableWidth
          horizontalAlignment: Text.AlignRight
          text: "Shared temperature scale  ·  "
            + detail.controller.weatherTemperatureCompact(detail.dailyMinimum())
            + " to "
            + detail.controller.weatherTemperatureCompact(detail.dailyMaximum())
          color: detail.dim
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.fontPx(0.75)
        }

        Item {
          id: outlookHeader
          x: outlookSection.contentLeft
          y: Style.space(50)
          width: outlookSection.usableWidth
          height: Style.space(26)

          Repeater {
            model: [
              { label: "DAY", x: 0, width: Style.space(38) },
              { label: "CONDITIONS", x: outlookSection.conditionX,
                width: Style.space(150) },
              { label: "RAIN", x: outlookSection.rainX,
                width: Style.space(52) },
              { label: "LOW → HIGH", x: outlookSection.rangeX,
                width: outlookSection.uvX - outlookSection.rangeX - Style.space(12) },
              { label: "UV MAX", x: outlookSection.uvX,
                width: Style.space(82) }
            ]

            Text {
              required property var modelData
              textFormat: Text.PlainText
              x: modelData.x
              width: modelData.width
              text: modelData.label
              color: detail.dim
              font.family: detail.controller.contentFontFamily
              font.pixelSize: Style.fontPx(0.75)
              font.letterSpacing: 0.55
            }
          }

          Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Style.spacing.hairline
            color: detail.foreground
            opacity: 0.07
          }
        }

        Column {
          id: outlookRows
          x: outlookSection.contentLeft
          y: Style.space(76)
          width: outlookSection.usableWidth

          Repeater {
            model: detail.forecast

            Item {
              id: outlookDay
              required property var modelData
              width: outlookRows.width
              height: Style.space(42)
              Accessible.role: Accessible.StaticText
              Accessible.name: detail.controller.weatherForecastDay(modelData.date)
                + ", " + String(modelData.condition || "")
                + ", high " + detail.controller.weatherTemperature(
                  modelData.temperature_max_celsius)
                + ", low " + detail.controller.weatherTemperature(
                  modelData.temperature_min_celsius)
                + ", UV " + detail.controller.weatherUv(modelData.uv_index_max)

              Text {
                textFormat: Text.PlainText
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(34)
                text: detail.controller.weatherForecastDay(outlookDay.modelData.date)
                color: detail.foreground
                font.family: detail.controller.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                font.letterSpacing: 0.35
              }

              Text {
                textFormat: Text.PlainText
                x: Style.space(41)
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(22)
                horizontalAlignment: Text.AlignHCenter
                text: detail.controller.weatherForecastGlyph(outlookDay.modelData)
                color: detail.controller.weatherForecastGlyphColor(outlookDay.modelData)
                font.family: detail.controller.contentFontFamily
                font.pixelSize: Style.space(16)
              }

              Text {
                textFormat: Text.PlainText
                x: outlookSection.conditionX
                anchors.verticalCenter: parent.verticalCenter
                width: outlookSection.rainX - outlookSection.conditionX
                  - Style.space(12)
                text: String(outlookDay.modelData.condition || "")
                color: detail.muted
                elide: Text.ElideRight
                font.family: detail.proseFontFamily
                font.pixelSize: Style.font.bodySmall
              }

              Text {
                textFormat: Text.PlainText
                x: outlookSection.rainX
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(48)
                text: detail.controller.weatherProbability(
                  outlookDay.modelData.precipitation_probability_percent)
                color: detail.finite(outlookDay.modelData
                  .precipitation_probability_percent) >= 30 ? detail.blue : detail.muted
                font.family: detail.controller.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
              }

              Item {
                id: dayRange
                x: outlookSection.rangeX
                anchors.verticalCenter: parent.verticalCenter
                width: outlookSection.uvX - outlookSection.rangeX - Style.space(14)
                height: Style.space(18)
                readonly property real trackLeft: Style.space(27)
                readonly property real trackRight: width - Style.space(29)
                readonly property real lowPosition: detail.dailyPosition(
                  outlookDay.modelData.temperature_min_celsius)
                readonly property real highPosition: detail.dailyPosition(
                  outlookDay.modelData.temperature_max_celsius)

                Text {
                  textFormat: Text.PlainText
                  anchors.left: parent.left
                  anchors.verticalCenter: parent.verticalCenter
                  text: detail.controller.weatherTemperatureCompact(
                    outlookDay.modelData.temperature_min_celsius)
                  color: detail.muted
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                }

                Rectangle {
                  id: fullRangeTrack
                  x: dayRange.trackLeft
                  anchors.verticalCenter: parent.verticalCenter
                  width: Math.max(0, dayRange.trackRight - dayRange.trackLeft)
                  height: Style.spacing.hairline
                  color: detail.foreground
                  opacity: 0.12
                }

                Rectangle {
                  x: fullRangeTrack.x + dayRange.lowPosition * fullRangeTrack.width
                  anchors.verticalCenter: parent.verticalCenter
                  width: Math.max(Style.spacing.hairline,
                    (dayRange.highPosition - dayRange.lowPosition)
                      * fullRangeTrack.width)
                  height: Style.space(2)
                  color: detail.amber
                  opacity: 0.88
                }

                Rectangle {
                  x: fullRangeTrack.x + dayRange.highPosition * fullRangeTrack.width
                    - width / 2
                  anchors.verticalCenter: parent.verticalCenter
                  width: Style.space(5)
                  height: width
                  radius: width / 2
                  color: Color.popups.background
                  border.width: Style.spacing.hairline
                  border.color: detail.amber
                }

                Text {
                  textFormat: Text.PlainText
                  anchors.right: parent.right
                  anchors.verticalCenter: parent.verticalCenter
                  text: detail.controller.weatherTemperatureCompact(
                    outlookDay.modelData.temperature_max_celsius)
                  color: detail.foreground
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.bold: true
                }
              }

              Item {
                x: outlookSection.uvX
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(82)
                height: Style.space(28)

                Row {
                  anchors.left: parent.left
                  anchors.top: parent.top
                  spacing: Style.space(5)

                  Text {
                    textFormat: Text.PlainText
                    text: "UV"
                    color: detail.muted
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.fontPx(0.75)
                  }

                  Text {
                    textFormat: Text.PlainText
                    text: isFinite(detail.finite(outlookDay.modelData.uv_index_max))
                      ? String(Math.round(outlookDay.modelData.uv_index_max)) : "—"
                    color: detail.coral
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.font.bodySmall
                    font.bold: true
                  }
                }

                Text {
                  textFormat: Text.PlainText
                  anchors.left: parent.left
                  anchors.bottom: parent.bottom
                  text: detail.uvLevel(outlookDay.modelData.uv_index_max)
                  color: detail.dim
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.fontPx(0.75)
                }
              }

              Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: Style.spacing.hairline
                color: detail.foreground
                opacity: 0.07
              }
            }
          }
        }
      }

      Rectangle {
        visible: outlookSection.visible
        x: detail.sectionInset
        width: parent.width - detail.sectionInset * 2
        height: visible ? Style.spacing.hairline : 0
        color: detail.foreground
        opacity: 0.09
      }

      Item {
        id: graphSection
        visible: detail.graphHours.length > 0
        width: parent.width
        height: visible ? Style.space(248) : 0

        Text {
          textFormat: Text.PlainText
          x: detail.sectionInset
          y: Style.space(18)
          text: "24-HOUR DETAIL"
          color: detail.muted
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.75
        }

        Row {
          id: metricButtons
          anchors.right: parent.right
          anchors.rightMargin: detail.sectionInset
          y: Style.space(11)
          spacing: Style.space(2)

          Repeater {
            model: detail.metricOptions

            Button {
              id: metricButton
              required property var modelData
              text: String(modelData.label || "")
              selected: detail.selectedMetric === modelData.key
              bordered: selected
              foreground: selected ? detail.foreground : detail.dim
              background: selected
                ? Style.normalFillFor(detail.foreground, Color.accent)
                : "transparent"
              fontFamily: detail.controller.contentFontFamily
              fontSize: Style.font.caption
              focusable: true
              horizontalPadding: Style.space(8)
              verticalPadding: Style.space(4)
              tooltipText: "Show " + String(modelData.label || "").toLowerCase()
                + " graph"
              Accessible.name: tooltipText
              Accessible.role: Accessible.Button
              onClicked: {
                detail.selectedMetric = String(modelData.key || "temperature")
                detail.metricWasChosen = true
              }
            }
          }
        }

        Text {
          textFormat: Text.PlainText
          x: detail.sectionInset
          y: Style.space(56)
          width: parent.width * 0.58
          text: detail.graphTitle()
          color: detail.foreground
          elide: Text.ElideRight
          font.family: detail.proseFontFamily
          font.pixelSize: Style.font.subtitle
          font.bold: true
        }

        Text {
          textFormat: Text.PlainText
          anchors.right: parent.right
          anchors.rightMargin: detail.sectionInset
          y: Style.space(59)
          width: parent.width * 0.37
          horizontalAlignment: Text.AlignRight
          text: detail.graphNote()
          color: detail.muted
          elide: Text.ElideRight
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.font.caption
        }

        WeatherMetricGraph {
          anchors.left: parent.left
          anchors.leftMargin: detail.sectionInset
          anchors.right: parent.right
          anchors.rightMargin: detail.sectionInset
          y: Style.space(86)
          controller: detail.controller
          hours: detail.graphHours
          metric: detail.selectedMetric
          foreground: detail.foreground
          temperatureColor: detail.amber
          precipitationColor: detail.blue
          uvColor: detail.coral
        }
      }

      Rectangle {
        visible: graphSection.visible
        x: detail.sectionInset
        width: parent.width - detail.sectionInset * 2
        height: visible ? Style.spacing.hairline : 0
        color: detail.foreground
        opacity: 0.09
      }

      Item {
        id: environmentSection
        width: parent.width
        height: Style.space(160)

        Text {
          textFormat: Text.PlainText
          x: detail.sectionInset
          y: Style.space(18)
          text: "SUN AND ATMOSPHERE"
          color: detail.muted
          font.family: detail.controller.contentFontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.75
        }

        Item {
          id: sunArea
          x: detail.sectionInset
          y: Style.space(51)
          width: Math.round((parent.width - detail.sectionInset * 2) * 0.52)
          height: Style.space(91)

          Canvas {
            id: sunCanvas
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: Style.space(54)
            property real progress: detail.sunProgress()
            property color lineColor: detail.foreground
            property color progressColor: detail.amber

            onProgressChanged: requestPaint()
            onLineColorChanged: requestPaint()
            onProgressColorChanged: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            Component.onCompleted: requestPaint()

            onPaint: {
              var context = getContext("2d")
              context.clearRect(0, 0, width, height)
              var left = Style.space(8)
              var right = width - Style.space(8)
              var baseline = height - Style.space(4)
              var controlX = width / 2
              var controlY = -Style.space(8)

              context.lineWidth = Style.spacing.hairline
              context.strokeStyle = "rgba(" + String(Math.round(lineColor.r * 255))
                + "," + String(Math.round(lineColor.g * 255)) + ","
                + String(Math.round(lineColor.b * 255)) + ",0.12)"
              context.beginPath()
              context.moveTo(left, baseline)
              context.lineTo(right, baseline)
              context.stroke()
              context.beginPath()
              context.moveTo(left, baseline)
              context.quadraticCurveTo(controlX, controlY, right, baseline)
              context.stroke()

              var clamped = Math.max(0, Math.min(1, progress))
              context.lineWidth = Style.space(1.25)
              context.strokeStyle = "rgba(" + String(Math.round(progressColor.r * 255))
                + "," + String(Math.round(progressColor.g * 255)) + ","
                + String(Math.round(progressColor.b * 255)) + ",0.92)"
              context.beginPath()
              for (var index = 0; index <= 40; index++) {
                var t = clamped * index / 40
                var inverse = 1 - t
                var x = inverse * inverse * left + 2 * inverse * t * controlX
                  + t * t * right
                var y = inverse * inverse * baseline + 2 * inverse * t * controlY
                  + t * t * baseline
                if (index === 0) context.moveTo(x, y)
                else context.lineTo(x, y)
              }
              context.stroke()

              var inverseProgress = 1 - clamped
              var dotX = inverseProgress * inverseProgress * left
                + 2 * inverseProgress * clamped * controlX
                + clamped * clamped * right
              var dotY = inverseProgress * inverseProgress * baseline
                + 2 * inverseProgress * clamped * controlY
                + clamped * clamped * baseline
              context.beginPath()
              context.arc(dotX, dotY, Style.space(3), 0, Math.PI * 2)
              context.fillStyle = String(Color.popups.background)
              context.fill()
              context.lineWidth = Style.space(1.2)
              context.strokeStyle = String(progressColor)
              context.stroke()
            }
          }

          Row {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Style.space(30)

            Repeater {
              model: [
                { label: "Sunrise", value: detail.today
                  ? detail.controller.weatherLocalTime(detail.today.sunrise) : "—" },
                { label: "Solar noon", value: detail.solarNoon() },
                { label: "Sunset", value: detail.today
                  ? detail.controller.weatherLocalTime(detail.today.sunset) : "—" }
              ]

              Item {
                id: sunTime
                required property var modelData
                required property int index
                width: parent.width / 3
                height: parent.height

                Column {
                  x: sunTime.index === 0 ? 0
                    : (sunTime.index === 2 ? parent.width - width
                      : (parent.width - width) / 2)
                  width: Style.space(70)
                  spacing: Style.space(3)

                  Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    horizontalAlignment: sunTime.index === 0 ? Text.AlignLeft
                      : (sunTime.index === 2 ? Text.AlignRight : Text.AlignHCenter)
                    text: String(sunTime.modelData.label || "")
                    color: detail.muted
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.font.caption
                  }

                  Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    horizontalAlignment: sunTime.index === 0 ? Text.AlignLeft
                      : (sunTime.index === 2 ? Text.AlignRight : Text.AlignHCenter)
                    text: String(sunTime.modelData.value || "—")
                    color: detail.foreground
                    font.family: detail.controller.contentFontFamily
                    font.pixelSize: Style.font.bodySmall
                    font.bold: true
                  }
                }
              }
            }
          }
        }

        Row {
          id: atmosphereRow
          anchors.left: sunArea.right
          anchors.leftMargin: Style.space(30)
          anchors.right: parent.right
          anchors.rightMargin: detail.sectionInset
          y: Style.space(57)
          height: Style.space(76)

          Repeater {
            model: [
              { label: "HUMIDITY", value: detail.weatherData
                ? detail.controller.weatherHumidity(
                  detail.weatherData.relative_humidity_percent) : "—",
                note: detail.humidityNote() },
              { label: "PRESSURE", value: detail.weatherData
                ? detail.controller.weatherPressure(detail.weatherData.pressure_hpa) : "—",
                note: "Sea-level" },
              { label: "VISIBILITY", value: detail.weatherData
                ? detail.controller.weatherVisibility(
                  detail.weatherData.visibility_meters) : "—",
                note: detail.visibilityNote() }
            ]

            Item {
              id: atmosphereCell
              required property var modelData
              required property int index
              width: atmosphereRow.width / 3
              height: atmosphereRow.height

              Rectangle {
                visible: atmosphereCell.index > 0
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: Style.spacing.hairline
                color: detail.foreground
                opacity: 0.07
              }

              Column {
                anchors.left: parent.left
                anchors.leftMargin: atmosphereCell.index === 0 ? 0 : Style.space(16)
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.space(7)

                Text {
                  textFormat: Text.PlainText
                  text: String(atmosphereCell.modelData.label || "")
                  color: detail.dim
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.letterSpacing: 0.55
                }

                Text {
                  textFormat: Text.PlainText
                  text: String(atmosphereCell.modelData.value || "—")
                  color: detail.foreground
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                }

                Text {
                  textFormat: Text.PlainText
                  text: String(atmosphereCell.modelData.note || "")
                  color: detail.dim
                  font.family: detail.controller.contentFontFamily
                  font.pixelSize: Style.fontPx(0.75)
                }
              }
            }
          }
        }
      }
    }
  }
}
