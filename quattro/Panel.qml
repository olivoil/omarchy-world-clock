pragma ComponentBehavior: Bound

import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui

// Quattro-native world-clock panel. Rust remains the timezone/config engine;
// this already-loaded QML surface owns the interaction and visual lifecycle.
Panel {
  id: root
  moduleName: "io.github.olivoil.world-clock"
  ipcTarget: moduleName
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  property string backendCommand:
    String(Qt.resolvedUrl("../bin/omarchy-world-clock-backend")).replace(/^file:\/\//, "")
  property var snapshot: ({
    schema_version: 1,
    reference_utc: "",
    local_timezone: "",
    time_format: "24h",
    weather_unit: "",
    configured_count: 0,
    local_configured: false,
    pinned_timezone: null,
    summary: ({ timezone: "", label: "", title: "", time: "--:--", day: "", notation: "", relative_minutes: 0, relative_label: "Same time" }),
    clocks: [],
    timeline: []
  })
  property bool snapshotLoaded: false
  property bool summaryFocusPending: false
  property string mode: "read"
  property bool live: true
  property bool editorActive: false
  property bool timeEditorActive: false
  property bool editorRefreshPending: false
  property string editorRefreshReference: ""
  property bool snapshotRequestPending: false
  property string snapshotRequestReference: ""
  property int snapshotStateGeneration: 0
  property int snapshotActiveGeneration: -1
  property string snapshotActiveReference: ""
  property int convertActiveGeneration: -1
  property string convertActiveSource: ""
  property string invalidConversionSource: ""
  property string statusText: ""
  property bool statusError: false
  property string actionName: ""
  property string searchQueryInFlight: ""
  property string searchResultsQuery: ""
  property string searchSubmitQuery: ""
  property var searchResults: []
  property var mapSelection: null
  property real mapRequestedLatitude: 0
  property real mapRequestedLongitude: 0
  property real mapLookupLatitude: 0
  property real mapLookupLongitude: 0
  property real mapCursorX: 0
  property real mapCursorY: 0
  property bool mapClickPending: false
  property var weather: ({
    source: "Open-Meteo",
    attribution_url: "https://open-meteo.com/",
    disabled: false,
    locations: []
  })
  property bool weatherError: false
  property bool weatherRequestPending: false
  property string weatherLoadedSignature: ""
  property string weatherActiveSignature: ""
  property string weatherAttemptSignature: ""
  property double weatherLastUpdatedAt: 0
  property double weatherLastAttemptAt: 0

  readonly property var barIdentity: hostWidget || root
  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property var clocks: snapshot && Array.isArray(snapshot.clocks) ? snapshot.clocks : []
  readonly property var timeline: snapshot && Array.isArray(snapshot.timeline) ? snapshot.timeline : []
  readonly property var summary: snapshot && snapshot.summary ? snapshot.summary : ({ time: "--:--", title: "", timezone: "", day: "", notation: "" })
  readonly property var weatherLocations: weather && Array.isArray(weather.locations)
    ? weather.locations : []
  readonly property bool weatherLoading: weatherProcess.running
  readonly property string weatherUnitOverride:
    String(snapshot.weather_unit || "").trim().toLowerCase()
  readonly property bool weatherUseImperial: weatherUnitOverride === "imperial"
    || (weatherUnitOverride !== "metric"
      && root.localeUsesImperial(Qt.locale().name))
  readonly property bool weatherPresentationActive: root.live
    && weather.disabled !== true
    && (root.weatherLoading || root.weatherLocations.length > 0)
  readonly property int weatherRefreshMilliseconds: 15 * 60 * 1000
  readonly property string currentLocationTitle: {
    var title = String(summary.title || summary.label || "").trim()
    return title || "World Clock"
  }
  readonly property string currentTimezoneMetadata: {
    var timezone = String(summary.timezone || "").trim()
    var notation = String(summary.notation || "").trim().toUpperCase()
    if (timezone && notation) return timezone + "  ·  " + notation
    return timezone || notation
  }
  readonly property int maxClocks: 9
  readonly property bool canRemove: Number(snapshot.configured_count || 0) > 1
  readonly property bool localTimezoneConfigured: snapshot.local_configured === true
  readonly property int nonLocalLocationCount: Math.max(0,
    Number(snapshot.configured_count || 0) - (localTimezoneConfigured ? 1 : 0))
  readonly property bool canAdd: nonLocalLocationCount < maxClocks
    || nonLocalLocationCount === maxClocks && !localTimezoneConfigured
  readonly property var mapClocks: {
    var entries = []
    if (root.hasMapCoordinate(summary)) entries.push(summary)
    for (var i = 0; i < clocks.length; i++)
      if (root.hasMapCoordinate(clocks[i])) entries.push(clocks[i])
    return entries
  }
  readonly property int timelineExtent: {
    var extent = 60
    for (var i = 0; i < timeline.length; i++)
      extent = Math.max(extent, Math.abs(Number(timeline[i].relative_minutes || 0)))
    return Math.ceil(extent / 60) * 60 + 60
  }

  function open() {
    controller.show()
  }

  function focusSummaryEditor() {
    if (mode !== "read") {
      summaryFocusPending = false
      return
    }
    if (!snapshotLoaded) {
      summaryFocusPending = true
      return
    }
    summaryFocusPending = false
    // Return is emitted while PanelKeyCatcher is still dispatching the key.
    // Hand focus over on the next event-loop turn so the catcher cannot take
    // it straight back, then select the live time for immediate replacement.
    Qt.callLater(function() {
      if (!opened || mode !== "read") return
      summaryInput.forceActiveFocus(Qt.ShortcutFocusReason)
      summaryInput.selectAll()
    })
  }

  function openEditor() {
    mode = "read"
    var alreadyOpened = opened
    controller.show()
    if (alreadyOpened) refresh()
    // Opening KeyboardPanel also schedules its focus target. Queue this one
    // turn later so editing wins that initial focus handoff as well.
    Qt.callLater(root.focusSummaryEditor)
  }

  function focusAddField() {
    Qt.callLater(function() {
      if (!opened || mode !== "add" || !canAdd) return
      addField.forceActiveFocus(Qt.ShortcutFocusReason)
      addField.selectAll()
    })
  }

  function openAdd() {
    mode = "add"
    var alreadyOpened = opened
    controller.show()
    if (alreadyOpened) refresh()
    focusAddField()
  }

  function close() {
    mode = "read"
    summaryFocusPending = false
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    mapSelection = null
    mapClickPending = false
    controller.hide()
  }

  function toggle() {
    if (opened) close()
    else open()
  }

  function switchPanel(direction) {
    if (bar && typeof bar.switchPanelFrom === "function")
      return bar.switchPanelFrom(barIdentity, direction)
    return false
  }

  function clearStatus() {
    statusText = ""
    statusError = false
  }

  function setStatus(text, error) {
    statusText = String(text || "")
    statusError = error === true
  }

  function mixColor(base, tint, amount) {
    var ratio = Math.max(0, Math.min(1, Number(amount)))
    return Qt.rgba(
      base.r * (1 - ratio) + tint.r * ratio,
      base.g * (1 - ratio) + tint.g * ratio,
      base.b * (1 - ratio) + tint.b * ratio,
      1)
  }

  function conversionSource(clock) {
    if (!clock) return ""
    var label = clock.label !== null && clock.label !== undefined
      ? clock.label : clock.title
    return String(clock.timezone || "") + "\u001f" + String(label || "")
  }

  function localeUsesImperial(localeName) {
    var name = String(localeName || "").replace(".", "_")
    return /^en[_-]US($|[_.-])/.test(name)
      || /^en[_-]LR($|[_.-])/.test(name)
      || /^my($|[_.-])/.test(name)
  }

  function weatherSignature() {
    if (!snapshotLoaded) return ""
    var entries = [summary].concat(clocks)
    var signatures = []
    for (var i = 0; i < entries.length; i++) {
      var entry = entries[i]
      signatures.push(conversionSource(entry)
        + "\u001f" + String(entry.latitude)
        + "\u001f" + String(entry.longitude))
    }
    signatures.sort()
    return signatures.join("\u001e")
  }

  function weatherFor(clock) {
    var key = conversionSource(clock)
    for (var i = 0; i < weatherLocations.length; i++) {
      if (conversionSource(weatherLocations[i]) === key) return weatherLocations[i]
    }
    return null
  }

  function weatherTemperature(value) {
    var celsius = Number(value)
    if (!isFinite(celsius)) return ""
    var temperature = weatherUseImperial ? celsius * 9 / 5 + 32 : celsius
    return String(Math.round(temperature)) + "°" + (weatherUseImperial ? "F" : "C")
  }

  function weatherTemperatureCompact(value) {
    return weatherTemperature(value).replace(/[FC]$/, "")
  }

  // Match the weather-icons glyph vocabulary already used by Omarchy's
  // native weather panel, including day/night-aware clear and cloud icons.
  function weatherGlyph(item) {
    if (!item) return ""
    var code = Number(item.weather_code)
    var night = item.is_day === false
    if (code === 0) return night ? "" : ""
    if (code === 1 || code === 2) return night ? "" : ""
    if (code === 3) return ""
    if (code === 45 || code === 48) return night ? "" : ""
    if (code === 51 || code === 53 || code === 55 || code === 56
        || code === 57 || code === 61) return ""
    if (code === 63 || code === 65 || code === 66 || code === 67
        || code === 80 || code === 81 || code === 82) return ""
    if (code === 71 || code === 73 || code === 75 || code === 77
        || code === 85 || code === 86) return ""
    if (code === 95 || code === 96 || code === 99) return ""
    return ""
  }

  function weatherText(item) {
    if (!item) return "WEATHER UNAVAILABLE"
    return weatherTemperature(item.temperature_celsius)
  }

  function requestWeather(force) {
    if (!opened || !snapshotLoaded || !live) return
    var signature = weatherSignature()
    if (!signature) return
    var fresh = !weatherError && signature === weatherLoadedSignature
      && Date.now() - weatherLastUpdatedAt < weatherRefreshMilliseconds
    var recentlyAttempted = signature === weatherAttemptSignature
      && Date.now() - weatherLastAttemptAt < 2 * 60 * 1000
    if (force !== true && (fresh || recentlyAttempted)) return
    if (weatherProcess.running) {
      weatherRequestPending = true
      return
    }
    weatherRequestPending = false
    weatherActiveSignature = signature
    weatherAttemptSignature = signature
    weatherLastAttemptAt = Date.now()
    weatherProcess.command = [backendCommand, "weather"]
    weatherProcess.running = true
  }

  function flushWeatherRequest() {
    if (!weatherRequestPending || weatherProcess.running) return
    weatherRequestPending = false
    requestWeather(false)
  }

  function clearConversionError(source) {
    if (invalidConversionSource !== String(source || "")) return
    invalidConversionSource = ""
    clearStatus()
  }

  function timeInputEdited(source) {
    clearConversionError(source)
    if (convertProcess.running) convertActiveGeneration = -1
  }

  function notifyHost() {
    if (hostWidget && typeof hostWidget.broadcast === "function")
      hostWidget.broadcast("refresh")
  }

  function applySnapshot(raw, manual) {
    try {
      var payload = JSON.parse(String(raw || ""))
      if (!payload || Number(payload.schema_version) !== 1 || !payload.summary)
        throw new Error("Unsupported snapshot")
      snapshot = payload
      snapshotLoaded = true
      invalidConversionSource = ""
      summaryInput.text = String(payload.summary.time || "--:--")
      if (manual !== true && live) live = true
      if (clocks.length === 0 && mode !== "add" && !summaryFocusPending)
        mode = "add"
      clearStatus()
      if (summaryFocusPending) Qt.callLater(root.focusSummaryEditor)
      requestWeather(false)
    } catch (error) {
      setStatus("World Clock backend returned invalid data.", true)
    }
  }

  function requestSnapshot(referenceUtc) {
    var reference = String(referenceUtc || "")
    if (timeEditorActive) {
      snapshotRequestPending = false
      snapshotRequestReference = ""
      editorRefreshPending = true
      editorRefreshReference = reference
      return
    }
    if (snapshotProcess.running) {
      if (snapshotRequestPending && snapshotRequestReference === reference) return
      if (!snapshotRequestPending
          && snapshotActiveGeneration === snapshotStateGeneration
          && snapshotActiveReference === reference) return
      snapshotRequestPending = true
      snapshotRequestReference = reference
      return
    }
    snapshotRequestPending = false
    snapshotRequestReference = ""
    snapshotActiveGeneration = snapshotStateGeneration
    snapshotActiveReference = reference
    var command = [backendCommand, "snapshot"]
    if (reference) command.push("--at", reference)
    snapshotProcess.command = command
    snapshotProcess.running = true
  }

  function flushSnapshotRequest() {
    if (!snapshotRequestPending || snapshotProcess.running) return
    var reference = snapshotRequestReference
    snapshotRequestPending = false
    snapshotRequestReference = ""
    requestSnapshot(reference)
  }

  function invalidateSnapshotRequests() {
    snapshotStateGeneration += 1
    snapshotRequestPending = false
    snapshotRequestReference = ""
    editorRefreshPending = false
    editorRefreshReference = ""
  }

  function requestLiveSnapshot() {
    if (!live) {
      return
    }
    requestSnapshot("")
  }

  function flushEditorRefresh() {
    if (!editorRefreshPending) return
    if (timeEditorActive) return
    var reference = editorRefreshReference
    editorRefreshPending = false
    editorRefreshReference = ""
    requestSnapshot(reference)
  }

  function refresh() {
    if (live) requestLiveSnapshot()
    else requestSnapshot(snapshot ? String(snapshot.reference_utc || "") : "")
  }

  function returnToLive() {
    invalidateSnapshotRequests()
    invalidConversionSource = ""
    live = true
    requestLiveSnapshot()
    requestWeather(false)
  }

  function convertFrom(timezone, value, source) {
    var text = String(value || "").trim()
    var timezoneName = String(timezone || "").trim()
    var reference = snapshot ? String(snapshot.reference_utc || "").trim() : ""
    if (!snapshotLoaded || !timezoneName || !reference
        || !text || convertProcess.running) return
    invalidateSnapshotRequests()
    convertActiveGeneration = snapshotStateGeneration
    convertActiveSource = String(source || "")
    convertProcess.command = [
      backendCommand,
      "convert",
      "--timezone", timezoneName,
      "--value", text,
      "--base", reference
    ]
    convertProcess.running = true
  }

  function runAction(name, timezone, result) {
    if (actionProcess.running) return
    if (name === "add" && !canAddLocation(timezone)) {
      setStatus("Only " + currentLocationTitle
        + " can be added at the nine-location limit.", true)
      return
    }
    actionName = name
    var command = [backendCommand, name]
    if (name !== "unpin") command.push(String(timezone || ""))
    if ((name === "add" || name === "pin" || name === "remove") && result) {
      var actionLabel = result.label !== null && result.label !== undefined
        ? result.label : result.title
      command.push("--label", String(actionLabel || ""))
      if (name === "add" && result.latitude !== null && result.latitude !== undefined
          && result.longitude !== null && result.longitude !== undefined) {
        command.push("--latitude", String(result.latitude))
        command.push("--longitude", String(result.longitude))
      }
    }
    actionProcess.command = command
    actionProcess.running = true
  }

  function togglePin(clock) {
    runAction(clock.pinned ? "unpin" : "pin", clock.timezone, clock)
  }

  function removeClock(clock) {
    if (canRemove) runAction("remove", clock.timezone, clock)
  }

  function canAddLocation(timezone) {
    if (nonLocalLocationCount < maxClocks) return true
    if (nonLocalLocationCount > maxClocks) return false
    var candidate = String(timezone || "").trim()
    var localTimezone = String(snapshot.local_timezone || "").trim()
    return !localTimezoneConfigured && candidate !== "" && candidate === localTimezone
  }

  function scheduleSearch() {
    searchDebounce.restart()
  }

  function searchTextChanged() {
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    scheduleSearch()
  }

  function startSearch() {
    if (mode !== "add") return
    var query = String(addField.text || "").trim()
    if (!query) {
      searchResults = []
      searchResultsQuery = ""
      return
    }
    if (searchProcess.running) return
    searchQueryInFlight = query
    searchProcess.command = [backendCommand, "search", query]
    searchProcess.running = true
  }

  function addFirstResult() {
    var query = String(addField.text || "").trim()
    if (mode !== "add" || !query || !canAdd || actionProcess.running) return
    if (searchResultsQuery === query) {
      if (searchResults.length > 0)
        runAction("add", searchResults[0].timezone, searchResults[0])
      return
    }
    searchSubmitQuery = query
    searchDebounce.stop()
    startSearch()
  }

  function hasMapCoordinate(entry) {
    if (!entry || entry.latitude === null || entry.latitude === undefined
        || entry.longitude === null || entry.longitude === undefined) return false
    var latitude = Number(entry.latitude)
    var longitude = Number(entry.longitude)
    return isFinite(latitude) && isFinite(longitude)
      && latitude >= -90 && latitude <= 90
      && longitude >= -180 && longitude <= 180
  }

  function mapX(longitude, width) {
    return Math.max(0, Math.min(width, ((Number(longitude) + 180) / 360) * width))
  }

  function mapY(latitude, height) {
    return Math.max(0, Math.min(height, ((90 - Number(latitude)) / 180) * height))
  }

  function compactMapLabel(value) {
    var label = String(value || "")
    return label.length <= 18 ? label : label.slice(0, 15) + "…"
  }

  function escapeHtml(value) {
    return String(value || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
  }

  function mapLabelWidth(label) {
    return Math.max(Style.space(42), Math.min(Style.space(148),
      String(label || "").length * Style.spaceReal(7.2) + Style.space(16)))
  }

  function mapRectsOverlap(left, right) {
    var gap = Style.space(4)
    return left.x - gap < right.x + right.width
      && left.x + left.width + gap > right.x
      && left.y - gap < right.y + right.height
      && left.y + left.height + gap > right.y
  }

  function mapLabelLayout(targetIndex, width, height) {
    var placed = []
    var labelHeight = Style.space(22)
    var margin = Style.space(7)
    var labelGap = Style.space(8)
    var rowGap = labelHeight + Style.space(5)
    var offsets = [0, -rowGap, rowGap, -rowGap * 2, rowGap * 2, -rowGap * 3, rowGap * 3]

    for (var i = 0; i <= targetIndex && i < mapClocks.length; i++) {
      var entry = mapClocks[i]
      var pointX = mapX(entry.longitude, width)
      var pointY = mapY(entry.latitude, height)
      var label = compactMapLabel(entry.title)
      var labelWidth = mapLabelWidth(label)
      var preferredRight = pointX < width / 2
      var sides = preferredRight
        ? [pointX + labelGap, pointX - labelGap - labelWidth]
        : [pointX - labelGap - labelWidth, pointX + labelGap]
      var chosen = null

      for (var offsetIndex = 0; offsetIndex < offsets.length && !chosen; offsetIndex++) {
        for (var sideIndex = 0; sideIndex < sides.length && !chosen; sideIndex++) {
          var candidate = {
            x: Math.max(margin, Math.min(width - labelWidth - margin, sides[sideIndex])),
            y: Math.max(margin, Math.min(height - labelHeight - margin,
              pointY - labelHeight / 2 + offsets[offsetIndex])),
            width: labelWidth,
            height: labelHeight,
            pointX: pointX,
            pointY: pointY,
            label: label
          }
          var overlaps = false
          for (var placedIndex = 0; placedIndex < placed.length; placedIndex++) {
            if (mapRectsOverlap(candidate, placed[placedIndex])) {
              overlaps = true
              break
            }
          }
          if (!overlaps) chosen = candidate
        }
      }

      if (!chosen) {
        chosen = {
          x: Math.max(margin, Math.min(width - labelWidth - margin, sides[0])),
          y: Math.max(margin, Math.min(height - labelHeight - margin, pointY - labelHeight / 2)),
          width: labelWidth,
          height: labelHeight,
          pointX: pointX,
          pointY: pointY,
          label: label
        }
      }
      placed.push(chosen)
    }

    return placed.length > targetIndex ? placed[targetIndex]
      : ({ x: 0, y: 0, width: 0, height: 0, pointX: 0, pointY: 0, label: "" })
  }

  function requestMapLocation(x, y, width, height) {
    if (!canAdd || width <= 0 || height <= 0) return
    mapCursorX = x
    mapCursorY = y
    mapRequestedLongitude = Math.max(-179.999999, Math.min(179.999999, x / width * 360 - 180))
    mapRequestedLatitude = Math.max(-89.999999, Math.min(89.999999, 90 - y / height * 180))
    mapSelection = null
    mapClickPending = true
    if (!mapProcess.running) startMapLookup()
  }

  function startMapLookup() {
    if (!mapClickPending || mapProcess.running) return
    mapLookupLatitude = mapRequestedLatitude
    mapLookupLongitude = mapRequestedLongitude
    var command = [
      backendCommand, "locate",
      "--latitude", String(mapLookupLatitude),
      "--longitude", String(mapLookupLongitude)
    ]
    if (snapshot && snapshot.reference_utc)
      command.push("--at", String(snapshot.reference_utc))
    mapProcess.command = command
    mapProcess.running = true
    setStatus("Locating timezone…", false)
  }

  function timelinePosition(relativeMinutes, width) {
    var railInset = Style.space(36)
    var usable = Math.max(1, width - railInset * 2)
    var normalized = (Number(relativeMinutes || 0) + timelineExtent) / (timelineExtent * 2)
    return railInset + Math.max(0, Math.min(1, normalized)) * usable
  }

  function timelineX(relativeMinutes, width, itemWidth) {
    var center = timelinePosition(relativeMinutes, width)
    return Math.max(0, Math.min(width - itemWidth, center - itemWidth / 2))
  }

  onOpenedChanged: if (opened) {
    refresh()
    requestWeather(false)
  }
  onTimeEditorActiveChanged: {
    if (!timeEditorActive && editorRefreshPending)
      Qt.callLater(root.flushEditorRefresh)
  }
  onModeChanged: {
    if (mode === "add") {
      focusAddField()
    } else {
      searchDebounce.stop()
      searchResults = []
      searchResultsQuery = ""
      searchSubmitQuery = ""
      mapSelection = null
      mapClickPending = false
    }
  }

  Process {
    id: snapshotProcess
    stdout: StdioCollector { id: snapshotOutput; waitForEnd: true }
    stderr: StdioCollector { id: snapshotError; waitForEnd: true }
    onExited: function(exitCode) {
      var current = root.snapshotActiveGeneration === root.snapshotStateGeneration
      var manual = root.snapshotActiveReference !== ""
      if (exitCode === 0 && current && root.timeEditorActive) {
        if (!root.editorRefreshPending) {
          root.editorRefreshPending = true
          root.editorRefreshReference = root.snapshotActiveReference
        }
      } else if (exitCode === 0 && current) {
        root.applySnapshot(snapshotOutput.text, manual)
      } else if (exitCode !== 0 && current)
        root.setStatus("The bundled World Clock backend could not produce a snapshot. Reinstall or update the plugin.", true)
      root.snapshotActiveReference = ""
      Qt.callLater(root.flushSnapshotRequest)
      Qt.callLater(root.flushEditorRefresh)
    }
  }

  Process {
    id: weatherProcess
    stdout: StdioCollector { id: weatherOutput; waitForEnd: true }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function(exitCode) {
      var signature = root.weatherActiveSignature
      root.weatherActiveSignature = ""
      if (signature !== root.weatherSignature()) {
        root.weatherRequestPending = true
      } else if (exitCode !== 0) {
        root.weatherError = true
      } else {
        try {
          var payload = JSON.parse(String(weatherOutput.text || ""))
          if (!payload || !Array.isArray(payload.locations))
            throw new Error("Unsupported weather response")
          root.weather = payload
          root.weatherError = false
          root.weatherLoadedSignature = signature
          root.weatherLastUpdatedAt = Date.now()
        } catch (error) {
          root.weatherError = true
        }
      }
      Qt.callLater(root.flushWeatherRequest)
    }
  }

  Process {
    id: convertProcess
    stdout: StdioCollector { id: convertOutput; waitForEnd: true }
    stderr: StdioCollector { id: convertError; waitForEnd: true }
    onExited: function(exitCode) {
      var current = root.convertActiveGeneration === root.snapshotStateGeneration
      var source = root.convertActiveSource
      root.convertActiveGeneration = -1
      root.convertActiveSource = ""
      if (!current) return
      if (exitCode !== 0) {
        root.invalidConversionSource = source
        root.setStatus(String(convertError.text || "Use HH:MM, 830, 8.5, 3pm, or YYYY-MM-DD HH:MM.").trim(), true)
        return
      }
      try {
        var payload = JSON.parse(String(convertOutput.text || ""))
        root.invalidateSnapshotRequests()
        root.invalidConversionSource = ""
        root.live = false
        root.applySnapshot(JSON.stringify(payload.snapshot), true)
      } catch (error) {
        root.setStatus("Could not convert that time.", true)
      }
    }
  }

  Process {
    id: actionProcess
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { id: actionError; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode !== 0) {
        root.setStatus(String(actionError.text || "Could not update World Clock.").trim(), true)
        return
      }
      if (root.actionName === "add") {
        addField.text = ""
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
        root.mapSelection = null
        root.mapClickPending = false
        root.setStatus("Location added.", false)
      }
      root.invalidateSnapshotRequests()
      root.requestSnapshot(root.live ? "" : String(root.snapshot.reference_utc || ""))
      root.notifyHost()
    }
  }

  Process {
    id: searchProcess
    stdout: StdioCollector { id: searchOutput; waitForEnd: true }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function(exitCode) {
      if (root.mode !== "add") {
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
        return
      }
      var currentQuery = String(addField.text || "").trim()
      if (root.searchQueryInFlight !== currentQuery) {
        if (root.mode === "add" && root.searchSubmitQuery === currentQuery)
          Qt.callLater(root.startSearch)
        else if (root.mode === "add")
          root.scheduleSearch()
        return
      }
      if (exitCode !== 0) {
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
        root.setStatus("Search is unavailable; exact timezone names still work.", true)
        return
      }
      try {
        var payload = JSON.parse(String(searchOutput.text || "[]"))
        root.searchResults = Array.isArray(payload) ? payload : []
        root.searchResultsQuery = root.searchQueryInFlight
        if (root.searchResults.length === 0) {
          root.searchSubmitQuery = ""
          root.setStatus("No matching location.", false)
        } else {
          root.clearStatus()
          if (root.mode === "add"
              && root.searchSubmitQuery === root.searchResultsQuery && root.canAdd) {
            root.searchSubmitQuery = ""
            root.runAction("add", root.searchResults[0].timezone, root.searchResults[0])
          }
        }
      } catch (error) {
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
      }
    }
  }

  Process {
    id: mapProcess
    stdout: StdioCollector { id: mapOutput; waitForEnd: true }
    stderr: StdioCollector { id: mapError; waitForEnd: true }
    onExited: function(exitCode) {
      if (root.mapLookupLatitude !== root.mapRequestedLatitude
          || root.mapLookupLongitude !== root.mapRequestedLongitude) {
        Qt.callLater(root.startMapLookup)
        return
      }
      if (exitCode !== 0) {
        root.mapClickPending = false
        root.setStatus(String(mapError.text || "Could not resolve that map region.").trim(), true)
        return
      }
      try {
        var payload = JSON.parse(String(mapOutput.text || "null"))
        if (!payload || !payload.timezone) {
          root.mapClickPending = false
          root.mapSelection = null
          root.setStatus("Choose a land region with a named timezone.", false)
          return
        }
        root.mapSelection = payload
        if (root.mapClickPending) {
          root.mapClickPending = false
          root.runAction("add", payload.timezone, payload)
        }
      } catch (error) {
        root.mapClickPending = false
        root.mapSelection = null
        root.setStatus("Could not read that map region.", true)
      }
    }
  }

  Timer {
    id: searchDebounce
    interval: 180
    onTriggered: root.startSearch()
  }

  Timer {
    interval: root.weatherRefreshMilliseconds
    running: root.opened && root.live && root.weather.disabled !== true
    repeat: true
    onTriggered: root.requestWeather(true)
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: true
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(960))
    contentHeight: panel.fittedContentHeight(panelColumn.implicitHeight, Style.space(680))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.editorActive || addField.activeFocus
      onCloseRequested: {
        if (root.mode === "read") root.close()
        else root.mode = "read"
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onReturnRequested: {
        if (root.mode === "read") root.focusSummaryEditor()
        else if (root.mode === "add") root.focusAddField()
      }
      onTextKey: function(text) {
        if (text === "a" || text === "A") root.mode = "add"
        else if (text === "e" || text === "E") root.mode = root.mode === "edit" ? "read" : "edit"
        else if (text === "r" || text === "R") root.returnToLive()
      }

      Flickable {
        id: panelScroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: panelColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height

        Column {
          id: panelColumn
          width: panelScroll.width
          spacing: Style.space(14)

          Item {
            width: parent.width
            height: Math.max(headerTitle.implicitHeight, headerStart.implicitHeight, headerActions.implicitHeight)

            Row {
              id: headerStart
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(6)

              Button {
                iconText: root.mode === "add" ? "󰅁" : "󰑐"
                active: root.mode !== "add" && !root.live
                tooltipText: root.mode === "add" ? "Back to world clock" : "Return to live time"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: {
                  if (root.mode === "add") root.mode = "read"
                  else root.returnToLive()
                }
              }

              Button {
                visible: root.summary.pinned === true
                text: "UNPIN"
                selected: true
                tooltipText: "Remove this time from the bar"
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(4)
                onClicked: root.togglePin(root.summary)
              }
            }

            Text {
              id: headerTitle
              anchors.centerIn: parent
              text: root.mode === "add" ? "Add a Location" : root.currentLocationTitle
              color: root.contentForeground
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.title
              font.bold: true
            }

            Row {
              id: headerActions
              visible: root.mode !== "add"
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(6)

              Button {
                iconText: "󰐕"
                enabled: root.canAdd
                tooltipText: root.canAdd ? "Add a location" : "Nine locations already shown"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.mode = "add"
              }

              Button {
                iconText: "󰏫"
                active: root.mode === "edit"
                tooltipText: root.mode === "edit" ? "Finish editing" : "Pin or remove locations"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.mode = root.mode === "edit" ? "read" : "edit"
              }
            }
          }

          Column {
            id: readPage
            visible: root.mode !== "add"
            width: parent.width
            spacing: Style.space(14)

            Item {
              id: summaryClock
              readonly property var weatherData: root.weatherFor(root.summary)
              width: parent.width
              height: Style.space(92)

              TextInput {
                id: summaryInput
                readonly property string conversionSource: root.conversionSource(root.summary)
                readonly property bool conversionInvalid:
                  root.invalidConversionSource === conversionSource
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.max(implicitWidth, Style.space(230))
                horizontalAlignment: Text.AlignHCenter
                text: root.summary.time || "--:--"
                color: conversionInvalid ? Color.urgent : root.contentForeground
                selectionColor: Style.selectionFill
                selectedTextColor: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.space(52)
                font.bold: true
                selectByMouse: true
                enabled: root.mode === "read" && root.snapshotLoaded
                readOnly: convertProcess.running
                onAccepted: root.convertFrom(root.summary.timezone, text, conversionSource)
                onTextEdited: root.timeInputEdited(conversionSource)
                onActiveFocusChanged: {
                  root.editorActive = activeFocus
                  root.timeEditorActive = activeFocus
                  if (!activeFocus && !conversionInvalid && text !== root.summary.time)
                    text = root.summary.time
                }
              }

              Rectangle {
                visible: summaryInput.activeFocus || summaryInput.conversionInvalid
                anchors.left: summaryInput.left
                anchors.right: summaryInput.right
                anchors.top: summaryInput.bottom
                height: Style.spacing.hairline
                color: summaryInput.conversionInvalid
                  ? Color.urgent
                  : Style.focusStateColor(root.contentForeground, Color.accent)
              }

              Row {
                id: summaryMetadataLine
                anchors.top: summaryInput.bottom
                anchors.topMargin: Style.space(7)
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Style.space(8)

                Text {
                  id: summaryMetadata
                  anchors.verticalCenter: parent.verticalCenter
                  text: root.currentTimezoneMetadata
                  color: Qt.darker(root.contentForeground, 1.45)
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                  font.letterSpacing: 1.1
                }

                Text {
                  visible: summaryWeatherLine.visible
                  anchors.verticalCenter: parent.verticalCenter
                  text: "·"
                  color: Qt.darker(root.contentForeground, 1.45)
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                }

                Item {
                  id: summaryWeatherLine
                  visible: root.weatherPresentationActive
                  width: Math.max(summaryWeatherText.implicitWidth,
                    summaryWeatherSkeleton.implicitWidth)
                  height: Style.space(16)

                  Text {
                    id: summaryWeatherText
                    visible: summaryClock.weatherData || !root.weatherLoading
                    anchors.centerIn: parent
                    text: root.weatherGlyph(summaryClock.weatherData)
                      + (summaryClock.weatherData ? "  " : "")
                      + root.weatherText(summaryClock.weatherData)
                    color: Qt.darker(root.contentForeground, 1.45)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.bodySmall
                    font.letterSpacing: 0.3
                  }

                  Row {
                    id: summaryWeatherSkeleton
                    visible: !summaryClock.weatherData && root.weatherLoading
                    anchors.centerIn: parent
                    spacing: Style.space(7)

                    Rectangle {
                      anchors.verticalCenter: parent.verticalCenter
                      width: Style.space(12)
                      height: Style.space(8)
                      radius: height / 2
                      color: root.contentForeground
                      opacity: 0.12
                    }

                    Rectangle {
                      anchors.verticalCenter: parent.verticalCenter
                      width: Style.space(30)
                      height: Style.space(6)
                      radius: height / 2
                      color: root.contentForeground
                      opacity: 0.12
                    }
                  }
                }
              }
            }

            Item {
              id: timelineView
              visible: root.timeline.length > 1
              width: parent.width
              height: visible ? Style.space(120) : 0
              readonly property real railY: Math.round(height / 2)

              Rectangle {
                x: Style.space(36)
                width: parent.width - Style.space(72)
                y: timelineView.railY
                height: Style.spacing.hairline
                color: root.contentForeground
                opacity: 0.20
              }

              Repeater {
                id: timelineTickRepeater
                model: Math.floor(root.timelineExtent * 2 / 60) + 1

                Rectangle {
                  required property int index
                  readonly property bool major: index % 3 === 0
                  x: root.timelinePosition(-root.timelineExtent + index * 60,
                    timelineView.width) - width / 2
                  y: timelineView.railY - height / 2
                  width: Style.spacing.hairline
                  height: major ? Style.space(10) : Style.space(6)
                  color: root.contentForeground
                  opacity: major ? 0.22 : 0.12
                }
              }

              Repeater {
                model: root.timeline

                Item {
                  id: timelinePoint
                  required property var modelData
                  readonly property bool localPoint:
                    Number(modelData.relative_minutes || 0) === 0
                  readonly property bool upperLane: Number(modelData.lane || 0) === 0
                  width: Style.space(96)
                  height: parent.height
                  x: root.timelineX(modelData.relative_minutes, timelineView.width, width)

                  Rectangle {
                    id: markerStem
                    x: Math.round((parent.width - width) / 2)
                    y: timelinePoint.upperLane
                      ? markerHalo.y - height : markerHalo.y + markerHalo.height
                    width: Style.spacing.hairline
                    height: Style.space(9)
                    color: root.contentForeground
                    opacity: 0.24
                  }

                  Rectangle {
                    id: markerHalo
                    x: Math.round((parent.width - width) / 2)
                    y: timelineView.railY - height / 2
                    width: Style.space(timelinePoint.localPoint
                      ? 11 : (Number(timelinePoint.modelData.count || 1) > 1 ? 9 : 7))
                    height: width
                    radius: width / 2
                    color: timelinePoint.localPoint
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : "transparent"
                    border.width: timelinePoint.localPoint
                      || Number(timelinePoint.modelData.count || 1) > 1
                      ? Style.spacing.hairline : 0
                    border.color: timelinePoint.localPoint
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : root.contentForeground

                    Rectangle {
                      anchors.centerIn: parent
                      width: Style.space(5)
                      height: width
                      radius: width / 2
                      color: timelinePoint.localPoint
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                      opacity: timelinePoint.localPoint ? 1 : 0.82
                    }
                  }

                  Column {
                    id: timelineLabel
                    width: parent.width
                    spacing: Style.space(2)
                    y: timelinePoint.upperLane
                      ? markerStem.y - height - Style.space(4)
                      : markerStem.y + markerStem.height + Style.space(4)

                    Text {
                      width: parent.width
                      horizontalAlignment: Text.AlignHCenter
                      text: timelinePoint.modelData.time
                      color: timelinePoint.localPoint
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.bodySmall
                      font.bold: timelinePoint.localPoint
                    }

                    Text {
                      width: parent.width
                      horizontalAlignment: Text.AlignHCenter
                      elide: Text.ElideRight
                      text: String(timelinePoint.modelData.label || "").toUpperCase()
                      color: Qt.darker(root.contentForeground, 1.5)
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.caption
                      font.bold: true
                      font.letterSpacing: 0.8
                    }
                  }
                }
              }
            }

            Column {
              id: clockRows
              anchors.horizontalCenter: parent.horizontalCenter
              width: parent.width - Style.space(36)
              spacing: Style.space(14)

              Repeater {
                model: Math.ceil(root.clocks.length / 3)

                Row {
                  id: clockRow
                  required property int index
                  readonly property int startIndex: index * 3
                  readonly property int itemCount:
                    Math.min(3, root.clocks.length - startIndex)
                  readonly property real cellWidth:
                    (clockRows.width - Style.space(32)) / 3
                  anchors.horizontalCenter: parent.horizontalCenter
                  width: itemCount * cellWidth
                    + Math.max(0, itemCount - 1) * spacing
                  height: Style.space(root.mode === "edit" ? 110 : 100)
                  spacing: Style.space(16)

                  Repeater {
                    model: clockRow.itemCount

                    Item {
                      id: clockCell
                      required property int index
                      readonly property var clockData:
                        root.clocks[clockRow.startIndex + index]
                      readonly property var weatherData: root.weatherFor(clockData)
                      readonly property bool showWeather: root.weatherPresentationActive
                        && (weatherData !== null || root.weatherLoading)
                      width: clockRow.cellWidth
                      height: clockRow.height
                      clip: true

                      Rectangle {
                        id: clockSurface
                        anchors.fill: parent
                        radius: Style.cornerRadius
                        color: Style.normalFillFor(root.contentForeground, Color.accent)
                      }

                      Column {
                        id: cardContent
                        anchors.fill: parent
                        anchors.leftMargin: Style.space(10)
                        anchors.rightMargin: Style.space(10)
                        anchors.topMargin: Style.space(7)
                        anchors.bottomMargin: Style.space(7)
                        spacing: Style.space(4)

                        Item {
                          width: parent.width
                          height: Math.max(cardTitle.implicitHeight, cardControls.implicitHeight)

                          Text {
                            id: cardTitle
                            anchors.left: parent.left
                            anchors.right: cardControls.visible
                              ? cardControls.left : cardNotation.left
                            anchors.rightMargin: Style.space(6)
                            anchors.verticalCenter: parent.verticalCenter
                            text: String(clockCell.clockData.title || "").toUpperCase()
                            color: root.contentForeground
                            font.family: root.contentFontFamily
                            font.pixelSize: Style.font.caption
                            font.bold: true
                            font.letterSpacing: 1
                            elide: Text.ElideRight
                          }

                          Text {
                            id: cardNotation
                            visible: !cardControls.visible
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: String(clockCell.clockData.notation || "").toUpperCase()
                            color: Qt.darker(root.contentForeground, 1.5)
                            font.family: root.contentFontFamily
                            font.pixelSize: Style.font.caption
                            font.letterSpacing: 0.7
                          }

                          Row {
                            id: cardControls
                            visible: root.mode === "edit"
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: Style.space(2)

                            Button {
                              text: clockCell.clockData.pinned ? "UNPIN" : "PIN"
                              selected: clockCell.clockData.pinned === true
                              tooltipText: clockCell.clockData.pinned
                                ? "Remove this time from the bar"
                                : "Keep this time visible in the bar"
                              fontSize: Style.font.caption
                              horizontalPadding: Style.space(5)
                              verticalPadding: Style.space(3)
                              onClicked: root.togglePin(clockCell.clockData)
                            }

                            PanelActionButton {
                              iconText: "󰆴"
                              enabled: root.canRemove
                              tooltipText: root.canRemove
                                ? "Remove location" : "Keep at least one timezone"
                              foreground: root.contentForeground
                              hoverColor: Color.urgent
                              fontFamily: root.contentFontFamily
                              onClicked: root.removeClock(clockCell.clockData)
                            }
                          }
                        }

                        TextInput {
                          id: cardTimeInput
                          readonly property string conversionSource:
                            root.conversionSource(clockCell.clockData)
                          readonly property bool conversionInvalid:
                            root.invalidConversionSource === conversionSource
                          width: parent.width
                          text: clockCell.clockData.time
                          color: conversionInvalid ? Color.urgent : root.contentForeground
                          selectionColor: Style.selectionFill
                          selectedTextColor: root.contentForeground
                          font.family: root.contentFontFamily
                          font.pixelSize: Style.font.displayLarge
                          font.bold: true
                          selectByMouse: true
                          enabled: root.mode === "read" && root.snapshotLoaded
                          readOnly: convertProcess.running
                          onAccepted: root.convertFrom(
                            clockCell.clockData.timezone, text, conversionSource)
                          onTextEdited: root.timeInputEdited(conversionSource)
                          onActiveFocusChanged: {
                            root.editorActive = activeFocus
                            root.timeEditorActive = activeFocus
                            if (!activeFocus && !conversionInvalid
                                && text !== clockCell.clockData.time)
                              text = clockCell.clockData.time
                          }

                          Rectangle {
                            visible: cardTimeInput.conversionInvalid
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            height: Style.spacing.hairline
                            color: Color.urgent
                          }
                        }

                        Item {
                          id: cardMetadataRow
                          width: parent.width
                          height: Math.max(cardRelativeMetadata.implicitHeight,
                            cardWeatherBlock.implicitHeight)

                          Text {
                            id: cardRelativeMetadata
                            anchors.left: parent.left
                            anchors.right: cardWeatherBlock.left
                            anchors.rightMargin: cardWeatherBlock.visible
                              ? Style.space(8) : 0
                            anchors.verticalCenter: parent.verticalCenter
                            text: String(clockCell.clockData.day || "").toUpperCase()
                              + "  ·  "
                              + String(clockCell.clockData.relative_label || "").toUpperCase()
                            color: Qt.darker(root.contentForeground, 1.5)
                            font.family: root.contentFontFamily
                            font.pixelSize: Style.font.caption
                            font.letterSpacing: 0.6
                            elide: Text.ElideRight
                          }

                          Item {
                            id: cardWeatherBlock
                            visible: clockCell.showWeather
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            implicitWidth: Style.space(64)
                            implicitHeight: Style.space(14)
                            width: visible ? implicitWidth : 0
                            height: implicitHeight

                            Text {
                              visible: clockCell.weatherData !== null
                              anchors.right: parent.right
                              anchors.verticalCenter: parent.verticalCenter
                              text: clockCell.weatherData === null ? ""
                                : root.weatherGlyph(clockCell.weatherData)
                                  + "  "
                                  + root.weatherTemperatureCompact(
                                    clockCell.weatherData.temperature_celsius)
                              color: Qt.darker(root.contentForeground, 1.5)
                              font.family: root.contentFontFamily
                              font.pixelSize: Style.font.caption
                              font.letterSpacing: 0.2
                            }

                            Rectangle {
                              visible: clockCell.weatherData === null
                                && root.weatherLoading
                              anchors.right: parent.right
                              anchors.verticalCenter: parent.verticalCenter
                              width: Style.space(42)
                              height: Style.space(6)
                              radius: height / 2
                              color: root.contentForeground
                              opacity: 0.09
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }

            Item {
              id: weatherAttributionSlot
              width: parent.width
              height: openMeteoAttribution.implicitHeight

              Button {
                id: openMeteoAttribution
                visible: root.live && root.weather.disabled !== true
                anchors.left: parent.left
                anchors.leftMargin: Style.space(12)
                anchors.verticalCenter: parent.verticalCenter
                text: root.weatherLocations.length > 0 || !root.weatherError
                  ? "Weather data by Open-Meteo"
                    + (root.weatherError ? "  ·  Update unavailable" : "")
                  : "Weather unavailable"
                tooltipText: "Open Open-Meteo"
                foreground: Qt.darker(root.contentForeground, 1.35)
                background: root.mixColor(Color.popups.background,
                  root.contentForeground, 0.025)
                fontFamily: root.contentFontFamily
                fontSize: Style.font.caption
                focusable: true
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(3)
                onClicked: Qt.openUrlExternally("https://open-meteo.com/")
              }
            }

            Button {
              visible: root.clocks.length === 0
              anchors.horizontalCenter: parent.horizontalCenter
              text: "Add a location"
              iconText: "󰐕"
              bordered: true
              onClicked: root.mode = "add"
            }
          }

          Column {
            id: addPage
            visible: root.mode === "add"
            width: parent.width
            spacing: Style.space(12)

            TextField {
              id: addField
              anchors.horizontalCenter: parent.horizontalCenter
              width: Math.min(parent.width, Style.space(760))
              placeholderText: "Search for a city or timezone"
              foreground: root.contentForeground
              enabled: root.canAdd && !actionProcess.running
              onTextChanged: root.searchTextChanged()
              onAccepted: root.addFirstResult()
              onActiveFocusChanged: root.editorActive = activeFocus
            }

            Text {
              visible: !root.canAdd
              width: parent.width
              horizontalAlignment: Text.AlignHCenter
              text: "Remove a location before adding another."
              color: Qt.darker(root.contentForeground, 1.45)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.body
            }

            Item {
              id: mapCanvas
              visible: root.canAdd
              anchors.horizontalCenter: parent.horizontalCenter
              width: Math.min(parent.width, Style.space(720))
              height: width / 2
              clip: true

              Rectangle {
                anchors.fill: parent
                color: Style.normalFillFor(root.contentForeground, Color.accent)
              }

              Rectangle {
                id: searchResultOverlay
                visible: root.searchResults.length > 0
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: Math.min(root.searchResults.length * Style.space(48), mapCanvas.height)
                z: 3
                color: Color.background
                clip: true

                ListView {
                  id: searchResultList
                  anchors.fill: parent
                  model: root.searchResults
                  boundsBehavior: Flickable.StopAtBounds
                  interactive: contentHeight > height
                  clip: true

                  delegate: Button {
                    id: resultButton
                    required property var modelData
                    width: searchResultList.width
                    height: Style.space(48)
                    enabled: root.canAddLocation(resultButton.modelData.timezone)
                      && !actionProcess.running
                    leftAlign: true
                    text: ""
                    onClicked: root.runAction("add", resultButton.modelData.timezone, resultButton.modelData)

                    Column {
                      anchors.left: parent.left
                      anchors.leftMargin: Style.spacing.rowPaddingX
                      anchors.right: parent.right
                      anchors.rightMargin: Style.spacing.rowPaddingX
                      anchors.verticalCenter: parent.verticalCenter
                      spacing: Style.space(2)

                      Text {
                        width: parent.width
                        text: resultButton.modelData.title
                        color: root.contentForeground
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.body
                        font.bold: true
                        elide: Text.ElideRight
                      }

                      Text {
                        width: parent.width
                        textFormat: Text.RichText
                        text: root.escapeHtml(resultButton.modelData.subtitle)
                          + (resultButton.modelData.open_meteo_attribution
                            ? "  ·  <a href=\"https://open-meteo.com/\">Open-Meteo</a>" : "")
                        color: Qt.darker(root.contentForeground, 1.5)
                        linkColor: root.contentForeground
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.caption
                        elide: Text.ElideRight
                        onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                      }
                    }
                  }
                }
              }

              Image {
                anchors.fill: parent
                source: Qt.resolvedUrl("../assets/world-map.png")
                fillMode: Image.Stretch
                smooth: true
                mipmap: true
                opacity: 0.78
              }

              Repeater {
                model: 11

                Rectangle {
                  required property int index
                  x: Math.round((index + 1) * mapCanvas.width / 12)
                  y: Style.space(8)
                  width: Style.spacing.hairline
                  height: mapCanvas.height - Style.space(16)
                  color: root.contentForeground
                  opacity: 0.10
                }
              }

              Repeater {
                model: root.mapClocks

                Item {
                  id: mapMarker
                  required property int index
                  required property var modelData
                  readonly property var layout:
                    root.mapLabelLayout(index, mapCanvas.width, mapCanvas.height)
                  anchors.fill: parent

                  Rectangle {
                    x: mapMarker.layout.x
                    y: mapMarker.layout.y
                    width: mapMarker.layout.width
                    height: mapMarker.layout.height
                    radius: height / 2
                    color: Color.background
                    opacity: 0.88

                    Text {
                      anchors.fill: parent
                      anchors.leftMargin: Style.space(7)
                      anchors.rightMargin: Style.space(7)
                      verticalAlignment: Text.AlignVCenter
                      horizontalAlignment: Text.AlignHCenter
                      text: mapMarker.layout.label
                      color: root.contentForeground
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.caption
                      font.bold: true
                      elide: Text.ElideRight
                    }
                  }

                  Rectangle {
                    x: mapMarker.layout.pointX - width / 2
                    y: mapMarker.layout.pointY - height / 2
                    width: Style.space(11)
                    height: width
                    radius: width / 2
                    color: Style.selectedFillFor(root.contentForeground, Color.accent)

                    Rectangle {
                      anchors.centerIn: parent
                      width: Style.space(5)
                      height: width
                      radius: width / 2
                      color: Style.selectedStateColor(root.contentForeground, Color.accent)
                    }
                  }
                }
              }

              Rectangle {
                visible: mapProcess.running
                  || root.mapSelection !== null
                    && actionProcess.running && root.actionName === "add"
                x: Math.max(Style.space(6), Math.min(parent.width - width - Style.space(6),
                  root.mapCursorX + Style.space(12)))
                y: Math.max(Style.space(6), Math.min(parent.height - height - Style.space(6),
                  root.mapCursorY + Style.space(12)))
                width: mapLookupLabel.implicitWidth + Style.space(16)
                height: mapLookupLabel.implicitHeight + Style.space(10)
                radius: height / 2
                color: Color.background
                opacity: 0.92

                Text {
                  id: mapLookupLabel
                  anchors.centerIn: parent
                  text: mapProcess.running ? "Locating…" : "Adding…"
                  color: root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                }
              }

              MouseArea {
                anchors.fill: parent
                enabled: root.canAdd && !actionProcess.running
                hoverEnabled: true
                cursorShape: enabled ? Qt.CrossCursor : Qt.ArrowCursor
                onClicked: function(mouse) {
                  root.requestMapLocation(mouse.x, mouse.y, width, height)
                }
              }
            }

            Row {
              visible: mapCanvas.visible
              anchors.horizontalCenter: parent.horizontalCenter
              width: mapCanvas.width

              Repeater {
                model: ["−12", "−8", "−4", "0", "+4", "+8", "+12"]

                Item {
                  id: legendCell
                  required property string modelData
                  width: mapCanvas.width / 7
                  height: zoneLegend.implicitHeight

                  Text {
                    id: zoneLegend
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: legendCell.modelData
                    color: Qt.darker(root.contentForeground, 1.55)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                  }
                }
              }
            }

            Text {
              visible: root.canAdd
              anchors.horizontalCenter: parent.horizontalCenter
              width: mapCanvas.width
              text: "Search by city or timezone, or click a land region on the map."
              color: Qt.darker(root.contentForeground, 1.55)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
            }
          }

          Text {
            visible: root.statusText !== ""
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: root.statusText
            color: root.statusError ? Color.urgent : Qt.darker(root.contentForeground, 1.4)
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }
  }
}
