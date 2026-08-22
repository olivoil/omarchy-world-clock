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
    timeline: [],
    featured_cities: []
  })
  property bool snapshotLoaded: false
  property bool summaryFocusPending: false
  property string mode: "read"
  property bool live: true
  property bool editorActive: false
  property bool timeEditorActive: false
  property bool labelEditorActive: false
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
  property bool mapSelectionCardOnRight: true
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
  property bool globeInitialized: false
  property bool globeDetailRequested: false
  property bool searchVisible: false
  property bool keyboardCursorActive: false
  property int keyboardClockIndex: -1
  property var timelineHoverMinutes: null
  property string timelineHoverOwner: ""

  readonly property var barIdentity: hostWidget || root
  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
  readonly property bool mapUsesLightLabels:
    contentForeground.r * 0.2126 + contentForeground.g * 0.7152 + contentForeground.b * 0.0722
      >= Color.background.r * 0.2126 + Color.background.g * 0.7152 + Color.background.b * 0.0722
  readonly property color mapLabelForeground: mapUsesLightLabels
    ? Qt.lighter(contentForeground, 1.20) : Qt.darker(contentForeground, 1.20)
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property color searchSurfaceColor: {
    var mixed = root.mixColor(Color.popups.background, root.contentForeground, 0.025)
    return Qt.rgba(mixed.r, mixed.g, mixed.b, 1)
  }
  readonly property var clocks: snapshot && Array.isArray(snapshot.clocks) ? snapshot.clocks : []
  readonly property var timeline: snapshot && Array.isArray(snapshot.timeline) ? snapshot.timeline : []
  readonly property var featuredCities: snapshot && Array.isArray(snapshot.featured_cities)
    ? snapshot.featured_cities : []
  readonly property var summary: snapshot && snapshot.summary ? snapshot.summary : ({ time: "--:--", title: "", timezone: "", day: "", notation: "" })
  readonly property var weatherLocations: weather && Array.isArray(weather.locations)
    ? weather.locations : []
  readonly property bool weatherEnabled:
    root.setting("showWeather", true) !== false
  readonly property bool weatherLoading: root.weatherEnabled && weatherProcess.running
  readonly property string weatherUnitOverride:
    String(snapshot.weather_unit || "").trim().toLowerCase()
  readonly property bool weatherUseImperial: weatherUnitOverride === "imperial"
    || (weatherUnitOverride !== "metric"
      && root.localeUsesImperial(Qt.locale().name))
  readonly property bool weatherPresentationActive: root.weatherEnabled && root.live
    && weather.disabled !== true
    && (root.weatherLoading || root.weatherLocations.length > 0)
  readonly property int weatherRefreshMilliseconds: 15 * 60 * 1000
  readonly property int weatherFreshnessCheckMilliseconds: 30 * 1000
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
  readonly property bool showOpenMeteoAttribution: {
    if (!searchVisible || !Array.isArray(searchResults)) return false
    for (var resultIndex = 0; resultIndex < searchResults.length; resultIndex++)
      if (searchResults[resultIndex]
          && searchResults[resultIndex].open_meteo_attribution === true) return true
    return false
  }
  readonly property var mapClocks: {
    var entries = []
    if (root.hasMapCoordinate(summary)) entries.push(summary)
    for (var i = 0; i < clocks.length; i++)
      if (root.hasMapCoordinate(clocks[i])) entries.push(clocks[i])
    return entries
  }
  readonly property var globeLocations: {
    var entries = []
    var seenPlaces = ({})
    for (var savedIndex = 0; savedIndex < mapClocks.length; savedIndex++) {
      var saved = mapClocks[savedIndex]
      var savedKey = root.mapLocationKey(saved)
      entries.push({ location: saved, configured: true })
      if (savedKey) seenPlaces[savedKey] = true
    }
    for (var cityIndex = 0; cityIndex < featuredCities.length; cityIndex++) {
      var city = featuredCities[cityIndex]
      var cityKey = root.mapLocationKey(city)
      if (!root.hasMapCoordinate(city) || seenPlaces[cityKey]) continue
      entries.push({ location: city, configured: false, featured: true })
      if (cityKey) seenPlaces[cityKey] = true
    }
    return entries
  }
  readonly property bool searchHasQuery: root.searchVisible
    && String(addField.text || "").trim() !== ""
  readonly property var searchMapLocations: {
    var entries = []
    for (var resultIndex = 0; resultIndex < searchResults.length; resultIndex++) {
      var result = searchResults[resultIndex]
      if (root.hasMapCoordinate(result))
        entries.push({ location: result, configured: false, searchResult: true })
    }
    return entries
  }
  readonly property var mapLocations: searchHasQuery
    ? searchMapLocations : globeLocations
  readonly property int timelineExtent: {
    var extent = 60
    for (var i = 0; i < timeline.length; i++)
      extent = Math.max(extent, Math.abs(Number(timeline[i].relative_minutes || 0)))
    return Math.ceil(extent / 60) * 60 + 60
  }

  onClocksChanged: {
    if (keyboardClockIndex >= clocks.length)
      keyboardClockIndex = clocks.length - 1
  }

  FontMetrics {
    id: searchResultTitleMetrics
    font.family: root.contentFontFamily
    font.pixelSize: Style.font.body
    font.bold: true
  }

  FontMetrics {
    id: searchResultSubtitleMetrics
    font.family: root.contentFontFamily
    font.pixelSize: Style.font.caption
  }

  function measuredSearchResultWidth() {
    var contentWidth = 0
    for (var resultIndex = 0; resultIndex < searchResults.length; resultIndex++) {
      var result = searchResults[resultIndex] || ({})
      contentWidth = Math.max(contentWidth,
        searchResultTitleMetrics.advanceWidth(String(result.title || "")),
        searchResultSubtitleMetrics.advanceWidth(String(result.subtitle || "")))
    }
    return Math.ceil(contentWidth + Style.spacing.rowPaddingX * 2 + Style.space(8))
  }

  function searchModuleWidth(availableWidth) {
    var available = Math.max(1, Number(availableWidth))
    return Math.max(available * 0.38,
      Math.min(available * 0.75, measuredSearchResultWidth()))
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

  function focusAddField(selectExisting) {
    Qt.callLater(function() {
      if (!opened || mode !== "add" || !searchVisible || !canAdd) return
      addField.forceActiveFocus(Qt.ShortcutFocusReason)
      if (selectExisting === false)
        addField.cursorPosition = addField.text.length
      else
        addField.selectAll()
    })
  }

  function openSearch(initialText) {
    if (mode !== "add" || !canAdd) return
    var seed = String(initialText || "")
    searchVisible = true
    clearStatus()
    if (seed) addField.text = seed
    focusAddField(seed === "")
  }

  function closeSearch() {
    searchVisible = false
    searchDebounce.stop()
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    addField.text = ""
    mapSelection = null
    mapClickPending = false
    editorActive = false
    keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
    Qt.callLater(function() {
      if (opened && mode === "add") keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
    })
  }

  function openAdd() {
    mode = "add"
    searchVisible = false
    var alreadyOpened = opened
    controller.show()
    if (alreadyOpened) refresh()
  }

  function close() {
    mode = "read"
    globeDetailRequested = false
    summaryFocusPending = false
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    searchVisible = false
    mapSelection = null
    mapClickPending = false
    globeInitialized = false
    keyboardCursorActive = false
    keyboardClockIndex = -1
    controller.hide()
  }

  function itemContainsPanelPoint(item, panelX, panelY) {
    if (!item || !item.visible || !item.enabled) return false
    var local = keyCatcher.mapToItem(item, panelX, panelY)
    return local.x >= 0 && local.y >= 0
      && local.x <= item.width && local.y <= item.height
  }

  function pointerInsideActiveEditor(panelX, panelY) {
    if (summaryInput.activeFocus
        && itemContainsPanelPoint(summaryInput, panelX, panelY)) return true
    if (summaryLabelInput.activeFocus
        && itemContainsPanelPoint(summaryLabelInput, panelX, panelY)) return true
    if (addField.activeFocus
        && itemContainsPanelPoint(addField, panelX, panelY)) return true
    for (var clockIndex = 0; clockIndex < clocks.length; clockIndex++) {
      var cell = clockCellAt(clockIndex)
      if (cell && cell.pointerInsideActiveEditor(panelX, panelY)) return true
    }
    return false
  }

  function handlePanelPointerTap(panelX, panelY) {
    keyboardCursorActive = false
    keyboardClockIndex = -1
    var pointerX = Number(panelX)
    var pointerY = Number(panelY)
    Qt.callLater(function() {
      if (!root.opened || !root.editorActive
          || root.pointerInsideActiveEditor(pointerX, pointerY)) return
      keyCatcher.forceActiveFocus(Qt.MouseFocusReason)
    })
  }

  function mixColor(base, tint, amount) {
    var ratio = Math.max(0, Math.min(1, Number(amount)))
    return Qt.rgba(
      base.r * (1 - ratio) + tint.r * ratio,
      base.g * (1 - ratio) + tint.g * ratio,
      base.b * (1 - ratio) + tint.b * ratio,
      1)
  }

  function initializeGlobe() {
    if (globeInitialized || mode !== "add" || !snapshotLoaded) return
    globeInitialized = true
    if (hasMapCoordinate(summary))
      mapCanvas.settleOn(summary.latitude, summary.longitude)
    else
      mapCanvas.settleOn(18, 0)
  }

  function requestGlobeDetailWhenReady() {
    if (!opened || mode !== "add" || !mapCanvas.previewReady) return
    globeDetailRequested = true
  }

  function focusGlobeOn(location, zoomValue) {
    if (!location || !hasMapCoordinate(location) || mode !== "add") return
    mapCanvas.focusOn(location.latitude, location.longitude,
      zoomValue === undefined ? 1.95 : zoomValue)
  }

  function clockCellAt(clockIndex) {
    var normalizedIndex = Number(clockIndex)
    if (!isFinite(normalizedIndex) || normalizedIndex < 0
        || normalizedIndex >= clocks.length) return null
    var row = clockRowRepeater.itemAt(Math.floor(normalizedIndex / 3))
    return row ? row.cellAt(normalizedIndex % 3) : null
  }

  function ensureKeyboardCursorVisible() {
    if (!keyboardCursorActive || mode === "add") return
    var target = keyboardClockIndex < 0
      ? summaryInput : clockCellAt(keyboardClockIndex)
    if (!target) return
    var mapped = target.mapToItem(panelColumn, 0, 0)
    var top = mapped.y - Style.space(12)
    var bottom = mapped.y + target.height + Style.space(12)
    if (top < panelScroll.contentY)
      panelScroll.contentY = Math.max(0, top)
    else if (bottom > panelScroll.contentY + panelScroll.height)
      panelScroll.contentY = Math.min(
        Math.max(0, panelScroll.contentHeight - panelScroll.height),
        bottom - panelScroll.height)
  }

  function moveKeyboardCursor(dx, dy) {
    if (mode === "add") {
      if (mapSelection !== null) dismissMapSelection()
      mapClickPending = false
      mapCanvas.setView(mapCanvas.latitude + Number(dy) * 7,
        mapCanvas.longitude - Number(dx) * 10)
      return
    }

    var count = clocks.length
    if (!keyboardCursorActive) {
      keyboardCursorActive = true
      keyboardClockIndex = count > 0 ? 0 : -1
      Qt.callLater(root.ensureKeyboardCursorVisible)
      return
    }
    if (count === 0) {
      keyboardClockIndex = -1
      return
    }

    var nextIndex = keyboardClockIndex
    if (Number(dy) > 0)
      nextIndex = nextIndex < 0 ? 0 : Math.min(count - 1, nextIndex + 3)
    else if (Number(dy) < 0)
      nextIndex = nextIndex < 3 ? -1 : nextIndex - 3
    else if (Number(dx) > 0)
      nextIndex = nextIndex < 0 ? 0 : Math.min(count - 1, nextIndex + 1)
    else if (Number(dx) < 0)
      nextIndex = nextIndex < 0 ? count - 1 : Math.max(0, nextIndex - 1)
    keyboardClockIndex = nextIndex
    Qt.callLater(root.ensureKeyboardCursorVisible)
  }

  function focusClockEditor(clockIndex) {
    Qt.callLater(function() {
      if (!opened || (mode !== "read" && mode !== "edit")) return
      var cell = root.clockCellAt(clockIndex)
      if (!cell) return
      if (mode === "read") cell.focusTimeEditor()
      else cell.focusLabelEditor(Qt.ShortcutFocusReason)
    })
  }

  function focusSummaryLabelEditor() {
    if (!opened || mode !== "edit" || !localTimezoneConfigured) return
    headerTitle.beginLabelEdit(Qt.ShortcutFocusReason)
  }

  function activateKeyboardCursor() {
    if (mode === "add") {
      openSearch()
      return
    }
    if (!keyboardCursorActive) {
      if (mode === "read") focusSummaryEditor()
      else {
        keyboardCursorActive = true
        keyboardClockIndex = clocks.length > 0 ? 0 : -1
      }
      return
    }
    if (keyboardClockIndex < 0) {
      if (mode === "read") focusSummaryEditor()
      else if (mode === "edit") focusSummaryLabelEditor()
      return
    }
    if (keyboardClockIndex >= clocks.length) return
    if (mode === "read" || mode === "edit") focusClockEditor(keyboardClockIndex)
  }

  function deleteKeyboardCursor() {
    if (mode !== "edit" || !keyboardCursorActive
        || keyboardClockIndex < 0 || keyboardClockIndex >= clocks.length
        || !canRemove || actionProcess.running) return
    removeClock(clocks[keyboardClockIndex])
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

  function conversionSource(clock) {
    if (!clock) return ""
    var label = clock.label !== null && clock.label !== undefined
      ? clock.label : clock.title
    return String(clock.timezone || "") + "\u001f" + String(label || "")
  }

  function localeTerritory(localeName) {
    var name = String(localeName || "").split(/[.@]/)[0]
    var parts = name.split(/[-_]/)
    if (parts.length < 2) return ""
    var territory = String(parts[parts.length - 1] || "").toUpperCase()
    return /^[A-Z]{2}$/.test(territory) ? territory : ""
  }

  function localeUsesImperial(localeName) {
    var territory = localeTerritory(localeName)
    return ["US", "LR", "MM"].indexOf(territory) !== -1
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

  function clearWeatherState() {
    weather = ({
      source: "Open-Meteo",
      attribution_url: "https://open-meteo.com/",
      disabled: false,
      locations: []
    })
    weatherError = false
    weatherRequestPending = false
    weatherLoadedSignature = ""
    weatherActiveSignature = ""
    weatherAttemptSignature = ""
    weatherLastUpdatedAt = 0
    weatherLastAttemptAt = 0
  }

  function requestWeather(force) {
    if (!weatherEnabled) {
      weatherRequestPending = false
      return
    }
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
    var command = [backendCommand, "weather"]
    if (snapshot && snapshot.reference_utc)
      command.push("--at", String(snapshot.reference_utc))
    weatherProcess.command = command
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
      clearTimelineHover()
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
      if (mode === "add") Qt.callLater(root.initializeGlobe)
    } catch (error) {
      setStatus("World Clock backend returned invalid data.", true)
    }
  }

  function requestSnapshot(referenceUtc) {
    var reference = String(referenceUtc || "")
    if (timeEditorActive || labelEditorActive) {
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
    if (timeEditorActive || labelEditorActive) return
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

  function runAction(name, timezone, result, value) {
    if (actionProcess.running) return false
    if (name === "add" && !canAddLocation(timezone)) {
      setStatus("Only " + currentLocationTitle
        + " can be added at the nine-location limit.", true)
      return false
    }
    if (name === "add" && result && hasMapCoordinate(result))
      mapCanvas.focusOnLocations([result])
    actionName = name
    var command = [backendCommand, name]
    if (name !== "unpin") command.push(String(timezone || ""))
    if ((name === "add" || name === "pin" || name === "remove"
        || name === "rename") && result) {
      var actionLabel = result.label !== null && result.label !== undefined
        ? result.label : result.title
      command.push("--label", String(actionLabel || ""))
      if (name === "rename") command.push("--new-label", String(value || ""))
      if (name === "add" && result.latitude !== null && result.latitude !== undefined
          && result.longitude !== null && result.longitude !== undefined) {
        command.push("--latitude", String(result.latitude))
        command.push("--longitude", String(result.longitude))
      }
    }
    actionProcess.command = command
    actionProcess.running = true
    return true
  }

  function renameClock(clock, label) {
    if (!clock) return false
    var nextLabel = String(label || "").trim()
    var currentLabel = String(clock.label !== null && clock.label !== undefined
      ? clock.label : clock.title || "").trim()
    if (nextLabel === currentLabel) return false
    return runAction("rename", clock.timezone, clock, nextLabel)
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
    mapSelection = null
    mapClickPending = false
    if (!searchVisible) {
      searchDebounce.stop()
      searchResults = []
      searchResultsQuery = ""
      searchSubmitQuery = ""
      return
    }
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    scheduleSearch()
  }

  function startSearch() {
    if (mode !== "add" || !searchVisible) return
    var query = String(addField.text || "").trim()
    if (!query) {
      searchResults = []
      searchResultsQuery = ""
      return
    }
    if (searchProcess.running) return
    searchQueryInFlight = query
    var command = [backendCommand, "search", query]
    if (snapshot && snapshot.reference_utc)
      command.push("--at", String(snapshot.reference_utc))
    searchProcess.command = command
    searchProcess.running = true
  }

  function addFirstResult() {
    var query = String(addField.text || "").trim()
    if (mode !== "add" || !searchVisible || !query || !canAdd
        || actionProcess.running) return
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

  function mapRectsOverlap(left, right) {
    var gap = Style.space(5)
    return left.x - gap < right.x + right.width
      && left.x + left.width + gap > right.x
      && left.y - gap < right.y + right.height
      && left.y + left.height + gap > right.y
  }

  function globeLabelWidth(location) {
    var titleLength = String(location && location.title || "").length
    var metadataLength = String(location
      && (location.time || location.timezone || location.subtitle) || "").length
    return Math.max(Style.space(76), Math.min(Style.space(176),
      Math.max(titleLength, metadataLength) * Style.spaceReal(8.2) + Style.space(20)))
  }

  function mapLocationKey(location) {
    if (!location) return ""
    var timezone = String(location.timezone || "").trim().toLowerCase()
    var title = String(location.title || location.label || "").trim().toLowerCase()
    return timezone && title ? timezone + "\u001f" + title : ""
  }

  function mapLocationSelected(location) {
    var selectedKey = mapLocationKey(mapSelection)
    return selectedKey !== "" && selectedKey === mapLocationKey(location)
  }

  function selectMapLocation(location) {
    if (!location || !canAddLocation(location.timezone)) return
    if (hasMapCoordinate(location)) {
      var projection = mapCanvas.project(location.latitude, location.longitude)
      mapSelectionCardOnRight = projection.x < mapCanvas.width / 2
    }
    mapClickPending = false
    mapSelection = location
    clearStatus()
    if (hasMapCoordinate(location)) mapCanvas.focusOnLocations([location])
  }

  function dismissMapSelection() {
    if (mapSelection === null) return
    mapSelection = null
    mapClickPending = false
    if (searchVisible) {
      closeSearch()
      return
    }
    Qt.callLater(function() {
      if (opened && mode === "add")
        keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
    })
  }

  function globeLocationReveal(wrapper) {
    if (!wrapper || wrapper.searchResult === true || wrapper.configured === true) return 1
    var minimumZoom = Number(wrapper.location && wrapper.location.minimum_zoom)
    if (!isFinite(minimumZoom)) minimumZoom = mapCanvas.minimumZoom
    return Math.max(0, Math.min(1, (mapCanvas.zoom - minimumZoom) / 0.24))
  }

  function globeLabelLayouts(width, height) {
    var placed = []
    var layouts = []
    var labelHeight = Style.space(42)
    var edge = Style.space(8)
    var pointGap = Style.space(12)

    for (var i = 0; i < mapLocations.length; i++) {
      var wrapper = mapLocations[i]
      var location = wrapper.location
      var projection = mapCanvas.project(location.latitude, location.longitude)
      var reveal = globeLocationReveal(wrapper)
      var layout = {
        visible: false,
        labelVisible: false,
        x: 0,
        y: 0,
        width: globeLabelWidth(location),
        height: labelHeight,
        pointX: projection.x,
        pointY: projection.y,
        depth: projection.depth,
        reveal: reveal
      }
      layouts.push(layout)
      if (!projection.visible || reveal <= 0.01) {
        continue
      }

      var mayPlaceLabel = !mapLocationSelected(location)
        && (wrapper.searchResult === true || wrapper.configured || reveal >= 0.72)
      var labelWidth = layout.width
      var candidates = [
        { x: projection.x - labelWidth / 2, y: projection.y - labelHeight - pointGap },
        { x: projection.x + pointGap, y: projection.y - labelHeight / 2 },
        { x: projection.x - labelWidth - pointGap, y: projection.y - labelHeight / 2 },
        { x: projection.x - labelWidth / 2, y: projection.y + pointGap },
        { x: projection.x + pointGap, y: projection.y - labelHeight - pointGap },
        { x: projection.x - labelWidth - pointGap, y: projection.y - labelHeight - pointGap }
      ]
      var chosen = null
      if (mayPlaceLabel) {
        for (var candidateIndex = 0; candidateIndex < candidates.length && !chosen;
             candidateIndex++) {
          var raw = candidates[candidateIndex]
          var candidate = {
            x: Math.max(edge, Math.min(width - labelWidth - edge, raw.x)),
            y: Math.max(edge, Math.min(height - labelHeight - edge, raw.y)),
            width: labelWidth,
            height: labelHeight
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

      if (chosen) {
        layout.x = chosen.x
        layout.y = chosen.y
        layout.labelVisible = true
        placed.push(chosen)
      }
      layout.visible = true
    }
    return layouts
  }

  function requestMapLocation(latitude, longitude, x, y) {
    if (!canAdd) return
    mapCursorX = x
    mapCursorY = y
    mapSelectionCardOnRight = x < mapCanvas.width / 2
    mapRequestedLongitude = Math.max(-179.999999,
      Math.min(179.999999, Number(longitude)))
    mapRequestedLatitude = Math.max(-89.999999,
      Math.min(89.999999, Number(latitude)))
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

  function timelineHoverMatches(relativeMinutes) {
    return timelineHoverMinutes !== null
      && Number(relativeMinutes || 0) === Number(timelineHoverMinutes)
  }

  function updateTimelineHover(owner, relativeMinutes, hovered) {
    var normalizedOwner = String(owner || "")
    if (hovered) {
      timelineHoverOwner = normalizedOwner
      timelineHoverMinutes = Number(relativeMinutes || 0)
    } else if (timelineHoverOwner === normalizedOwner) {
      timelineHoverOwner = ""
      timelineHoverMinutes = null
    }
  }

  function clearTimelineHover() {
    timelineHoverOwner = ""
    timelineHoverMinutes = null
  }

  onOpenedChanged: {
    if (!opened) {
      globeDetailRequested = false
      clearTimelineHover()
      return
    }
    if (mode === "add") Qt.callLater(root.requestGlobeDetailWhenReady)
    refresh()
    requestWeather(false)
  }
  onWeatherEnabledChanged: {
    if (!weatherEnabled) {
      clearWeatherState()
    } else if (opened && snapshotLoaded && live) {
      Qt.callLater(function() { root.requestWeather(true) })
    }
  }
  onTimeEditorActiveChanged: {
    if (!timeEditorActive && editorRefreshPending)
      Qt.callLater(root.flushEditorRefresh)
  }
  onLabelEditorActiveChanged: {
    if (!labelEditorActive && editorRefreshPending)
      Qt.callLater(root.flushEditorRefresh)
  }
  onModeChanged: {
    clearTimelineHover()
    if (mode === "add") {
      searchVisible = false
      Qt.callLater(root.initializeGlobe)
      Qt.callLater(root.requestGlobeDetailWhenReady)
    } else {
      searchVisible = false
      addField.text = ""
      searchDebounce.stop()
      searchResults = []
      searchResultsQuery = ""
      searchSubmitQuery = ""
      mapSelection = null
      mapClickPending = false
    }
  }
  onCanAddChanged: if (!canAdd && searchVisible) closeSearch()

  Process {
    id: snapshotProcess
    stdout: StdioCollector { id: snapshotOutput; waitForEnd: true }
    stderr: StdioCollector { id: snapshotError; waitForEnd: true }
    onExited: function(exitCode) {
      var current = root.snapshotActiveGeneration === root.snapshotStateGeneration
      var manual = root.snapshotActiveReference !== ""
      if (exitCode === 0 && current
          && (root.timeEditorActive || root.labelEditorActive)) {
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
      if (!root.weatherEnabled) {
        root.weatherRequestPending = false
        return
      }
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
        var message = String(actionError.text || "Could not update World Clock.").trim()
        if (root.actionName === "add" && message.indexOf("already configured") >= 0)
          message = "That location is already added."
        root.setStatus(message, true)
        return
      }
      if (root.actionName === "add") {
        addField.text = ""
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
        root.mapSelection = null
        root.mapClickPending = false
        root.mode = "read"
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
      if (root.mode !== "add" || !root.searchVisible) {
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
          mapCanvas.focusOnLocations(root.searchResults)
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
      // Typing, choosing a marker, or dismissing the detail card cancels the
      // bare-map request. Its process may still finish, but that stale result
      // must not replace the user's newer interaction.
      if (!root.mapClickPending) return
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
          root.clearStatus()
          mapCanvas.focusOnLocations([payload])
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
    interval: root.weatherFreshnessCheckMilliseconds
    running: root.weatherEnabled && root.opened && root.live
      && root.weather.disabled !== true
    repeat: true
    onTriggered: root.requestWeather(false)
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

    WorldClockKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.editorActive || addField.activeFocus
      directTextInput: root.mode === "add" && !addField.activeFocus
      onMoveRequested: function(dx, dy) { root.moveKeyboardCursor(dx, dy) }
      onActivateRequested: root.activateKeyboardCursor()
      onDeleteRequested: root.deleteKeyboardCursor()
      onCloseRequested: {
        if (root.mapSelection !== null) root.dismissMapSelection()
        else if (root.mode === "add" && root.searchVisible) root.closeSearch()
        else if (root.mode === "read") root.close()
        else root.mode = "read"
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(text) {
        if (root.mode === "add") {
          if (!root.searchVisible) root.openSearch(text)
          else {
            addField.text += text
            root.focusAddField(false)
          }
          return
        }
        if (text === "a" || text === "A") root.mode = "add"
        else if (text === "e" || text === "E") root.mode = root.mode === "edit" ? "read" : "edit"
        else if (text === "r" || text === "R") root.returnToLive()
      }

      // Observe pointer taps without covering the controls below. Empty
      // panel space otherwise lands on KeyboardPanel's click-swallowing
      // layer, which intentionally does not move keyboard focus.
      TapHandler {
        id: focusDismissHandler
        parent: keyCatcher
        acceptedButtons: Qt.LeftButton
        onTapped: function(eventPoint) {
          root.handlePanelPointerTap(eventPoint.position.x, eventPoint.position.y)
        }
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
          spacing: Style.space(root.mode === "add" ? 14 : 8)

          Item {
            visible: root.mode !== "add"
            width: parent.width
            height: visible
              ? Math.max(headerTitle.implicitHeight, headerStart.implicitHeight,
                headerActions.implicitHeight)
              : 0

            Row {
              id: headerStart
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(6)

              Button {
                iconText: "󰑐"
                active: !root.live
                enabled: !actionProcess.running
                tooltipText: "Return to live time"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.returnToLive()
              }

              Button {
                id: weatherProviderAttribution
                anchors.verticalCenter: parent.verticalCenter
                visible: root.mode !== "add" && root.weatherEnabled && root.live
                  && root.weather.disabled !== true
                text: root.weatherLocations.length > 0 || !root.weatherError
                  ? "Open-Meteo"
                    + (root.weatherError ? "  ·  Update unavailable" : "")
                  : "Weather unavailable"
                tooltipText: "Weather data by Open-Meteo"
                foreground: Qt.darker(root.contentForeground, 1.35)
                background: "transparent"
                fontFamily: root.contentFontFamily
                fontSize: Style.fontPx(0.75)
                focusable: true
                horizontalPadding: Style.space(4)
                verticalPadding: Style.space(1)
                onClicked: Qt.openUrlExternally("https://open-meteo.com/")
              }

              Button {
                visible: root.summary.pinned === true
                enabled: !actionProcess.running
                text: "UNPIN"
                selected: true
                tooltipText: "Remove this time from the bar"
                fontSize: Style.font.caption
                horizontalPadding: Style.space(6)
                verticalPadding: Style.space(4)
                onClicked: root.togglePin(root.summary)
              }
            }

            Item {
              id: headerTitle
              anchors.centerIn: parent
              readonly property bool editable:
                root.mode === "edit" && root.localTimezoneConfigured
              property bool labelEditing: false
              function beginLabelEdit(focusReason) {
                if (!editable || actionProcess.running) return
                labelEditing = true
                summaryLabelInput.resetText()
                Qt.callLater(function() {
                  if (!headerTitle.editable || !headerTitle.labelEditing) return
                  var reason = focusReason === undefined
                    ? Qt.MouseFocusReason : focusReason
                  summaryLabelInput.forceActiveFocus(reason)
                  if (reason === Qt.MouseFocusReason)
                    summaryLabelInput.cursorPosition = summaryLabelInput.text.length
                  else
                    summaryLabelInput.selectAll()
                })
              }
              onEditableChanged: if (!editable) labelEditing = false
              implicitWidth: labelEditing
                ? Style.space(260) : headerReadTitle.implicitWidth
              implicitHeight: Math.max(headerReadTitle.implicitHeight,
                summaryLabelInput.implicitHeight)
              width: implicitWidth
              height: implicitHeight

              Text {
                textFormat: Text.PlainText
                id: headerReadTitle
                visible: !headerTitle.labelEditing
                anchors.centerIn: parent
                text: root.currentLocationTitle
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.space(18)
                font.bold: true
              }

              TextInput {
                id: summaryLabelInput
                function resetText() {
                  text = String(root.summary.label || root.currentLocationTitle)
                }
                visible: headerTitle.editable && headerTitle.labelEditing
                anchors.fill: parent
                horizontalAlignment: Text.AlignHCenter
                color: root.contentForeground
                selectionColor: Style.selectionFill
                selectedTextColor: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.space(18)
                font.bold: true
                selectByMouse: true
                clip: true
                enabled: visible && root.snapshotLoaded && !actionProcess.running
                Accessible.name: "Location name"
                Accessible.description: "Press Enter to save or Escape to cancel"
                Component.onCompleted: resetText()
                onVisibleChanged: if (visible && !activeFocus) resetText()
                onAccepted: {
                  root.renameClock(root.summary, text)
                  keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                }
                onActiveFocusChanged: {
                  root.editorActive = activeFocus
                  root.labelEditorActive = activeFocus
                  if (!activeFocus) {
                    resetText()
                    headerTitle.labelEditing = false
                  }
                }
                Keys.onEscapePressed: function(event) {
                  resetText()
                  keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                  event.accepted = true
                }
              }

              MouseArea {
                id: summaryLabelMouse
                visible: headerTitle.editable && !headerTitle.labelEditing
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.IBeamCursor
                Accessible.name: "Rename " + root.currentLocationTitle
                Accessible.role: Accessible.Button
                onClicked: headerTitle.beginLabelEdit(Qt.MouseFocusReason)
              }
            }

            Row {
              id: headerActions
              visible: root.mode !== "add"
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(6)

              Button {
                iconText: "󰐕"
                enabled: root.canAdd && !actionProcess.running
                tooltipText: root.canAdd ? "Add a location" : "Nine locations already shown"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.mode = "add"
              }

              Button {
                id: editModeButton
                iconText: "󰏫"
                active: root.mode === "edit"
                enabled: !actionProcess.running
                tooltipText: root.mode === "edit"
                  ? "Finish editing" : "Rename, pin, or remove locations"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: {
                  keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                  root.mode = root.mode === "edit" ? "read" : "edit"
                }
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
                  || root.keyboardCursorActive && root.keyboardClockIndex < 0
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
                  textFormat: Text.PlainText
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
                  textFormat: Text.PlainText
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
                    textFormat: Text.PlainText
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
                  readonly property bool linkedHovered:
                    root.timelineHoverMatches(modelData.relative_minutes)
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
                    opacity: timelinePoint.linkedHovered ? 0.48 : 0.24

                    Behavior on opacity {
                      NumberAnimation { duration: 160; easing.type: Easing.OutQuart }
                    }
                  }

                  Rectangle {
                    id: markerHalo
                    x: Math.round((parent.width - width) / 2)
                    y: timelineView.railY - height / 2
                    width: Style.space(timelinePoint.localPoint
                      ? 11 : (Number(timelinePoint.modelData.count || 1) > 1 ? 9 : 7))
                    height: width
                    radius: width / 2
                    scale: timelinePoint.linkedHovered ? 1.45 : 1
                    color: timelinePoint.localPoint || timelinePoint.linkedHovered
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : "transparent"
                    border.width: timelinePoint.localPoint || timelinePoint.linkedHovered
                      || Number(timelinePoint.modelData.count || 1) > 1
                      ? Style.spacing.hairline : 0
                    border.color: timelinePoint.localPoint || timelinePoint.linkedHovered
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : root.contentForeground

                    Behavior on scale {
                      NumberAnimation { duration: 180; easing.type: Easing.OutQuart }
                    }

                    Behavior on color { ColorAnimation { duration: 150 } }

                    Rectangle {
                      anchors.centerIn: parent
                      width: Style.space(5)
                      height: width
                      radius: width / 2
                      color: timelinePoint.localPoint || timelinePoint.linkedHovered
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                      opacity: timelinePoint.localPoint || timelinePoint.linkedHovered ? 1 : 0.82

                      Behavior on color { ColorAnimation { duration: 150 } }
                      Behavior on opacity { NumberAnimation { duration: 150 } }
                    }
                  }

                  Item {
                    width: Style.space(24)
                    height: width
                    x: Math.round((parent.width - width) / 2)
                    y: timelineView.railY - height / 2

                    HoverHandler {
                      id: timelinePointHover
                      onHoveredChanged: root.updateTimelineHover(
                        "timeline:" + String(timelinePoint.modelData.relative_minutes),
                        timelinePoint.modelData.relative_minutes, hovered)
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
                      textFormat: Text.PlainText
                      width: parent.width
                      horizontalAlignment: Text.AlignHCenter
                      text: timelinePoint.modelData.time
                      color: timelinePoint.localPoint || timelinePoint.linkedHovered
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.bodySmall
                      font.bold: timelinePoint.localPoint || timelinePoint.linkedHovered

                      Behavior on color { ColorAnimation { duration: 150 } }
                    }

                    Text {
                      textFormat: Text.PlainText
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
                id: clockRowRepeater
                model: Math.ceil(root.clocks.length / 3)

                Row {
                  id: clockRow
                  required property int index
                  readonly property int startIndex: index * 3
                  readonly property int itemCount:
                    Math.min(3, root.clocks.length - startIndex)
                  readonly property real cellWidth:
                    (clockRows.width - Style.space(32)) / 3
                  function cellAt(cellIndex) {
                    return clockCellRepeater.itemAt(cellIndex)
                  }
                  anchors.horizontalCenter: parent.horizontalCenter
                  width: itemCount * cellWidth
                    + Math.max(0, itemCount - 1) * spacing
                  height: Style.space(root.mode === "edit" ? 110 : 100)
                  spacing: Style.space(16)

                  Repeater {
                    id: clockCellRepeater
                    model: clockRow.itemCount

                    Item {
                      id: clockCell
                      required property int index
                      readonly property var clockData:
                        root.clocks[clockRow.startIndex + index]
                      readonly property var weatherData: root.weatherFor(clockData)
                      readonly property bool showWeather: root.weatherPresentationActive
                        && (weatherData !== null || root.weatherLoading)
                      readonly property int clockIndex: clockRow.startIndex + index
                      readonly property bool hasKeyboardCursor:
                        root.keyboardCursorActive
                          && root.keyboardClockIndex === clockIndex
                      readonly property bool linkedHovered:
                        root.timelineHoverMatches(clockData.relative_minutes)
                      property bool labelEditing: false
                      function focusTimeEditor() {
                        cardTimeInput.forceActiveFocus(Qt.ShortcutFocusReason)
                        cardTimeInput.selectAll()
                      }
                      function focusLabelEditor(focusReason) {
                        if (root.mode !== "edit" || actionProcess.running) return
                        labelEditing = true
                        cardLabelInput.resetText()
                        Qt.callLater(function() {
                          if (root.mode !== "edit" || !clockCell.labelEditing) return
                          var reason = focusReason === undefined
                            ? Qt.MouseFocusReason : focusReason
                          cardLabelInput.forceActiveFocus(reason)
                          if (reason === Qt.MouseFocusReason)
                            cardLabelInput.cursorPosition = cardLabelInput.text.length
                          else
                            cardLabelInput.selectAll()
                        })
                      }
                      function pointerInsideActiveEditor(panelX, panelY) {
                        if (cardTimeInput.activeFocus)
                          return root.itemContainsPanelPoint(
                            cardTimeInput, panelX, panelY)
                        if (cardLabelInput.activeFocus)
                          return root.itemContainsPanelPoint(
                            cardLabelInput, panelX, panelY)
                        return false
                      }
                      onClockDataChanged: {
                        if (!cardLabelInput.activeFocus) cardLabelInput.resetText()
                      }
                      width: clockRow.cellWidth
                      height: clockRow.height
                      clip: true

                      HoverHandler {
                        id: cardHoverHandler
                        onHoveredChanged: root.updateTimelineHover(
                          "card:" + String(clockCell.clockIndex),
                          clockCell.clockData.relative_minutes, hovered)
                      }

                      Rectangle {
                        id: clockSurface
                        anchors.fill: parent
                        radius: Style.cornerRadius
                        color: clockCell.hasKeyboardCursor || clockCell.linkedHovered
                          ? Style.hoverFillFor(root.contentForeground, Color.accent)
                          : Style.normalFillFor(root.contentForeground, Color.accent)
                        border.width: clockCell.hasKeyboardCursor || clockCell.linkedHovered
                          ? Style.spacing.hairline : 0
                        border.color: clockCell.hasKeyboardCursor
                          ? Style.focusStateColor(root.contentForeground, Color.accent)
                          : Style.hoverBorderFor(root.contentForeground, Color.accent)

                        Behavior on color { ColorAnimation { duration: 150 } }
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
                          height: Math.max(cardTitle.implicitHeight,
                            cardLabelInput.implicitHeight, cardControls.implicitHeight)

                          Text {
                            textFormat: Text.PlainText
                            id: cardTitle
                            visible: !clockCell.labelEditing
                            anchors.left: parent.left
                            anchors.right: cardControls.visible
                              ? cardControls.left : cardNotation.left
                            anchors.rightMargin: Style.space(6)
                            anchors.verticalCenter: parent.verticalCenter
                            text: String(clockCell.clockData.title || "").toUpperCase()
                            color: root.contentForeground
                            font.family: root.contentFontFamily
                            font.pixelSize: Style.font.bodySmall
                            font.bold: true
                            font.letterSpacing: 1
                            elide: Text.ElideRight
                          }

                          TextInput {
                            id: cardLabelInput
                            function resetText() {
                              text = String(clockCell.clockData.label
                                || clockCell.clockData.title || "")
                            }
                            visible: root.mode === "edit" && clockCell.labelEditing
                            anchors.left: parent.left
                            anchors.right: cardControls.left
                            anchors.rightMargin: Style.space(6)
                            anchors.verticalCenter: parent.verticalCenter
                            color: root.contentForeground
                            selectionColor: Style.selectionFill
                            selectedTextColor: root.contentForeground
                            font.family: root.contentFontFamily
                            font.pixelSize: Style.font.bodySmall
                            font.bold: true
                            font.capitalization: Font.AllUppercase
                            font.letterSpacing: 1
                            selectByMouse: true
                            clip: true
                            enabled: visible && root.snapshotLoaded && !actionProcess.running
                            Accessible.name: "Location name"
                            Accessible.description: "Press Enter to save or Escape to cancel"
                            Component.onCompleted: resetText()
                            onVisibleChanged: if (visible && !activeFocus) resetText()
                            onAccepted: {
                              root.renameClock(clockCell.clockData, text)
                              keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                            }
                            onActiveFocusChanged: {
                              root.editorActive = activeFocus
                              root.labelEditorActive = activeFocus
                              if (!activeFocus) {
                                resetText()
                                clockCell.labelEditing = false
                              }
                            }
                            Keys.onEscapePressed: function(event) {
                              resetText()
                              keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                              event.accepted = true
                            }
                          }

                          MouseArea {
                            id: cardLabelMouse
                            visible: root.mode === "edit" && !clockCell.labelEditing
                            anchors.left: parent.left
                            anchors.right: cardControls.left
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            hoverEnabled: true
                            cursorShape: Qt.IBeamCursor
                            Accessible.name: "Rename "
                              + String(clockCell.clockData.title || "location")
                            Accessible.role: Accessible.Button
                            onClicked: clockCell.focusLabelEditor(Qt.MouseFocusReason)
                          }

                          Text {
                            textFormat: Text.PlainText
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
                              enabled: !actionProcess.running
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
                              enabled: root.canRemove && !actionProcess.running
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
                            textFormat: Text.PlainText
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
                              textFormat: Text.PlainText
                              id: cardWeatherTemperature
                              visible: clockCell.weatherData !== null
                              anchors.right: parent.right
                              anchors.verticalCenter: parent.verticalCenter
                              text: clockCell.weatherData === null ? ""
                                : root.weatherTemperatureCompact(
                                  clockCell.weatherData.temperature_celsius)
                              color: Qt.darker(root.contentForeground, 1.5)
                              font.family: root.contentFontFamily
                              font.pixelSize: Style.font.caption
                              font.letterSpacing: 0.2
                            }

                            Text {
                              textFormat: Text.PlainText
                              id: cardWeatherGlyph
                              visible: clockCell.weatherData !== null
                              anchors.right: cardWeatherTemperature.left
                              anchors.rightMargin: Style.space(4)
                              anchors.baseline: cardWeatherTemperature.baseline
                              text: clockCell.weatherData === null ? ""
                                : root.weatherGlyph(clockCell.weatherData)
                              color: Qt.darker(root.contentForeground, 1.5)
                              font.family: root.contentFontFamily
                              font.pixelSize: Style.font.bodySmall
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

            Button {
              visible: root.clocks.length === 0
              anchors.horizontalCenter: parent.horizontalCenter
              text: "Add a location"
              iconText: "󰐕"
              bordered: true
              onClicked: root.mode = "add"
            }
          }

          Item {
            id: addPage
            visible: root.mode === "add"
            width: parent.width
            height: Math.max(Style.space(360),
              (panel.availableCardHeight > 0
                ? Math.min(panel.availableCardHeight, Style.space(680))
                : Style.space(680)) - panel.verticalContentInset)
            clip: true

            Rectangle {
              id: addSearchSurface
              visible: root.searchVisible
              anchors.top: parent.top
              anchors.topMargin: Style.space(12)
              anchors.right: parent.right
              anchors.rightMargin: Style.space(12)
              width: root.searchModuleWidth(Math.max(1,
                parent.width - addBackButton.width - Style.space(36)))
              height: addSearchButton.height
              z: 29
              radius: Style.cornerRadius
              color: root.searchSurfaceColor
              border.width: Style.spacing.hairline
              border.color: root.mixColor(Color.popups.background,
                root.contentForeground, 0.26)
              opacity: root.searchVisible ? 1 : 0

              Behavior on opacity {
                NumberAnimation { duration: 160; easing.type: Easing.OutCubic }
              }

              TextField {
                id: addField
                anchors.fill: parent
                z: 1
                rightPadding: addSearchCloseButton.width + Style.spacing.controlPaddingX
                placeholderText: "Search for a city or timezone"
                foreground: root.contentForeground
                enabled: root.searchVisible && root.canAdd && !actionProcess.running
                onTextChanged: root.searchTextChanged()
                onAccepted: root.addFirstResult()
                onActiveFocusChanged: root.editorActive = activeFocus
                Keys.onEscapePressed: function(event) {
                  if (root.mapSelection !== null) root.dismissMapSelection()
                  else root.closeSearch()
                  event.accepted = true
                }
              }

              Item {
                id: addSearchCloseButton
                anchors.right: parent.right
                anchors.rightMargin: Style.space(2)
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(38)
                height: parent.height
                z: 2

                Rectangle {
                  id: addSearchCloseHover
                  anchors.centerIn: parent
                  width: Style.space(28)
                  height: width
                  radius: Style.cornerRadius
                  color: addSearchCloseMouse.containsMouse
                    ? Style.hoverFillFor(root.contentForeground, Color.accent)
                    : "transparent"

                  Behavior on color { ColorAnimation { duration: 120 } }
                }

                Text {
                  textFormat: Text.PlainText
                  id: addSearchCloseGlyph
                  anchors.centerIn: parent
                  text: "󰅖"
                  color: root.contentForeground
                  opacity: addSearchCloseMouse.containsMouse ? 1 : 0.82
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.icon

                  Behavior on opacity { NumberAnimation { duration: 120 } }
                }

                MouseArea {
                  id: addSearchCloseMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.closeSearch()
                }
              }
            }

            Rectangle {
              id: addBackButtonSurface
              anchors.fill: addBackButton
              z: addBackButton.z - 1
              radius: addBackButton.radius
              color: root.searchSurfaceColor
            }

            Button {
              id: addBackButton
              anchors.top: parent.top
              anchors.left: parent.left
              anchors.margins: Style.space(12)
              width: Style.space(42)
              height: width
              z: 31
              radius: height / 2
              iconText: "󰅁"
              iconSize: Style.font.iconLarge
              tooltipText: "Back to world clock"
              foreground: root.contentForeground
              background: "transparent"
              bordered: true
              horizontalPadding: 0
              verticalPadding: 0
              onClicked: root.mode = "read"
            }

            Rectangle {
              id: addSearchButtonSurface
              visible: addSearchButton.visible
              anchors.fill: addSearchButton
              z: addSearchButton.z - 1
              radius: addSearchButton.radius
              color: root.searchSurfaceColor
            }

            Button {
              id: addSearchButton
              visible: !root.searchVisible
              anchors.top: parent.top
              anchors.right: parent.right
              anchors.margins: Style.space(12)
              width: Style.space(42)
              height: width
              z: 31
              radius: height / 2
              iconText: "󰍉"
              iconSize: Style.font.iconLarge
              tooltipText: "Search locations"
              foreground: root.contentForeground
              background: "transparent"
              bordered: true
              horizontalPadding: 0
              verticalPadding: 0
              onClicked: root.openSearch()
            }

            Rectangle {
              visible: !root.canAdd
              anchors.centerIn: parent
              width: capacityLabel.implicitWidth + Style.space(28)
              height: capacityLabel.implicitHeight + Style.space(16)
              z: 25
              radius: height / 2
              color: root.mixColor(Color.background, root.contentForeground, 0.035)
              border.width: Style.spacing.hairline
              border.color: root.mixColor(Color.background, root.contentForeground, 0.24)

              Text {
                textFormat: Text.PlainText
                id: capacityLabel
                anchors.centerIn: parent
                text: "Remove a location before adding another."
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.body
              }
            }

            Globe {
              id: mapCanvas
              anchors.fill: parent
              clip: true
              interactive: root.canAdd && !actionProcess.running
              highResolutionEnabled: root.opened && root.globeDetailRequested
              property var markerLayouts: root.globeLabelLayouts(width, height)
              diameterRatio: 0.63
              oceanColor: root.mixColor(Color.background, root.contentForeground, 0.07)
              landColor: root.mixColor(Color.background, root.contentForeground, 0.68)
              boundaryColor: root.mixColor(Color.background, root.contentForeground, 0.40)
              rimColor: root.mixColor(Color.background, root.contentForeground, 0.28)
              onPreviewReadyChanged: {
                if (previewReady) root.requestGlobeDetailWhenReady()
              }
              onLocationPicked: function(latitude, longitude, viewX, viewY) {
                root.requestMapLocation(latitude, longitude, viewX, viewY)
              }
              onViewInteractionStarted: root.dismissMapSelection()

              Rectangle {
                id: searchResultOverlay
                visible: root.searchVisible && root.searchResults.length > 0
                  && root.mapSelection === null
                x: addSearchSurface.x
                y: addSearchSurface.y + addSearchSurface.height + Style.space(8)
                width: addSearchSurface.width
                height: Math.min(root.searchResults.length * Style.space(48), Style.space(240))
                z: 29
                radius: Style.cornerRadius
                color: root.searchSurfaceColor
                border.width: Style.spacing.hairline
                border.color: root.mixColor(Color.background, root.contentForeground, 0.24)
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
                        textFormat: Text.PlainText
                        width: parent.width
                        text: resultButton.modelData.title
                        color: root.contentForeground
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.body
                        font.bold: true
                        elide: Text.ElideRight
                      }

                      Text {
                        textFormat: Text.PlainText
                        width: parent.width
                        text: resultButton.modelData.subtitle
                        color: Qt.darker(root.contentForeground, 1.5)
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.caption
                        elide: Text.ElideRight
                      }
                    }
                  }
                }
              }

              Button {
                id: openMeteoAttribution
                visible: root.showOpenMeteoAttribution
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.margins: Style.space(12)
                z: 28
                text: "Location data by Open-Meteo"
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

              Repeater {
                model: root.mapLocations

                Item {
                  id: mapMarker
                  required property int index
                  required property var modelData
                  readonly property var location: modelData.location
                  readonly property bool configured: modelData.configured === true
                  readonly property bool searchResult: modelData.searchResult === true
                  readonly property bool selectable: !configured
                    && root.canAddLocation(location.timezone)
                    && !actionProcess.running
                  readonly property bool selected:
                    root.mapLocationSelected(mapMarker.location)
                  readonly property var layout: mapCanvas.markerLayouts[index] || ({
                    visible: false,
                    labelVisible: false,
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    pointX: 0,
                    pointY: 0,
                    depth: -1,
                    reveal: 0
                  })
                  anchors.fill: parent
                  visible: layout && layout.visible
                  z: 4
                  opacity: layout ? layout.reveal * (searchResult || configured
                    ? 1 : Math.max(0.88, Math.min(1, 0.88 + layout.depth * 0.14))) : 0

                  Behavior on opacity {
                    NumberAnimation { duration: 160; easing.type: Easing.OutQuint }
                  }

                  Rectangle {
                    id: cityLabel
                    visible: mapMarker.layout.labelVisible && !mapMarker.selected
                    x: mapMarker.layout.x
                    y: mapMarker.layout.y
                    width: mapMarker.layout.width
                    height: mapMarker.layout.height
                    radius: Style.cornerRadius
                    color: mapMarker.selected
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : cityMouse.containsMouse && mapMarker.selectable
                      ? Style.hoverFillFor(root.contentForeground, Color.accent)
                      : "transparent"

                    Column {
                      anchors.leftMargin: Style.space(6)
                      anchors.rightMargin: Style.space(6)
                      anchors.left: parent.left
                      anchors.right: parent.right
                      anchors.verticalCenter: parent.verticalCenter
                      spacing: Style.space(1)

                      Text {
                        textFormat: Text.PlainText
                        width: parent.width
                        text: mapMarker.location.title
                        horizontalAlignment: Text.AlignHCenter
                        color: root.mapLabelForeground
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.title
                        font.bold: true
                        style: Text.Outline
                        styleColor: Color.background
                        elide: Text.ElideRight
                      }

                      Text {
                        textFormat: Text.PlainText
                        width: parent.width
                        text: mapMarker.location.time
                          || mapMarker.location.timezone
                          || mapMarker.location.subtitle
                        horizontalAlignment: Text.AlignHCenter
                        color: root.mapLabelForeground
                        opacity: 0.88
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.bodySmall
                        font.bold: false
                        style: Text.Outline
                        styleColor: Color.background
                        elide: Text.ElideRight
                      }
                    }

                    MouseArea {
                      id: cityMouse
                      anchors.fill: parent
                      enabled: true
                      hoverEnabled: mapMarker.selectable
                      cursorShape: mapMarker.selectable
                        ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: {
                        if (mapMarker.selectable)
                          root.selectMapLocation(mapMarker.location)
                      }
                    }
                  }

                  Rectangle {
                    x: mapMarker.layout.pointX - width / 2
                    y: mapMarker.layout.pointY - height / 2
                    width: mapMarker.selected ? Style.space(13)
                      : (mapMarker.searchResult ? Style.space(12)
                      : (mapMarker.configured ? Style.space(11) : Style.space(9))
                      )
                    height: width
                    radius: width / 2
                    color: mapMarker.selected
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : mapMarker.searchResult
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : mapMarker.configured
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : root.mixColor(Color.background, root.contentForeground, 0.22)
                    border.width: 1
                    border.color: mapMarker.selected
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : mapMarker.searchResult
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : mapMarker.configured
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : root.mixColor(Color.background, root.contentForeground, 0.72)

                    Rectangle {
                      anchors.centerIn: parent
                      width: mapMarker.selected || mapMarker.searchResult || mapMarker.configured
                        ? Style.space(5) : Style.space(3)
                      height: width
                      radius: width / 2
                      color: mapMarker.selected || mapMarker.searchResult || mapMarker.configured
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                    }

                    MouseArea {
                      anchors.fill: parent
                      anchors.margins: -Style.space(5)
                      enabled: true
                      cursorShape: mapMarker.selectable
                        ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: {
                        if (mapMarker.selectable)
                          root.selectMapLocation(mapMarker.location)
                      }
                    }
                  }
                }
              }

              Rectangle {
                id: mapSelectionPin
                readonly property var projection: root.hasMapCoordinate(root.mapSelection)
                  ? mapCanvas.project(root.mapSelection.latitude, root.mapSelection.longitude)
                  : ({ x: 0, y: 0, visible: false })
                visible: root.mapSelection !== null && projection.visible
                z: 31
                x: projection.x - width / 2
                y: projection.y - height / 2
                width: Style.space(14)
                height: width
                radius: width / 2
                color: Style.selectedFillFor(root.contentForeground, Color.accent)
                border.width: 1
                border.color: Style.selectedStateColor(root.contentForeground, Color.accent)

                Rectangle {
                  anchors.centerIn: parent
                  width: Style.space(6)
                  height: width
                  radius: width / 2
                  color: Style.selectedStateColor(root.contentForeground, Color.accent)
                }
              }

              Rectangle {
                id: mapSelectionCard
                readonly property real edgeInset: Style.space(12)
                readonly property real topInset: Style.space(84)
                readonly property real pointGap: Style.space(22)
                readonly property var projection: root.hasMapCoordinate(root.mapSelection)
                  ? mapCanvas.project(root.mapSelection.latitude, root.mapSelection.longitude)
                  : ({ x: mapCanvas.width / 2, y: mapCanvas.height / 2, visible: false })
                readonly property real preferredX: root.mapSelectionCardOnRight
                  ? projection.x + pointGap : projection.x - width - pointGap
                visible: root.mode === "add" && root.mapSelection !== null
                  && !mapProcess.running
                z: 32
                width: Math.min(Style.space(252), parent.width - edgeInset * 2)
                height: mapSelectionContent.implicitHeight + Style.space(32)
                x: Math.max(edgeInset,
                  Math.min(parent.width - width - edgeInset, preferredX))
                y: Math.max(topInset,
                  Math.min(parent.height - height - edgeInset, projection.y - height / 2))
                radius: Math.max(Style.cornerRadius, Style.space(14))
                color: root.searchSurfaceColor
                border.width: Style.spacing.hairline
                border.color: root.mixColor(Color.background, root.contentForeground, 0.34)
                opacity: visible ? 1 : 0
                scale: visible ? 1 : 0.96
                transformOrigin: Item.Center

                Behavior on opacity {
                  NumberAnimation { duration: 160; easing.type: Easing.OutQuint }
                }
                Behavior on scale {
                  NumberAnimation { duration: 160; easing.type: Easing.OutQuint }
                }

                MouseArea {
                  anchors.fill: parent
                  acceptedButtons: Qt.LeftButton
                }

                Column {
                  id: mapSelectionContent
                  z: 1
                  anchors.left: parent.left
                  anchors.right: parent.right
                  anchors.leftMargin: Style.space(18)
                  anchors.rightMargin: Style.space(18)
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: Style.space(8)

                  Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: String(root.mapSelection && root.mapSelection.title || "")
                    color: root.contentForeground
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.heading
                    font.bold: true
                    elide: Text.ElideRight
                  }

                  Row {
                    width: parent.width
                    spacing: Style.space(8)

                    Text {
                      textFormat: Text.PlainText
                      id: mapSelectionTime
                      text: String(root.mapSelection && root.mapSelection.time || "--:--")
                      color: root.contentForeground
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.displayLarge
                      font.bold: true
                    }

                    Text {
                      textFormat: Text.PlainText
                      anchors.baseline: mapSelectionTime.baseline
                      text: String(root.mapSelection && root.mapSelection.notation || "")
                      color: root.contentForeground
                      opacity: 0.78
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.body
                      font.bold: true
                    }
                  }

                  Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: {
                      var day = String(root.mapSelection && root.mapSelection.day || "")
                      var relative = String(root.mapSelection
                        && root.mapSelection.relative_label || "")
                      return day && relative ? day + "  ·  " + relative : day || relative
                    }
                    color: root.contentForeground
                    opacity: 0.88
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.body
                    elide: Text.ElideRight
                  }

                  Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: String(root.mapSelection && root.mapSelection.timezone || "")
                    color: root.contentForeground
                    opacity: 0.66
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideMiddle
                  }

                  Rectangle {
                    width: parent.width
                    height: Style.spacing.hairline
                    color: root.contentForeground
                    opacity: 0.18
                  }

                  Button {
                    id: mapSelectionAddButton
                    width: parent.width
                    text: actionProcess.running && root.actionName === "add"
                      ? "Adding…" : "Add"
                    enabled: root.mapSelection !== null && !actionProcess.running
                      && root.canAddLocation(root.mapSelection.timezone)
                    active: true
                    bordered: true
                    focusable: true
                    foreground: root.contentForeground
                    fontFamily: root.contentFontFamily
                    fontSize: Style.font.title
                    horizontalPadding: Style.space(12)
                    verticalPadding: Style.space(8)
                    onClicked: root.runAction("add", root.mapSelection.timezone,
                      root.mapSelection)
                  }
                }
              }

              Rectangle {
                visible: mapProcess.running
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
                  textFormat: Text.PlainText
                  id: mapLookupLabel
                  anchors.centerIn: parent
                  text: "Locating…"
                  color: root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                }
              }

              Rectangle {
                visible: root.statusText !== ""
                  && !mapProcess.running
                  && !(root.mapSelection !== null
                    && actionProcess.running && root.actionName === "add")
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Style.space(12)
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(parent.width - Style.space(24),
                  Math.max(Style.space(180), addStatusLabel.implicitWidth + Style.space(28)))
                height: addStatusLabel.implicitHeight + Style.space(16)
                z: 25
                radius: height / 2
                color: root.mixColor(Color.background, root.contentForeground, 0.035)
                border.width: Style.spacing.hairline
                border.color: root.statusError
                  ? Color.urgent
                  : root.mixColor(Color.background, root.contentForeground, 0.24)

                Text {
                  textFormat: Text.PlainText
                  id: addStatusLabel
                  anchors.centerIn: parent
                  width: parent.width - Style.space(24)
                  horizontalAlignment: Text.AlignHCenter
                  wrapMode: Text.Wrap
                  text: root.statusText
                  color: root.statusError ? Color.urgent : root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                }
              }
            }
          }

          Text {
            textFormat: Text.PlainText
            visible: root.mode !== "add" && root.statusText !== ""
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
