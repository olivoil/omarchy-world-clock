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
  property string backendCommand: "omarchy-world-clock"
  property var snapshot: ({
    schema_version: 1,
    reference_utc: "",
    local_timezone: "",
    time_format: "24h",
    configured_count: 0,
    pinned_timezone: null,
    summary: ({ timezone: "", label: "", title: "", time: "--:--", day: "", notation: "", relative_minutes: 0, relative_label: "Same time" }),
    clocks: [],
    timeline: []
  })
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
  property string statusText: ""
  property bool statusError: false
  property string actionName: ""
  property string searchQueryInFlight: ""
  property var searchResults: []
  property var mapSelection: null
  property real mapRequestedLatitude: 0
  property real mapRequestedLongitude: 0
  property real mapLookupLatitude: 0
  property real mapLookupLongitude: 0
  property real mapCursorX: 0
  property real mapCursorY: 0
  property bool mapClickPending: false

  readonly property var barIdentity: hostWidget || root
  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property var clocks: snapshot && Array.isArray(snapshot.clocks) ? snapshot.clocks : []
  readonly property var timeline: snapshot && Array.isArray(snapshot.timeline) ? snapshot.timeline : []
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
  readonly property bool canAdd: clocks.length < maxClocks
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
    if (mode !== "read") return
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
    searchResults = []
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
      summaryInput.text = String(payload.summary.time || "--:--")
      if (manual !== true && live) live = true
      if (clocks.length === 0 && mode !== "add") mode = "add"
      clearStatus()
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
    live = true
    requestLiveSnapshot()
  }

  function convertFrom(timezone, value) {
    var text = String(value || "").trim()
    if (!text || convertProcess.running) return
    invalidateSnapshotRequests()
    convertProcess.command = [
      backendCommand,
      "convert",
      "--timezone", String(timezone),
      "--value", text,
      "--base", String(snapshot.reference_utc || "")
    ]
    convertProcess.running = true
  }

  function runAction(name, timezone, result) {
    if (actionProcess.running) return
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

  function scheduleSearch() {
    searchDebounce.restart()
  }

  function startSearch() {
    var query = String(addField.text || "").trim()
    if (!query) {
      searchResults = []
      return
    }
    if (searchProcess.running) return
    searchQueryInFlight = query
    searchProcess.command = [backendCommand, "search", query]
    searchProcess.running = true
  }

  function addFirstResult() {
    if (searchResults.length > 0 && canAdd)
      runAction("add", searchResults[0].timezone, searchResults[0])
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

  onOpenedChanged: if (opened) refresh()
  onTimeEditorActiveChanged: {
    if (!timeEditorActive && editorRefreshPending)
      Qt.callLater(root.flushEditorRefresh)
  }
  onModeChanged: {
    if (mode === "add") {
      focusAddField()
    } else {
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
      if (exitCode === 0 && current && root.timeEditorActive
          && !root.editorRefreshPending) {
        root.editorRefreshPending = true
        root.editorRefreshReference = root.snapshotActiveReference
      } else if (exitCode === 0 && current) {
        root.applySnapshot(snapshotOutput.text, manual)
      } else if (exitCode !== 0 && current)
        root.setStatus("World Clock backend needs an update. Install omarchy-world-clock-bin.", true)
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
      if (exitCode !== 0) {
        root.setStatus(String(convertError.text || "Use HH:MM, 830, 8.5, 3pm, or YYYY-MM-DD HH:MM.").trim(), true)
        return
      }
      try {
        var payload = JSON.parse(String(convertOutput.text || ""))
        root.invalidateSnapshotRequests()
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
      var currentQuery = String(addField.text || "").trim()
      if (root.searchQueryInFlight !== currentQuery) {
        root.scheduleSearch()
        return
      }
      if (exitCode !== 0) {
        root.searchResults = []
        root.setStatus("Search is unavailable; exact timezone names still work.", true)
        return
      }
      try {
        var payload = JSON.parse(String(searchOutput.text || "[]"))
        root.searchResults = Array.isArray(payload) ? payload : []
        if (root.searchResults.length === 0) root.setStatus("No matching location.", false)
        else root.clearStatus()
      } catch (error) {
        root.searchResults = []
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
              width: parent.width
              height: Style.space(92)

              TextInput {
                id: summaryInput
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.max(implicitWidth, Style.space(230))
                horizontalAlignment: Text.AlignHCenter
                text: root.summary.time || "--:--"
                color: root.contentForeground
                selectionColor: Style.selectionFill
                selectedTextColor: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.space(52)
                font.bold: true
                selectByMouse: true
                enabled: root.mode === "read"
                onAccepted: root.convertFrom(root.summary.timezone, text)
                onActiveFocusChanged: {
                  root.editorActive = activeFocus
                  root.timeEditorActive = activeFocus
                  if (!activeFocus && text !== root.summary.time) text = root.summary.time
                }
              }

              Rectangle {
                visible: summaryInput.activeFocus
                anchors.left: summaryInput.left
                anchors.right: summaryInput.right
                anchors.top: summaryInput.bottom
                height: Style.spacing.hairline
                color: Style.focusStateColor(root.contentForeground, Color.accent)
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
                          width: parent.width
                          text: clockCell.clockData.time
                          color: root.contentForeground
                          selectionColor: Style.selectionFill
                          selectedTextColor: root.contentForeground
                          font.family: root.contentFontFamily
                          font.pixelSize: Style.font.displayLarge
                          font.bold: true
                          selectByMouse: true
                          enabled: root.mode === "read"
                          onAccepted: root.convertFrom(clockCell.clockData.timezone, text)
                          onActiveFocusChanged: {
                            root.editorActive = activeFocus
                            root.timeEditorActive = activeFocus
                            if (!activeFocus && text !== clockCell.clockData.time)
                              text = clockCell.clockData.time
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
              onTextChanged: root.scheduleSearch()
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
                    enabled: root.canAdd && !actionProcess.running
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
