pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC
import Quickshell.Io
import qs.Commons
import qs.Ui
import "TimelineHoverState.js" as TimelineHoverState
import "TimeRail.js" as TimeRail

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
    summary: ({ timezone: "", label: "", title: "", time: "--:--", date: "", day: "", notation: "", local_minutes: 0, relative_minutes: 0, relative_label: "Same time" }),
    clocks: [],
    timeline: [],
    featured_cities: []
  })
  property bool snapshotLoaded: false
  property bool summaryFocusPending: false
  property bool summaryFocusScheduled: false
  property string summaryFocusSeed: ""
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
  property int searchResultIndex: -1
  property var mapSelection: null
  property bool mapSelectionActionFocusPending: false
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
  property var timelineHoverOwners: ({})
  property var scrubPayload: null
  property var scrubBaseSnapshot: null
  property string scrubSourceTimezone: ""
  property string scrubSourceTitle: ""
  property string scrubSourceKey: ""
  property string scrubActiveTimezone: ""
  property string scrubActiveLocationSignature: ""
  property bool scrubRequestPending: false
  property int scrubSelectedSlotIndex: -1
  property bool scrubPreviewActive: false
  property real scrubAnchorMinute: 0

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
  readonly property var timeline:
    TimeRail.buildMarkers(snapshot, scrubSourceTimezone, scrubViewportMinute)
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
    && !root.scrubPreviewActive
    && weather.disabled !== true
    && (root.weatherLoading || root.weatherLocations.length > 0)
  readonly property bool localDayRulersVisible:
    mode === "read" && (scrubPreviewActive || !live)
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
  readonly property bool scrubSourceIsSummary: !scrubSourceKey
    || scrubSourceKey === root.conversionSource(root.summary)
  readonly property bool canRemove: Number(snapshot.configured_count || 0) > 1
  readonly property bool localTimezoneConfigured: snapshot.local_configured === true
  readonly property int compactClockColumns: panelScroll.width >= Style.space(780)
    ? 5 : (panelScroll.width >= Style.space(620) ? 4 : 3)
  readonly property real comfortableClockGridHeight: {
    var rows = Math.ceil(clocks.length / 3)
    return rows <= 0 ? 0
      : rows * Style.space(104) + (rows - 1) * Style.space(14)
  }
  readonly property real comfortableRequiredHeight: panelHeader.height
    + panelColumn.spacing + Style.space(92)
    + (timelineView.visible ? Style.space(18) + Style.space(128) : 0)
    + (clocks.length > 0 ? Style.space(18) + comfortableClockGridHeight : 0)
  readonly property real readHeightLimit: {
    var available = Number(panel.availableCardHeight || 0)
    var cap = Style.space(680)
    var outer = available > 0 ? Math.min(available, cap) : cap
    return Math.max(0, outer - panel.verticalContentInset)
  }
  readonly property bool autoCompactDensity: clocks.length > 0
    && comfortableRequiredHeight > readHeightLimit + Style.space(1)
  readonly property bool compactDensity:
    (mode === "read" || mode === "edit") && autoCompactDensity
  readonly property int clockColumnCount: compactDensity ? compactClockColumns : 3
  readonly property real clockRowHeight: Style.space(compactDensity ? 92 : 104)
  readonly property real clockRowSpacing: Style.space(compactDensity ? 8 : 14)
  readonly property real clockGridHeight: {
    var rows = Math.ceil(clocks.length / clockColumnCount)
    if (rows <= 0) return 0
    return rows * clockRowHeight + (rows - 1) * clockRowSpacing
  }
  readonly property real readChromeHeight: panelHeader.height
    + panelColumn.spacing + summaryClock.height
    + (timelineView.visible ? readPage.spacing + timelineView.height : 0)
    + (clocks.length > 0 ? readPage.spacing : 0)
  readonly property real clockViewportHeight: clocks.length <= 0 ? 0
    : Math.min(clockGridHeight, Math.max(0, readHeightLimit - readChromeHeight))
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
  readonly property int maximumSavedGlobeMarkers: 48
  readonly property var globeLocations: {
    var entries = []
    var seenPlaces = ({})
    var preferredSavedMarkers = []
    var remainingSavedMarkers = []
    var summaryIdentity = root.conversionSource(summary)
    for (var savedIndex = 0; savedIndex < mapClocks.length; savedIndex++) {
      var saved = mapClocks[savedIndex]
      var savedKey = root.mapLocationKey(saved)
      if (savedKey) seenPlaces[savedKey] = true
      var wrapper = { location: saved, configured: true }
      if (root.conversionSource(saved) === summaryIdentity || saved.pinned === true)
        preferredSavedMarkers.push(wrapper)
      else
        remainingSavedMarkers.push(wrapper)
    }
    var savedMarkers = preferredSavedMarkers.concat(remainingSavedMarkers)
      .slice(0, maximumSavedGlobeMarkers)
    entries = entries.concat(savedMarkers)
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
      var focusLocation = mapFocusLocation(result)
      if (focusLocation)
        entries.push({ location: focusLocation, selection: result,
          configured: false, searchResult: true })
    }
    return entries
  }
  readonly property var mapLocations: searchHasQuery
    ? searchMapLocations : globeLocations
  readonly property var scrubSlots: scrubPayload && Array.isArray(scrubPayload.slots)
    ? scrubPayload.slots : []
  readonly property var scrubSelectedFrame:
    scrubSelectedSlotIndex >= 0 && scrubSelectedSlotIndex < scrubSlots.length
      ? scrubSlots[scrubSelectedSlotIndex] : null
  readonly property real scrubViewportMinute: {
    if (scrubSelectedFrame && (scrubPreviewActive || !live))
      return Number(scrubSelectedFrame.minute || 0)
    return scrubAnchorMinute
  }
  readonly property bool scrubLoading: scrubProcess.running
    && scrubActiveTimezone === scrubSourceTimezone
  readonly property bool scrubReady: {
    return TimeRail.scrubPayloadReady(scrubPayload, snapshot,
      scrubBaseSnapshot, scrubSourceTimezone, scrubSourceKey)
  }
  readonly property var scrubAxisTicks:
    TimeRail.axisTicks(scrubViewportMinute, String(snapshot.time_format || "24h"))

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

  function focusSummaryEditor(initialText) {
    if (initialText !== undefined)
      summaryFocusSeed += String(initialText || "")
    if (mode !== "read") {
      summaryFocusPending = false
      summaryFocusScheduled = false
      summaryFocusSeed = ""
      return
    }
    if (!snapshotLoaded) {
      summaryFocusPending = true
      return
    }
    summaryFocusPending = false
    if (summaryFocusScheduled) return
    summaryFocusScheduled = true
    // Return is emitted while PanelKeyCatcher is still dispatching the key.
    // Hand focus over on the next event-loop turn so the catcher cannot take
    // it straight back, then select the live time for immediate replacement.
    // A digit typed into the unfocused panel has already been consumed by the
    // catcher, so apply it after focus moves to preserve that first character.
    Qt.callLater(function() {
      summaryFocusScheduled = false
      if (!opened || mode !== "read") {
        summaryFocusSeed = ""
        return
      }
      var seed = summaryFocusSeed
      summaryFocusSeed = ""
      summaryInput.forceActiveFocus(Qt.ShortcutFocusReason)
      summaryInput.selectAll()
      if (seed) {
        summaryInput.text = seed
        summaryInput.cursorPosition = summaryInput.text.length
        root.timeInputEdited(summaryInput.conversionSource)
      }
    })
  }

  function restoreReadModeFocus() {
    if (opened && mode === "read")
      keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
  }

  function isLetterKey(text) {
    var value = String(text || "")
    return value.length === 1 && value.toLowerCase() !== value.toUpperCase()
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
      if (!opened || mode !== "add" || !searchVisible) return
      addField.forceActiveFocus(Qt.ShortcutFocusReason)
      if (selectExisting === false)
        addField.cursorPosition = addField.text.length
      else
        addField.selectAll()
    })
  }

  function openSearch(initialText) {
    if (mode !== "add") return
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
    searchResultIndex = -1
    addField.text = ""
    mapSelection = null
    mapSelectionActionFocusPending = false
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
    cancelScrubPreview()
    mode = "read"
    globeDetailRequested = false
    summaryFocusPending = false
    summaryFocusScheduled = false
    summaryFocusSeed = ""
    searchResults = []
    searchResultsQuery = ""
    searchSubmitQuery = ""
    searchResultIndex = -1
    searchVisible = false
    mapSelection = null
    mapSelectionActionFocusPending = false
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
    var row = clockRows.itemAtIndex(
      Math.floor(normalizedIndex / root.clockColumnCount))
    return row ? row.cellAt(normalizedIndex % root.clockColumnCount) : null
  }

  function positionClockRow(clockIndex) {
    var normalizedIndex = Number(clockIndex)
    if (!isFinite(normalizedIndex) || normalizedIndex < 0
        || normalizedIndex >= clocks.length) return false
    clockRows.positionViewAtIndex(
      Math.floor(normalizedIndex / root.clockColumnCount), ListView.Contain)
    return true
  }

  function ensureKeyboardCursorVisible() {
    if (!keyboardCursorActive || mode === "add") return
    if (keyboardClockIndex >= 0) {
      positionClockRow(keyboardClockIndex)
      return
    }
    var target = summaryInput
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
      nextIndex = nextIndex < 0 ? 0
        : Math.min(count - 1, nextIndex + clockColumnCount)
    else if (Number(dy) < 0)
      nextIndex = nextIndex < clockColumnCount
        ? -1 : nextIndex - clockColumnCount
    else if (Number(dx) > 0)
      nextIndex = nextIndex < 0 ? 0 : Math.min(count - 1, nextIndex + 1)
    else if (Number(dx) < 0)
      nextIndex = nextIndex < 0 ? count - 1 : Math.max(0, nextIndex - 1)
    keyboardClockIndex = nextIndex
    Qt.callLater(root.ensureKeyboardCursorVisible)
  }

  function focusClockEditor(clockIndex, retryCount) {
    if (!positionClockRow(clockIndex)) return
    Qt.callLater(function() {
      if (!opened || (mode !== "read" && mode !== "edit")) return
      var cell = root.clockCellAt(clockIndex)
      if (!cell) {
        if (Number(retryCount || 0) < 1) root.focusClockEditor(clockIndex, 1)
        return
      }
      if (mode === "read") cell.focusTimeEditor()
      else cell.focusLabelEditor(Qt.ShortcutFocusReason)
    })
  }

  function focusSummaryLabelEditor() {
    if (!opened || mode !== "edit" || !localTimezoneConfigured) return
    headerTitle.beginLabelEdit(Qt.ShortcutFocusReason)
  }

  function toggleEditMode() {
    if (mode === "add" || actionProcess.running) return false
    keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
    if (mode === "edit") {
      mode = "read"
      return true
    }
    var hadCursor = keyboardCursorActive
    mode = "edit"
    keyboardCursorActive = true
    if (!hadCursor || keyboardClockIndex < -1 || keyboardClockIndex >= clocks.length)
      keyboardClockIndex = -1
    Qt.callLater(root.ensureKeyboardCursorVisible)
    return true
  }

  function toggleKeyboardCursorPin() {
    if (mode !== "edit" || !keyboardCursorActive || actionProcess.running)
      return false
    var clock = null
    if (keyboardClockIndex < 0) {
      if (!localTimezoneConfigured) return false
      clock = summary
    } else if (keyboardClockIndex < clocks.length) {
      clock = clocks[keyboardClockIndex]
    }
    if (!clock) return false
    togglePin(clock)
    return true
  }

  function activateKeyboardCursor() {
    if (mode === "add") {
      if (mapSelection !== null) addMapSelection()
      else openSearch()
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

  function scrubLocationSignature() {
    if (!snapshotLoaded) return ""
    var entries = [summary].concat(clocks)
    var values = []
    for (var index = 0; index < entries.length; index++)
      values.push(conversionSource(entries[index]))
    return values.join("\u001e")
  }

  function clockForScrubSource() {
    var entries = [summary].concat(clocks)
    for (var index = 0; index < entries.length; index++)
      if (conversionSource(entries[index]) === scrubSourceKey) return entries[index]
    for (var timezoneIndex = 0; timezoneIndex < entries.length; timezoneIndex++)
      if (String(entries[timezoneIndex].timezone || "") === scrubSourceTimezone)
        return entries[timezoneIndex]
    return summary
  }

  function clockDayLabel(clock) {
    var liveLabel = String(clock && clock.day || "")
    if (root.live && !root.scrubPreviewActive) return liveLabel
    return TimeRail.relativeDayLabel(clock, root.clockForScrubSource()) || liveLabel
  }

  function nearestScrubSlot(clock) {
    var minute = Number(clock && clock.local_minutes)
    if (!isFinite(minute)) minute = 0
    if (scrubPayload) return TimeRail.slotIndexFor(scrubPayload, 0, minute)
    return Math.max(0, Math.min(95, Math.round(minute / 15)))
  }

  function requestScrubFor(clock) {
    if (!snapshotLoaded || !clock) return
    var timezone = String(clock.timezone || "").trim()
    if (!timezone) return
    scrubSourceTimezone = timezone
    scrubSourceTitle = String(clock.title || clock.label || timezone)
    scrubSourceKey = conversionSource(clock)
    var date = String(clock.date || "")
    var timeFormat = String(snapshot.time_format || "24h")
    if (scrubPayload && scrubPayload.source_timezone === timezone
        && String(scrubPayload.date || "") === date
        && String(scrubPayload.time_format || "") === timeFormat
        && TimeRail.payloadMatchesSnapshot(scrubPayload, snapshot)) {
      if (scrubSelectedSlotIndex < 0)
        scrubSelectedSlotIndex = nearestScrubSlot(clock)
      return
    }
    if (scrubProcess.running) {
      scrubRequestPending = true
      return
    }
    startScrubRequest()
  }

  function startScrubRequest() {
    if (!snapshotLoaded || scrubProcess.running || !scrubSourceTimezone) return
    var reference = String(snapshot.reference_utc || "").trim()
    if (!reference) return
    scrubRequestPending = false
    scrubActiveTimezone = scrubSourceTimezone
    scrubActiveLocationSignature = scrubLocationSignature()
    scrubProcess.command = [
      backendCommand,
      "scrub",
      "--timezone", scrubActiveTimezone,
      "--at", reference
    ]
    scrubProcess.running = true
  }

  function flushScrubRequest() {
    if (scrubProcess.running) return
    if (!scrubRequestPending) return
    scrubRequestPending = false
    var clock = clockForScrubSource()
    requestScrubFor(clock)
  }

  function selectScrubSource(clock) {
    if (!clock || mode === "add") return
    cancelScrubPreview()
    scrubAnchorMinute = Number(clock.local_minutes || 0)
    scrubSelectedSlotIndex = -1
    requestScrubFor(clock)
  }

  function ensureScrubSource() {
    if (!snapshotLoaded || mode === "add") return
    var clock = clockForScrubSource()
    if (!scrubSourceTimezone || !clock
        || String(clock.timezone || "") !== scrubSourceTimezone)
      clock = summary
    requestScrubFor(clock)
    if (!scrubPreviewActive) {
      scrubAnchorMinute = Number(clock.local_minutes || 0)
      scrubSelectedSlotIndex = nearestScrubSlot(clock)
    }
  }

  function applyScrubSlot(slotIndex) {
    if (!scrubReady || scrubSlots.length === 0) return
    var index = Math.max(0, Math.min(scrubSlots.length - 1, Number(slotIndex)))
    if (scrubPreviewActive && scrubSelectedSlotIndex === index) return
    var frame = scrubSlots[index]
    if (!scrubPreviewActive) {
      var resumeSnapshotRequest = snapshotRequestPending
        || (snapshotProcess.running
          && snapshotActiveGeneration === snapshotStateGeneration)
      var resumeSnapshotReference = snapshotRequestPending
        ? snapshotRequestReference : snapshotActiveReference
      invalidateSnapshotRequests()
      if (resumeSnapshotRequest) {
        snapshotRequestPending = true
        snapshotRequestReference = resumeSnapshotReference
      }
      scrubBaseSnapshot = snapshot
    }
    scrubSelectedSlotIndex = index
    scrubPreviewActive = true
    if (!frame || !frame.reference_utc) {
      snapshot = scrubBaseSnapshot
      summaryInput.text = String(snapshot.summary.time || "--:--")
      return
    }
    var merged = TimeRail.mergeSnapshot(scrubBaseSnapshot, scrubPayload, index)
    if (!merged) {
      cancelScrubPreview()
      scrubPayload = null
      requestScrubFor(clockForScrubSource())
      return
    }
    snapshot = merged
    summaryInput.text = String(merged.summary.time || "--:--")
    invalidConversionSource = ""
  }

  function cancelScrubPreview() {
    if (scrubBaseSnapshot) {
      snapshot = scrubBaseSnapshot
      summaryInput.text = String(snapshot.summary.time || "--:--")
    }
    scrubBaseSnapshot = null
    scrubPreviewActive = false
    if (snapshotLoaded) {
      var clock = clockForScrubSource()
      scrubAnchorMinute = Number(clock.local_minutes || 0)
      scrubSelectedSlotIndex = nearestScrubSlot(clock)
    }
    Qt.callLater(root.flushSnapshotRequest)
  }

  function dismissTransientTime() {
    if (scrubPreviewActive) {
      cancelScrubPreview()
      return true
    }
    if (!live) {
      returnToLive()
      return true
    }
    return false
  }

  function lockScrubSelection() {
    if (!scrubReady || !scrubSelectedFrame) {
      if (scrubPreviewActive) cancelScrubPreview()
      return
    }
    if (!scrubSelectedFrame.reference_utc) {
      cancelScrubPreview()
      return
    }
    var base = scrubBaseSnapshot || snapshot
    var merged = TimeRail.mergeSnapshot(base, scrubPayload, scrubSelectedSlotIndex)
    if (!merged) {
      cancelScrubPreview()
      return
    }
    snapshot = merged
    summaryInput.text = String(merged.summary.time || "--:--")
    scrubBaseSnapshot = null
    scrubPreviewActive = false
    invalidateSnapshotRequests()
    live = false
    requestSnapshot(String(scrubSelectedFrame.reference_utc))
  }

  function moveScrubSelection(delta) {
    if (!scrubReady) return
    var current = scrubSelectedSlotIndex >= 0
      ? scrubSelectedSlotIndex : nearestScrubSlot(clockForScrubSource())
    var next = Math.max(0, Math.min(scrubSlots.length - 1,
      current + Number(delta)))
    applyScrubSlot(next)
  }

  function focusTimeRail() {
    if (mode !== "read" || !scrubReady) return
    keyboardCursorActive = false
    keyboardClockIndex = -1
    panelScroll.contentY = 0
    railMouse.forceActiveFocus(Qt.ShortcutFocusReason)
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

  function weatherGlyphColor(item) {
    var muted = Qt.darker(root.contentForeground, 1.45)
    if (!item) return muted
    var code = Number(item.weather_code)
    if (item.is_day === false)
      return mixColor(muted, Qt.rgba(0.50, 0.60, 0.90, 1), 0.66)
    if (code === 0 || code === 1 || code === 2)
      return mixColor(muted, Qt.rgba(0.96, 0.72, 0.27, 1), 0.68)
    if (code === 45 || code === 48 || code === 3)
      return mixColor(muted, Qt.rgba(0.58, 0.68, 0.73, 1), 0.52)
    if (code === 51 || code === 53 || code === 55 || code === 56
        || code === 57 || code === 61 || code === 63 || code === 65
        || code === 66 || code === 67 || code === 80 || code === 81
        || code === 82)
      return mixColor(muted, Qt.rgba(0.35, 0.66, 0.84, 1), 0.64)
    if (code === 71 || code === 73 || code === 75 || code === 77
        || code === 85 || code === 86)
      return mixColor(muted, Qt.rgba(0.72, 0.86, 0.92, 1), 0.58)
    if (code === 95 || code === 96 || code === 99)
      return mixColor(muted, Qt.rgba(0.66, 0.48, 0.84, 1), 0.64)
    return muted
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
    if (!opened || !snapshotLoaded || !live || scrubPreviewActive) return
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

  function cancelTimeEditor(input, currentTime, source) {
    if (!input) return
    input.text = String(currentTime || "--:--")
    timeInputEdited(source)
    keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
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
      Qt.callLater(root.ensureScrubSource)
      requestWeather(false)
      if (mode === "add") Qt.callLater(root.initializeGlobe)
    } catch (error) {
      setStatus("World Clock backend returned invalid data.", true)
    }
  }

  function requestSnapshot(referenceUtc) {
    var reference = String(referenceUtc || "")
    if (scrubPreviewActive) {
      snapshotRequestPending = true
      snapshotRequestReference = reference
      return
    }
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
    if (!live) return
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
    cancelScrubPreview()
    invalidateSnapshotRequests()
    invalidConversionSource = ""
    live = true
    keyCatcher.forceActiveFocus(Qt.MouseFocusReason)
    requestLiveSnapshot()
    requestWeather(false)
  }

  function resetTimeOnPanelClose() {
    cancelScrubPreview()
    invalidateSnapshotRequests()
    if (live) return
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

  function runAction(name, timezone, result, value) {
    if (actionProcess.running) return false
    var focusLocation = mapFocusLocation(result)
    if (name === "add" && focusLocation)
      mapCanvas.focusOnLocations([focusLocation])
    actionName = name
    var command = [backendCommand, name]
    command.push(String(timezone || ""))
    if ((name === "add" || name === "pin" || name === "unpin" || name === "remove"
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

  function scheduleSearch() {
    searchDebounce.restart()
  }

  function searchTextChanged() {
    mapSelection = null
    mapSelectionActionFocusPending = false
    mapClickPending = false
    searchResultIndex = -1
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

  function focusSearchResult(index) {
    if (mode !== "add" || !searchVisible || searchResults.length === 0)
      return false
    var boundedIndex = Math.max(0, Math.min(searchResults.length - 1,
      Math.round(Number(index))))
    if (!isFinite(boundedIndex)) boundedIndex = 0
    searchResultIndex = boundedIndex
    mapSelection = null
    mapSelectionActionFocusPending = false
    var result = searchResults[searchResultIndex]
    var focusLocation = mapFocusLocation(result)
    if (focusLocation) mapCanvas.focusOnLocations([focusLocation])
    Qt.callLater(function() {
      if (root.searchVisible && root.searchResultIndex === boundedIndex)
        searchResultList.positionViewAtIndex(boundedIndex, ListView.Contain)
    })
    return true
  }

  function moveSearchResultSelection(direction) {
    if (searchResults.length === 0) return false
    var currentIndex = searchResultIndex >= 0 ? searchResultIndex : 0
    return focusSearchResult(currentIndex + Number(direction || 0))
  }

  function addMapSelection() {
    if (mode !== "add" || mapSelection === null || actionProcess.running)
      return false
    return root.runAction("add", root.mapSelection.timezone,
      root.mapSelection)
  }

  function completeMapSelectionActionFocus() {
    if (!mapSelectionActionFocusPending) return
    if (!opened || mode !== "add" || mapSelection === null) {
      mapSelectionActionFocusPending = false
      return
    }
    if (!mapSelectionCard.visible || !mapSelectionAddButton.enabled) return
    mapSelectionActionFocusPending = false
    mapSelectionAddButton.forceActiveFocus(Qt.ShortcutFocusReason)
  }

  function focusMapSelectionAction() {
    mapSelectionActionFocusPending = true
    Qt.callLater(root.completeMapSelectionActionFocus)
  }

  function acceptSearchResult() {
    var query = String(addField.text || "").trim()
    if (mode !== "add" || !searchVisible || !query
        || actionProcess.running) return
    if (mapSelection !== null) {
      addMapSelection()
      return
    }
    if (searchResultsQuery === query) {
      if (searchResults.length > 0) {
        var activeIndex = searchResultIndex >= 0 ? searchResultIndex : 0
        focusSearchResult(activeIndex)
        selectMapLocation(searchResults[activeIndex])
        focusMapSelectionAction()
      }
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

  function mapFocusLocation(entry) {
    if (!entry) return null
    var usePrimaryCoordinate = hasMapCoordinate(entry)
    var latitudeValue = usePrimaryCoordinate
      ? entry.latitude : entry.focus_latitude
    var longitudeValue = usePrimaryCoordinate
      ? entry.longitude : entry.focus_longitude
    if (latitudeValue === null || latitudeValue === undefined
        || longitudeValue === null || longitudeValue === undefined) return null
    var latitude = Number(latitudeValue)
    var longitude = Number(longitudeValue)
    if (!isFinite(latitude) || !isFinite(longitude)
        || latitude < -90 || latitude > 90
        || longitude < -180 || longitude > 180) return null
    return {
      timezone: entry.timezone,
      title: entry.title,
      subtitle: entry.subtitle,
      latitude: latitude,
      longitude: longitude,
      time: entry.time,
      day: entry.day,
      notation: entry.notation,
      relative_label: entry.relative_label
    }
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
    if (!location) return
    mapSelectionActionFocusPending = false
    var focusLocation = mapFocusLocation(location)
    if (focusLocation) {
      var projection = mapCanvas.project(focusLocation.latitude,
        focusLocation.longitude)
      mapSelectionCardOnRight = projection.x < mapCanvas.width / 2
    }
    mapClickPending = false
    mapSelection = location
    clearStatus()
    if (focusLocation) mapCanvas.focusOnLocations([focusLocation])
  }

  function dismissMapSelection() {
    if (mapSelection === null) return
    mapSelection = null
    mapSelectionActionFocusPending = false
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

  function timelineHoverMatches(clock) {
    return TimelineHoverState.matchesIdentity(
      timelineHoverOwners, conversionSource(clock))
  }

  function timelineMarkerHovered(marker) {
    return TimelineHoverState.markerHovered(timelineHoverOwners, marker)
  }

  function updateTimelineHover(owner, clock, hovered) {
    timelineHoverOwners = TimelineHoverState.updateOwners(
      timelineHoverOwners, owner, conversionSource(clock), hovered)
  }

  function clearTimelineHover() {
    timelineHoverOwners = ({})
  }

  onOpenedChanged: {
    if (!opened) {
      globeDetailRequested = false
      resetTimeOnPanelClose()
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
    clearStatus()
    if (mode === "add") {
      cancelScrubPreview()
      clearTimelineHover()
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
      if (mode === "read") Qt.callLater(root.restoreReadModeFocus)
    }
  }
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
    id: scrubProcess
    stdout: StdioCollector { id: scrubOutput; waitForEnd: true }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function(exitCode) {
      var current = root.scrubActiveTimezone === root.scrubSourceTimezone
        && root.scrubActiveLocationSignature === root.scrubLocationSignature()
      if (exitCode === 0 && current) {
        try {
          var payload = JSON.parse(String(scrubOutput.text || ""))
          if (!payload || Number(payload.schema_version) !== 1
              || payload.source_timezone !== root.scrubActiveTimezone
              || (payload.time_format !== "24h" && payload.time_format !== "ampm")
              || !TimeRail.payloadMatchesSnapshot(payload, root.snapshot)
              || Number(payload.first_day_offset) > -1
              || Number(payload.day_count) < 3
              || !Array.isArray(payload.slots) || payload.slots.length === 0)
            throw new Error("Unsupported time rail response")
          root.scrubPayload = payload
          var sourceClock = root.clockForScrubSource()
          root.scrubAnchorMinute = Number(sourceClock.local_minutes || 0)
          root.scrubSelectedSlotIndex = root.nearestScrubSlot(sourceClock)
        } catch (error) {
          root.scrubPayload = null
          root.setStatus("The time rail could not be prepared.", true)
        }
      } else if (exitCode !== 0 && current) {
        root.scrubPayload = null
        root.setStatus("The time rail is unavailable.", true)
      }
      root.scrubActiveTimezone = ""
      root.scrubActiveLocationSignature = ""
      Qt.callLater(root.flushScrubRequest)
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
        if (root.actionName === "add" && root.mapSelection !== null)
          root.focusMapSelectionAction()
        return
      }
      if (root.actionName === "add") {
        addField.text = ""
        root.searchResults = []
        root.searchResultsQuery = ""
        root.searchSubmitQuery = ""
        root.mapSelection = null
        root.mapSelectionActionFocusPending = false
        root.mapClickPending = false
        root.mode = "read"
        root.editorActive = false
        // The search field owned focus when Add was submitted. It disappears
        // with the mode change, so explicitly return focus to the read-mode
        // dispatcher before the user begins another quick entry.
        Qt.callLater(root.restoreReadModeFocus)
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
          root.focusSearchResult(0)
          if (root.mode === "add"
              && root.searchSubmitQuery === root.searchResultsQuery) {
            root.searchSubmitQuery = ""
            root.selectMapLocation(root.searchResults[0])
            root.focusMapSelectionAction()
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
      directTextInput: (root.mode === "read" || root.mode === "add")
        && !addField.activeFocus
      onMoveRequested: function(dx, dy) { root.moveKeyboardCursor(dx, dy) }
      onActivateRequested: root.activateKeyboardCursor()
      onDeleteRequested: root.deleteKeyboardCursor()
      onEditRequested: root.toggleEditMode()
      onLiveRequested: root.returnToLive()
      onTimelineRequested: root.focusTimeRail()
      onCloseRequested: {
        if (root.mapSelection !== null) root.dismissMapSelection()
        else if (root.mode === "add" && root.searchVisible) root.closeSearch()
        else if (root.mode === "read") {
          if (!root.dismissTransientTime()) root.close()
        }
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
        if (root.mode === "edit") {
          if (text === "p" || text === "P") root.toggleKeyboardCursorPin()
          else if (text === "e" || text === "E") root.toggleEditMode()
          return
        }
        if (root.mode === "read" && /^[0-9]$/.test(text)) {
          root.focusSummaryEditor(text)
          return
        }
        if (root.mode === "read" && root.isLetterKey(text)) {
          root.mode = "add"
          root.openSearch(text)
          return
        }
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
            id: panelHeader
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
                active: !root.live || root.scrubPreviewActive
                enabled: !actionProcess.running
                tooltipText: "Return to live time (Home)"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.returnToLive()
              }

              Button {
                id: weatherProviderAttribution
                anchors.verticalCenter: parent.verticalCenter
                visible: root.mode !== "add" && root.weatherEnabled && root.live
                  && !root.scrubPreviewActive
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
                tooltipText: "Remove this time from the bar (P in edit mode)"
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
                enabled: !actionProcess.running
                tooltipText: "Add a location"
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
                  ? "Finish editing (F2)" : "Rename, pin, or remove locations (F2)"
                horizontalPadding: Style.space(8)
                verticalPadding: Style.space(5)
                onClicked: root.toggleEditMode()
              }
            }
          }

          Column {
            id: readPage
            visible: root.mode !== "add"
            width: parent.width
            spacing: Style.space(18)

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
                Keys.onEscapePressed: function(event) {
                  root.cancelTimeEditor(summaryInput, root.summary.time,
                    conversionSource)
                  event.accepted = true
                }
                onActiveFocusChanged: {
                  root.editorActive = activeFocus
                  root.timeEditorActive = activeFocus
                  if (activeFocus) root.selectScrubSource(root.summary)
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
                  width: Math.max(summaryWeatherContent.implicitWidth,
                    summaryWeatherSkeleton.implicitWidth)
                  height: Style.space(16)

                  Row {
                    id: summaryWeatherContent
                    visible: summaryClock.weatherData || !root.weatherLoading
                    anchors.centerIn: parent
                    spacing: summaryClock.weatherData ? Style.space(6) : 0

                    Text {
                      textFormat: Text.PlainText
                      id: summaryWeatherGlyph
                      anchors.verticalCenter: parent.verticalCenter
                      text: root.weatherGlyph(summaryClock.weatherData)
                      color: root.weatherGlyphColor(summaryClock.weatherData)
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.title
                    }

                    Text {
                      textFormat: Text.PlainText
                      id: summaryWeatherTemperature
                      anchors.verticalCenter: parent.verticalCenter
                      text: root.weatherText(summaryClock.weatherData)
                      color: Qt.darker(root.contentForeground, 1.45)
                      font.family: root.contentFontFamily
                      font.pixelSize: Style.font.bodySmall
                      font.letterSpacing: 0.3
                    }
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
              visible: root.clocks.length > 0
              width: parent.width
              height: visible ? Style.space(128) : 0
              readonly property real railInset: Style.space(54)
              readonly property real railWidth: Math.max(1, width - railInset * 2)
              readonly property real railY: Style.space(70)
              readonly property bool interacting:
                root.scrubPreviewActive || railMouse.activeFocus || !root.live
              readonly property real selectedX: railInset + railWidth / 2
              readonly property bool sourceMarkerHovered:
                TimelineHoverState.sourceMarkerHovered(
                  root.timelineHoverOwners, root.timeline)
              readonly property bool selectedUnavailable: root.scrubSelectedFrame
                && !root.scrubSelectedFrame.reference_utc

              Text {
                textFormat: Text.PlainText
                visible: root.scrubLoading || !root.scrubSourceIsSummary
                anchors.left: parent.left
                anchors.leftMargin: timelineView.railInset
                anchors.right: parent.right
                anchors.rightMargin: timelineView.railInset
                anchors.top: parent.top
                elide: Text.ElideRight
                text: root.scrubLoading
                  ? "PREPARING " + String(root.scrubSourceTitle || "TIME RAIL").toUpperCase()
                  : "FROM " + String(root.scrubSourceTitle || root.currentLocationTitle).toUpperCase()
                color: Qt.darker(root.contentForeground, 1.55)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
                font.letterSpacing: 0.6
              }

              Rectangle {
                x: timelineView.railInset
                width: timelineView.railWidth
                y: timelineView.railY
                height: Style.spacing.hairline
                color: root.contentForeground
                opacity: root.scrubReady ? 0.26 : 0.12
              }

              Repeater {
                id: timelineTickRepeater
                model: root.scrubAxisTicks

                Item {
                  id: axisTick
                  required property int index
                  required property var modelData
                  x: timelineView.railInset
                    + Number(modelData.position || 0) * timelineView.railWidth
                  width: Style.spacing.hairline
                  height: timelineView.height

                  Rectangle {
                    y: timelineView.railY - height / 2
                    width: parent.width
                    height: axisTick.modelData.major ? Style.space(10) : Style.space(6)
                    color: root.contentForeground
                    opacity: axisTick.modelData.major ? 0.22 : 0.12
                  }

                  Text {
                    textFormat: Text.PlainText
                    visible: axisTick.modelData.major === true
                    width: Style.space(34)
                    x: -width / 2
                    y: timelineView.railY + Style.space(34)
                    horizontalAlignment: Text.AlignHCenter
                    text: String(axisTick.modelData.label || "")
                    color: Qt.darker(root.contentForeground, 1.65)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                    font.letterSpacing: 0.4
                  }
                }
              }

              Repeater {
                model: root.timeline

                Item {
                  id: timelinePoint
                  required property int index
                  required property var modelData
                  readonly property bool sourcePoint: modelData.source === true
                  readonly property string overflowDirection:
                    String(modelData.overflow || "")
                  readonly property bool overflowPoint: overflowDirection !== ""
                  readonly property bool linkedHovered:
                    root.timelineMarkerHovered(modelData)
                  readonly property bool upperLane: Number(modelData.lane || 0) === 0
                  readonly property real markerCenterX:
                    overflowDirection === "previous"
                      ? timelineView.railInset - Style.space(18)
                      : overflowDirection === "next"
                      ? timelineView.railInset + timelineView.railWidth + Style.space(18)
                      : timelineView.railInset
                        + Number(modelData.position || 0) * timelineView.railWidth
                  readonly property real markerLocalX: markerCenterX - x
                  width: Style.space(overflowPoint ? 160 : 72)
                  height: parent.height
                  x: overflowDirection === "previous" ? 0
                    : overflowDirection === "next" ? timelineView.width - width
                    : Math.max(0, Math.min(timelineView.width - width,
                      markerCenterX - width / 2))

                  Rectangle {
                    id: markerStem
                    visible: !(timelinePoint.sourcePoint && scrubPlayhead.visible)
                    x: Math.round(timelinePoint.markerLocalX - width / 2)
                    y: timelinePoint.upperLane
                      ? markerHalo.y - height : markerHalo.y + markerHalo.height
                    width: Style.spacing.hairline
                    height: Style.space(7)
                    color: root.contentForeground
                    opacity: timelinePoint.linkedHovered ? 0.48 : 0.24

                    Behavior on opacity {
                      NumberAnimation { duration: 160; easing.type: Easing.OutQuart }
                    }
                  }

                  Rectangle {
                    id: markerHalo
                    visible: !(timelinePoint.sourcePoint && scrubPlayhead.visible)
                    x: Math.round(timelinePoint.markerLocalX - width / 2)
                    y: timelineView.railY - height / 2
                    width: Style.space(timelinePoint.sourcePoint
                      ? 11 : (Number(timelinePoint.modelData.count || 1) > 1 ? 9 : 7))
                    height: width
                    radius: width / 2
                    scale: timelinePoint.linkedHovered ? 1.45 : 1
                    color: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                      ? Style.selectedFillFor(root.contentForeground, Color.accent)
                      : "transparent"
                    border.width: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                      || Number(timelinePoint.modelData.count || 1) > 1
                      ? Style.spacing.hairline : 0
                    border.color: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : timelinePoint.overflowPoint
                      ? root.mixColor(root.contentForeground, Color.accent, 0.28)
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
                      color: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                        ? Style.selectedStateColor(root.contentForeground, Color.accent)
                        : root.contentForeground
                      opacity: timelinePoint.sourcePoint || timelinePoint.linkedHovered ? 1 : 0.82

                      Behavior on color { ColorAnimation { duration: 150 } }
                      Behavior on opacity { NumberAnimation { duration: 150 } }
                    }
                  }

                  Text {
                    textFormat: Text.PlainText
                    id: timelineLabel
                    visible: !(timelinePoint.sourcePoint && scrubValueBubble.visible)
                    width: parent.width
                    y: timelinePoint.upperLane
                      ? timelineView.railY - height - Style.space(14)
                      : timelineView.railY + Style.space(13)
                    horizontalAlignment: timelinePoint.overflowDirection === "previous"
                      ? Text.AlignLeft : timelinePoint.overflowDirection === "next"
                      ? Text.AlignRight : Text.AlignHCenter
                    elide: Text.ElideRight
                    text: (timelinePoint.overflowDirection === "previous" ? "← "
                      : timelinePoint.overflowDirection === "next" ? "→ " : "")
                      + String(timelinePoint.modelData.label || "").toUpperCase()
                    color: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                      ? Style.selectedStateColor(root.contentForeground, Color.accent)
                      : timelinePoint.overflowPoint
                      ? root.mixColor(Qt.darker(root.contentForeground, 1.45),
                        Color.accent, 0.28)
                      : Qt.darker(root.contentForeground, 1.45)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                    font.bold: timelinePoint.sourcePoint || timelinePoint.linkedHovered
                    font.letterSpacing: 0.8

                    Behavior on color { ColorAnimation { duration: 150 } }
                  }
                }
              }

              Rectangle {
                id: scrubPlayhead
                visible: root.scrubReady
                readonly property bool linkedHovered: timelineView.sourceMarkerHovered
                readonly property real connectorBottom:
                  timelineView.railY + Style.space(32)
                x: Math.round(timelineView.selectedX - width / 2)
                y: scrubValueBubble.visible
                  ? Style.space(34) : timelineView.railY - Style.space(5)
                width: Style.spacing.hairline
                height: Math.max(0, connectorBottom - y)
                color: Style.selectedStateColor(root.contentForeground, Color.accent)
                opacity: timelineView.interacting || linkedHovered ? 0.9 : 0.38

                Behavior on opacity {
                  NumberAnimation { duration: 160; easing.type: Easing.OutQuart }
                }

                Rectangle {
                  id: fixedPlayheadDot
                  anchors.horizontalCenter: parent.horizontalCenter
                  y: timelineView.railY - scrubPlayhead.y - height / 2
                  width: Style.space(9)
                  height: width
                  radius: width / 2
                  scale: scrubPlayhead.linkedHovered ? 1.45 : 1
                  color: Style.selectedStateColor(root.contentForeground, Color.accent)

                  Behavior on scale {
                    NumberAnimation { duration: 180; easing.type: Easing.OutQuart }
                  }
                }
              }

              Rectangle {
                id: scrubValueBubble
                visible: root.scrubReady
                  && timelineView.interacting
                x: Math.max(0, Math.min(timelineView.width - width,
                  timelineView.selectedX - width / 2))
                y: Style.space(17)
                width: Math.max(Style.space(70), scrubValueText.implicitWidth + Style.space(16))
                height: Style.space(24)
                radius: Style.cornerRadius
                color: Style.selectedFillFor(root.contentForeground, Color.accent)
                border.width: Style.spacing.hairline
                border.color: Style.selectedStateColor(root.contentForeground, Color.accent)

                Text {
                  textFormat: Text.PlainText
                  id: scrubValueText
                  anchors.centerIn: parent
                  text: TimeRail.selectionLabel(root.scrubPayload,
                    root.scrubSelectedFrame, timelineView.selectedUnavailable)
                  color: root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                  font.letterSpacing: 0.5
                }
              }

              MouseArea {
                id: railMouse
                z: 10
                x: timelineView.railInset
                y: timelineView.railY - Style.space(24)
                width: timelineView.railWidth
                height: Style.space(48)
                enabled: root.scrubReady && root.mode === "read"
                hoverEnabled: true
                activeFocusOnTab: true
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                Accessible.role: Accessible.Slider
                Accessible.name: "Compare times across the day"
                Accessible.description:
                  "Press Ctrl+T to focus, then use Left and Right. Press Enter to set the time, or drag or scroll the ruler."
                property real pressX: 0
                property int pressSlotIndex: -1
                property bool dragged: false
                property real wheelSlotRemainder: 0
                property bool wheelMovedSelection: false
                function previewAtDelta(delta) {
                  var nextSlotIndex = TimeRail.draggedSlotIndexAt(
                    delta, width, root.scrubPayload, pressSlotIndex)
                  dragged = nextSlotIndex !== pressSlotIndex
                  if (dragged || root.scrubPreviewActive)
                    root.applyScrubSlot(nextSlotIndex)
                  return nextSlotIndex
                }
                function scrubByWheel(event) {
                  var motion = TimeRail.wheelSlotMotion(
                    event.pixelDelta.x, event.pixelDelta.y,
                    event.angleDelta.x, event.angleDelta.y,
                    width, root.scrubPayload)
                  if (!isFinite(motion) || motion === 0) return false
                  wheelSlotRemainder += motion
                  scrubWheelCommitTimer.restart()
                  var wholeSlots = wheelSlotRemainder < 0
                    ? Math.ceil(wheelSlotRemainder)
                    : Math.floor(wheelSlotRemainder)
                  if (wholeSlots !== 0) {
                    wheelSlotRemainder -= wholeSlots
                    wheelMovedSelection = true
                    root.moveScrubSelection(wholeSlots)
                  }
                  return true
                }
                function handleWheel(event) {
                  if (!scrubByWheel(event)) {
                    event.accepted = false
                    return
                  }
                  forceActiveFocus(Qt.MouseFocusReason)
                  event.accepted = true
                }
                preventStealing: true
                onPositionChanged: function(mouse) {
                  if (!pressed) return
                  previewAtDelta(mouse.x - pressX)
                }
                onPressed: function(mouse) {
                  scrubWheelCommitTimer.stop()
                  wheelSlotRemainder = 0
                  wheelMovedSelection = false
                  pressX = mouse.x
                  pressSlotIndex = root.scrubSelectedSlotIndex >= 0
                    ? root.scrubSelectedSlotIndex
                    : root.nearestScrubSlot(root.clockForScrubSource())
                  dragged = false
                  forceActiveFocus(Qt.MouseFocusReason)
                }
                onReleased: function(mouse) {
                  previewAtDelta(mouse.x - pressX)
                  if (dragged)
                    root.lockScrubSelection()
                  else if (root.scrubPreviewActive)
                    root.cancelScrubPreview()
                  pressSlotIndex = -1
                  dragged = false
                }
                onCanceled: {
                  scrubWheelCommitTimer.stop()
                  wheelSlotRemainder = 0
                  wheelMovedSelection = false
                  pressSlotIndex = -1
                  dragged = false
                  root.cancelScrubPreview()
                }
                onActiveFocusChanged: {
                  if (!activeFocus && !pressed) {
                    scrubWheelCommitTimer.stop()
                    wheelSlotRemainder = 0
                    wheelMovedSelection = false
                    root.cancelScrubPreview()
                  }
                }
                Keys.onLeftPressed: function(event) {
                  root.moveScrubSelection(
                    event.modifiers & Qt.ShiftModifier ? -4 : -1)
                  event.accepted = true
                }
                Keys.onRightPressed: function(event) {
                  root.moveScrubSelection(
                    event.modifiers & Qt.ShiftModifier ? 4 : 1)
                  event.accepted = true
                }
                Keys.onReturnPressed: function(event) {
                  root.lockScrubSelection()
                  event.accepted = true
                }
                Keys.onEnterPressed: function(event) {
                  root.lockScrubSelection()
                  event.accepted = true
                }
                Keys.onPressed: function(event) {
                  if (event.key !== Qt.Key_Home) return
                  root.returnToLive()
                  event.accepted = true
                }
                Keys.onEscapePressed: function(event) {
                  scrubWheelCommitTimer.stop()
                  wheelSlotRemainder = 0
                  wheelMovedSelection = false
                  root.dismissTransientTime()
                  keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)
                  event.accepted = true
                }

                Timer {
                  id: scrubWheelCommitTimer
                  interval: 180
                  repeat: false
                  onTriggered: {
                    railMouse.wheelSlotRemainder = 0
                    if (railMouse.wheelMovedSelection)
                      root.lockScrubSelection()
                    railMouse.wheelMovedSelection = false
                  }
                }
              }

              WheelHandler {
                id: verticalScrubWheel
                // Wheel and two-finger gestures belong to the whole visual
                // timeline, including its labels, not only the thin drag band.
                parent: timelineView
                enabled: railMouse.enabled
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                orientation: Qt.Vertical
                onWheel: function(event) {
                  railMouse.handleWheel(event)
                }
              }

              WheelHandler {
                id: horizontalScrubWheel
                // WheelHandler defaults to vertical events, so a dedicated
                // horizontal handler is required for sideways trackpad motion.
                parent: timelineView
                enabled: railMouse.enabled
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                orientation: Qt.Horizontal
                onWheel: function(event) {
                  railMouse.handleWheel(event)
                }
              }
            }

            ListView {
              id: clockRows
              anchors.horizontalCenter: parent.horizontalCenter
              width: parent.width - Style.space(36)
              height: root.clockViewportHeight
              spacing: root.clockRowSpacing
              model: Math.ceil(root.clocks.length / root.clockColumnCount)
              clip: true
              boundsBehavior: Flickable.StopAtBounds
              interactive: contentHeight > height
              reuseItems: true
              cacheBuffer: root.clockRowHeight + root.clockRowSpacing

              QQC.ScrollBar.vertical: QQC.ScrollBar {
                policy: QQC.ScrollBar.AsNeeded
              }

              delegate: Row {
                  id: clockRow
                  required property int index
                  readonly property int startIndex: index * root.clockColumnCount
                  readonly property int itemCount:
                    Math.min(root.clockColumnCount, root.clocks.length - startIndex)
                  readonly property real cellSpacing:
                    Style.space(root.compactDensity ? 8 : 16)
                  readonly property real cellWidth:
                    (clockRows.width - cellSpacing * (root.clockColumnCount - 1))
                      / root.clockColumnCount
                  function cellAt(cellIndex) {
                    return clockCellRepeater.itemAt(cellIndex)
                  }
                  function resetRecycledState() {
                    for (var cellIndex = 0; cellIndex < itemCount; cellIndex++) {
                      var cell = cellAt(cellIndex)
                      if (cell) cell.resetRecycledState()
                    }
                  }
                  ListView.onPooled: clockRow.resetRecycledState()
                  ListView.onReused: clockRow.resetRecycledState()
                  anchors.left: parent.left
                  width: itemCount * cellWidth
                    + Math.max(0, itemCount - 1) * cellSpacing
                  height: root.clockRowHeight
                  spacing: cellSpacing

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
                      readonly property string hoverOwner: "card:" + String(clockIndex)
                      readonly property bool hasKeyboardCursor:
                        root.keyboardCursorActive
                          && root.keyboardClockIndex === clockIndex
                      readonly property bool linkedHovered:
                        root.timelineHoverMatches(clockData)
                      readonly property var localDaylight:
                        TimeRail.localDaylight(clockData, root.snapshot.reference_utc)
                      property bool labelEditing: false
                      function resetRecycledState() {
                        root.updateTimelineHover(hoverOwner, null, false)
                        if (cardTimeInput.activeFocus || cardLabelInput.activeFocus)
                          keyCatcher.forceActiveFocus(Qt.OtherFocusReason)
                        labelEditing = false
                        cardLabelInput.resetText()
                        cardTimeInput.text = String(clockData.time || "")
                      }
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
                        if (cardHoverHandler.hovered)
                          root.updateTimelineHover(hoverOwner, clockData, true)
                      }
                      Component.onDestruction:
                        root.updateTimelineHover(hoverOwner, null, false)
                      width: clockRow.cellWidth
                      height: clockRow.height
                      clip: true

                      HoverHandler {
                        id: cardHoverHandler
                        onHoveredChanged: root.updateTimelineHover(
                          clockCell.hoverOwner, clockCell.clockData, hovered)
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
                        anchors.leftMargin: Style.space(root.compactDensity ? 8 : 10)
                        anchors.rightMargin: Style.space(root.compactDensity ? 8 : 10)
                        anchors.topMargin: Style.space(root.compactDensity ? 5 : 7)
                        anchors.bottomMargin: Style.space(root.compactDensity ? 5 : 7)
                        spacing: Style.space(root.compactDensity ? 1 : 4)

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
                            font.pixelSize: root.compactDensity
                              ? Style.font.caption : Style.font.bodySmall
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
                                ? "Remove this time from the bar (P)"
                                : "Keep this time visible in the bar (P)"
                              fontSize: Style.font.caption
                              horizontalPadding: Style.space(5)
                              verticalPadding: Style.space(3)
                              onClicked: root.togglePin(clockCell.clockData)
                            }

                            PanelActionButton {
                              iconText: "󰆴"
                              enabled: root.canRemove && !actionProcess.running
                              tooltipText: root.canRemove
                                ? "Remove location (Delete)" : "Keep at least one timezone"
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
                          font.pixelSize: root.compactDensity
                            ? Style.font.display : Style.font.displayLarge
                          font.bold: true
                          selectByMouse: true
                          enabled: root.mode === "read" && root.snapshotLoaded
                          readOnly: convertProcess.running
                          onAccepted: root.convertFrom(
                            clockCell.clockData.timezone, text, conversionSource)
                          onTextEdited: root.timeInputEdited(conversionSource)
                          Keys.onEscapePressed: function(event) {
                            root.cancelTimeEditor(cardTimeInput,
                              clockCell.clockData.time, conversionSource)
                            event.accepted = true
                          }
                          onActiveFocusChanged: {
                            root.editorActive = activeFocus
                            root.timeEditorActive = activeFocus
                            if (activeFocus)
                              root.selectScrubSource(clockCell.clockData)
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
                            text: root.compactDensity
                              ? root.clockDayLabel(clockCell.clockData).toUpperCase()
                              : root.clockDayLabel(clockCell.clockData).toUpperCase()
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
                              color: root.weatherGlyphColor(clockCell.weatherData)
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

                      Item {
                        id: cardLocalDayRuler
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.leftMargin: Style.space(root.compactDensity ? 8 : 10)
                        anchors.rightMargin: Style.space(root.compactDensity ? 8 : 10)
                        height: Style.space(root.compactDensity ? 8 : 12)
                        opacity: root.localDayRulersVisible ? 1 : 0

                        Behavior on opacity {
                          NumberAnimation { duration: 150; easing.type: Easing.OutQuart }
                        }

                        Canvas {
                          id: localSolarGlow
                          anchors.fill: parent
                          readonly property var curveSegments:
                            clockCell.localDaylight.curve_segments
                          readonly property color glowColor: root.contentForeground
                          visible: curveSegments && curveSegments.length > 0
                          Accessible.ignored: true

                          function canvasColor(color, alpha) {
                            return "rgba(" + String(Math.round(color.r * 255)) + ","
                              + String(Math.round(color.g * 255)) + ","
                              + String(Math.round(color.b * 255)) + ","
                              + String(Math.max(0, Math.min(1, Number(alpha)))) + ")"
                          }

                          onCurveSegmentsChanged: requestPaint()
                          onGlowColorChanged: requestPaint()
                          onWidthChanged: requestPaint()
                          onHeightChanged: requestPaint()
                          Component.onCompleted: requestPaint()

                          onPaint: {
                            var context = getContext("2d")
                            context.clearRect(0, 0, width, height)
                            if (!visible) return
                            var baseline = Math.max(0,
                              height - Style.spacing.hairline / 2)
                            var amplitude = Math.max(0, height - Style.space(1))
                            context.save()
                            var glow = context.createLinearGradient(0, 0, 0, height)
                            glow.addColorStop(0, canvasColor(glowColor, 0.008))
                            glow.addColorStop(0.48, canvasColor(glowColor, 0.05))
                            glow.addColorStop(1, canvasColor(glowColor, 0.17))
                            context.fillStyle = glow
                            context.shadowColor = canvasColor(glowColor, 0.11)
                            context.shadowBlur = Style.space(3)
                            for (var segmentIndex = 0;
                                segmentIndex < curveSegments.length; segmentIndex++) {
                              var segment = curveSegments[segmentIndex]
                              var positions = segment && segment.positions
                              var heights = segment && segment.heights
                              if (!positions || !heights || positions.length < 2
                                  || positions.length !== heights.length) continue
                              var firstX = Math.max(0, Math.min(1,
                                Number(positions[0]))) * width
                              var lastX = firstX
                              context.beginPath()
                              context.moveTo(firstX, baseline)
                              for (var curveIndex = 0;
                                  curveIndex < positions.length; curveIndex++) {
                                var curveX = Math.max(0, Math.min(1,
                                  Number(positions[curveIndex]))) * width
                                var curveHeight = Math.max(0, Math.min(1,
                                  Number(heights[curveIndex])))
                                context.lineTo(curveX,
                                  baseline - curveHeight * amplitude)
                                lastX = curveX
                              }
                              context.lineTo(lastX, baseline)
                              context.closePath()
                              context.fill()
                            }
                            context.restore()
                          }
                        }

                        Rectangle {
                          id: localDayTrack
                          anchors.left: parent.left
                          anchors.right: parent.right
                          anchors.bottom: parent.bottom
                          height: Style.spacing.hairline
                          color: root.contentForeground
                          opacity: 0.08
                        }

                        Repeater {
                          id: localDaylightEdgeRepeater
                          model: clockCell.localDaylight.daylight_intervals || []

                          Rectangle {
                            required property var modelData
                            readonly property real startPosition: Math.max(0,
                              Math.min(1, Number(modelData.start)))
                            readonly property real endPosition: Math.max(0,
                              Math.min(1, Number(modelData.end)))
                            visible: endPosition > startPosition
                            x: Math.round(startPosition * parent.width)
                            anchors.bottom: parent.bottom
                            width: Math.max(0,
                              Math.round(endPosition * parent.width) - x)
                            height: Style.spacing.hairline
                            color: root.contentForeground
                            opacity: 0.14
                          }
                        }

                        Rectangle {
                          id: localDayMarker
                          readonly property real dayPosition:
                            TimeRail.localDayPosition(clockCell.clockData.local_minutes)
                          readonly property real sunlight: Math.max(0, Math.min(1,
                            Number(clockCell.localDaylight.marker_light)))
                          readonly property color nightTint: root.mixColor(
                            root.contentForeground,
                            Qt.rgba(0.43, 0.54, 0.76, 1), 0.48)
                          readonly property color nightColor: root.mixColor(
                            nightTint, clockSurface.color, 0.30)
                          readonly property color dayColor: root.mixColor(
                            root.contentForeground,
                            Qt.rgba(0.96, 0.72, 0.27, 1), 0.36)
                          readonly property real restingOpacity:
                            0.68 + sunlight * 0.26
                          x: Math.round(dayPosition * Math.max(0,
                            parent.width - width))
                          anchors.bottom: parent.bottom
                          width: Style.spacing.hairline
                          height: Math.round(parent.height * 0.5)
                          color: root.mixColor(nightColor, dayColor, sunlight)
                          opacity: clockCell.hasKeyboardCursor || clockCell.linkedHovered
                            ? 0.96 : restingOpacity

                          Behavior on color { ColorAnimation { duration: 150 } }
                          Behavior on opacity {
                            NumberAnimation { duration: 150; easing.type: Easing.OutQuart }
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
                enabled: root.searchVisible && !actionProcess.running
                Keys.priority: Keys.BeforeItem
                onTextChanged: root.searchTextChanged()
                onAccepted: root.acceptSearchResult()
                onActiveFocusChanged: root.editorActive = activeFocus
                Keys.onDownPressed: function(event) {
                  event.accepted = root.moveSearchResultSelection(1)
                }
                Keys.onUpPressed: function(event) {
                  event.accepted = root.moveSearchResultSelection(-1)
                }
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

            Globe {
              id: mapCanvas
              anchors.fill: parent
              clip: true
              interactive: !actionProcess.running
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

                  delegate: CursorSurface {
                    id: resultButton
                    required property int index
                    required property var modelData
                    width: searchResultList.width
                    height: Style.space(48)
                    enabled: !actionProcess.running
                    hasCursor: resultButton.index === root.searchResultIndex
                    foreground: root.contentForeground
                    accent: Color.accent

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

                    MouseArea {
                      anchors.fill: parent
                      hoverEnabled: true
                      enabled: !actionProcess.running
                      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onPositionChanged: {
                        if (resultButton.index !== root.searchResultIndex)
                          root.focusSearchResult(resultButton.index)
                      }
                      onClicked: {
                        root.focusSearchResult(resultButton.index)
                        root.selectMapLocation(resultButton.modelData)
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
                  readonly property var selection: modelData.selection || location
                  readonly property bool configured: modelData.configured === true
                  readonly property bool searchResult: modelData.searchResult === true
                  readonly property bool selectable: !configured
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
                          root.selectMapLocation(mapMarker.selection)
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
                          root.selectMapLocation(mapMarker.selection)
                      }
                    }
                  }
                }
              }

              Rectangle {
                id: mapSelectionPin
                readonly property var focusLocation:
                  root.mapFocusLocation(root.mapSelection)
                readonly property var projection: focusLocation
                  ? mapCanvas.project(focusLocation.latitude, focusLocation.longitude)
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
                readonly property var focusLocation:
                  root.mapFocusLocation(root.mapSelection)
                readonly property var projection: focusLocation
                  ? mapCanvas.project(focusLocation.latitude, focusLocation.longitude)
                  : ({ x: mapCanvas.width / 2, y: mapCanvas.height / 2, visible: false })
                readonly property real preferredX: root.mapSelectionCardOnRight
                  ? projection.x + pointGap : projection.x - width - pointGap
                visible: root.mode === "add" && root.mapSelection !== null
                  && !mapProcess.running
                onVisibleChanged: {
                  if (visible) root.completeMapSelectionActionFocus()
                }
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
                    active: true
                    bordered: true
                    focusable: true
                    foreground: root.contentForeground
                    fontFamily: root.contentFontFamily
                    fontSize: Style.font.title
                    horizontalPadding: Style.space(12)
                    verticalPadding: Style.space(8)
                    Accessible.name: "Add selected location"
                    Accessible.description: "Press Enter or Space to add"
                    onClicked: root.addMapSelection()
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
