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
  property bool globeInitialized: false
  property bool searchVisible: false

  readonly property var barIdentity: hostWidget || root
  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
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
    var seenTimezones = ({})
    for (var savedIndex = 0; savedIndex < mapClocks.length; savedIndex++) {
      var saved = mapClocks[savedIndex]
      var savedTimezone = String(saved.timezone || "")
      entries.push({ location: saved, configured: true })
      if (savedTimezone) seenTimezones[savedTimezone] = true
    }
    for (var cityIndex = 0; cityIndex < featuredCities.length; cityIndex++) {
      var city = featuredCities[cityIndex]
      var cityTimezone = String(city.timezone || "")
      if (!root.hasMapCoordinate(city) || seenTimezones[cityTimezone]) continue
      entries.push({ location: city, configured: false })
      if (cityTimezone) seenTimezones[cityTimezone] = true
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
    editorActive = false
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
    summaryFocusPending = false
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    searchVisible = false
    mapSelection = null
    mapClickPending = false
    globeInitialized = false
    controller.hide()
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

  function focusGlobeOn(location, zoomValue) {
    if (!location || !hasMapCoordinate(location) || mode !== "add") return
    mapCanvas.focusOn(location.latitude, location.longitude,
      zoomValue === undefined ? 1.95 : zoomValue)
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
      if (mode === "add") Qt.callLater(root.initializeGlobe)
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
    if (name === "add" && result && hasMapCoordinate(result))
      mapCanvas.focusOnLocations([result])
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
    searchProcess.command = [backendCommand, "search", query]
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
    var gap = Style.space(4)
    return left.x - gap < right.x + right.width
      && left.x + left.width + gap > right.x
      && left.y - gap < right.y + right.height
      && left.y + left.height + gap > right.y
  }

  function globeLabelWidth(location) {
    var titleLength = String(location && location.title || "").length
    var metadataLength = String(location
      && (location.time || location.timezone || location.subtitle) || "").length
    return Math.max(Style.space(68), Math.min(Style.space(148),
      Math.max(titleLength, metadataLength) * Style.spaceReal(7.1) + Style.space(18)))
  }

  function globeLabelLayout(targetIndex, width, height) {
    var placed = []
    var featuredPlaced = 0
    var target = {
      visible: false,
      labelVisible: false,
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      pointX: 0,
      pointY: 0,
      depth: -1
    }
    var labelHeight = Style.space(38)
    var edge = Style.space(8)
    var pointGap = Style.space(12)

    for (var i = 0; i <= targetIndex && i < mapLocations.length; i++) {
      var wrapper = mapLocations[i]
      var location = wrapper.location
      var projection = mapCanvas.project(location.latitude, location.longitude)
      var layout = {
        visible: false,
        labelVisible: false,
        x: 0,
        y: 0,
        width: globeLabelWidth(location),
        height: labelHeight,
        pointX: projection.x,
        pointY: projection.y,
        depth: projection.depth
      }
      if (!projection.visible) {
        if (i === targetIndex) target = layout
        continue
      }

      var mayPlaceLabel = wrapper.searchResult === true
        || wrapper.configured || featuredPlaced < 7
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
        if (!wrapper.configured) featuredPlaced++
      }
      layout.visible = wrapper.searchResult === true
        || wrapper.configured || layout.labelVisible
      if (i === targetIndex) target = layout
    }
    return target
  }

  function requestMapLocation(latitude, longitude, x, y) {
    if (!canAdd) return
    mapCursorX = x
    mapCursorY = y
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

  onOpenedChanged: if (opened) refresh()
  onTimeEditorActiveChanged: {
    if (!timeEditorActive && editorRefreshPending)
      Qt.callLater(root.flushEditorRefresh)
  }
  onModeChanged: {
    if (mode === "add") {
      searchVisible = false
      Qt.callLater(root.initializeGlobe)
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
      onCloseRequested: {
        if (root.mode === "add" && root.searchVisible) root.closeSearch()
        else if (root.mode === "read") root.close()
        else root.mode = "read"
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onReturnRequested: {
        if (root.mode === "read") root.focusSummaryEditor()
        else if (root.mode === "add") root.openSearch()
      }
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
                tooltipText: "Return to live time"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.returnToLive()
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
              text: root.currentLocationTitle
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

              Text {
              anchors.top: summaryInput.bottom
              anchors.topMargin: Style.space(7)
              anchors.horizontalCenter: parent.horizontalCenter
              text: root.currentTimezoneMetadata
                color: Qt.darker(root.contentForeground, 1.45)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                font.letterSpacing: 1.1
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
                      width: clockRow.cellWidth
                      height: clockRow.height

                      Rectangle {
                        id: clockSurface
                        anchors.fill: parent
                        radius: Style.cornerRadius
                        color: Style.normalFillFor(root.contentForeground, Color.accent)
                      }

                      Column {
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

                        Text {
                          width: parent.width
                          text: String(clockCell.clockData.day || "").toUpperCase()
                            + "  ·  "
                            + String(clockCell.clockData.relative_label || "").toUpperCase()
                          color: Qt.darker(root.contentForeground, 1.5)
                          font.family: root.contentFontFamily
                          font.pixelSize: Style.font.caption
                          font.letterSpacing: 0.6
                          elide: Text.ElideRight
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
                  root.closeSearch()
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
              background: root.mixColor(Color.popups.background,
                root.contentForeground, 0.025)
              bordered: true
              horizontalPadding: 0
              verticalPadding: 0
              onClicked: root.mode = "read"
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
              background: root.mixColor(Color.popups.background,
                root.contentForeground, 0.025)
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
              diameterRatio: 0.63
              oceanColor: root.mixColor(Color.background, root.contentForeground, 0.07)
              landColor: root.mixColor(Color.background, root.contentForeground, 0.68)
              boundaryColor: root.mixColor(Color.background, root.contentForeground, 0.40)
              rimColor: root.mixColor(Color.background, root.contentForeground, 0.28)
              onLocationPicked: function(latitude, longitude, viewX, viewY) {
                root.requestMapLocation(latitude, longitude, viewX, viewY)
              }

              Rectangle {
                id: searchResultOverlay
                visible: root.searchVisible && root.searchResults.length > 0
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
                  readonly property var layout:
                    root.globeLabelLayout(index, mapCanvas.width, mapCanvas.height)
                  anchors.fill: parent
                  visible: layout.visible
                  z: 4
                  opacity: Math.max(0.42, Math.min(1, 0.46 + layout.depth * 0.62))

                  Rectangle {
                    id: cityLabel
                    visible: mapMarker.layout.labelVisible
                    x: mapMarker.layout.x
                    y: mapMarker.layout.y
                    width: mapMarker.layout.width
                    height: mapMarker.layout.height
                    radius: Style.cornerRadius
                    color: cityMouse.containsMouse
                      ? Style.hoverFillFor(root.contentForeground, Color.accent)
                      : "transparent"

                    Column {
                      anchors.fill: parent
                      anchors.leftMargin: Style.space(6)
                      anchors.rightMargin: Style.space(6)
                      spacing: Style.space(1)

                      Text {
                        width: parent.width
                        text: mapMarker.location.title
                        horizontalAlignment: Text.AlignHCenter
                        color: root.contentForeground
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.bodySmall
                        font.bold: true
                        style: Text.Outline
                        styleColor: Color.background
                        elide: Text.ElideRight
                      }

                      Text {
                        width: parent.width
                        text: mapMarker.location.time
                          || mapMarker.location.timezone
                          || mapMarker.location.subtitle
                        horizontalAlignment: Text.AlignHCenter
                        color: Qt.darker(root.contentForeground, 1.35)
                        font.family: root.contentFontFamily
                        font.pixelSize: Style.font.caption
                        font.bold: mapMarker.configured
                        style: Text.Outline
                        styleColor: Color.background
                        elide: Text.ElideRight
                      }
                    }

                    MouseArea {
                      id: cityMouse
                      anchors.fill: parent
                      enabled: !mapMarker.configured
                        && root.canAddLocation(mapMarker.location.timezone)
                        && !actionProcess.running
                      hoverEnabled: true
                      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: root.runAction("add", mapMarker.location.timezone,
                        mapMarker.location)
                    }
                  }

                  Rectangle {
                    x: mapMarker.layout.pointX - width / 2
                    y: mapMarker.layout.pointY - height / 2
                    width: mapMarker.searchResult ? Style.space(12)
                      : (mapMarker.configured ? Style.space(11) : Style.space(9))
                    height: width
                    radius: width / 2
                    color: mapMarker.searchResult
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : mapMarker.configured
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : root.mixColor(Color.background, root.contentForeground, 0.22)
                    border.width: 1
                    border.color: mapMarker.searchResult
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : mapMarker.configured
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : root.mixColor(Color.background, root.contentForeground, 0.72)

                    Rectangle {
                      anchors.centerIn: parent
                      width: mapMarker.searchResult || mapMarker.configured
                        ? Style.space(5) : Style.space(3)
                      height: width
                      radius: width / 2
                      color: mapMarker.searchResult || mapMarker.configured
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                    }

                    MouseArea {
                      anchors.fill: parent
                      anchors.margins: -Style.space(5)
                      enabled: !mapMarker.configured
                        && root.canAddLocation(mapMarker.location.timezone)
                        && !actionProcess.running
                      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: root.runAction("add", mapMarker.location.timezone,
                        mapMarker.location)
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
