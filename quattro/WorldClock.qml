pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Native Quattro host for the world-clock panel. The panel stays loaded in
// omarchy-shell, so opening it is a local state change rather than launching
// an external UI process.
BarWidget {
  id: root
  moduleName: "io.github.olivoil.world-clock"

  property bool backendChecked: false
  property bool backendAvailable: false
  property string clockTooltip: "World Clock"
  property var pinnedClocks: []
  property bool moduleRefreshPending: false
  property string backendFailureDetail: "The bundled backend could not be started"

  readonly property int supportedBackendProtocol: 4
  readonly property string backendCommand:
    String(Qt.resolvedUrl("../bin/omarchy-world-clock-backend")).replace(/^file:\/\//, "")
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  readonly property bool popoutSwitchClosing: panelLoader.item
    ? panelLoader.item.popoutSwitchClosing === true : false
  readonly property bool hasPinnedClocks: pinnedClocks.length > 0
  readonly property Item activeButton: hasPinnedClocks && !vertical
    ? pinnedButton : iconButton
  // Match the clock widget's painted-content convention. The generic bar
  // fallback only marks 55% of a slot, which is correct for one glyph but
  // looks truncated once pinned clocks widen this widget.
  readonly property real openPanelIndicatorWidth: hasPinnedClocks && !vertical
    ? pinnedContent.implicitWidth
    : Math.max(Style.space(10), Math.round(activeButton.implicitWidth * 0.55))
  readonly property string unavailableTooltip:
    "World Clock unavailable\n" + backendFailureDetail

  implicitWidth: activeButton.implicitWidth
  implicitHeight: activeButton.implicitHeight

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = root.activeButton
    if ("hostWidget" in target) target.hostWidget = root
    if ("backendCommand" in target) target.backendCommand = root.backendCommand
  }

  function refresh() {
    requestModuleRefresh()
    if (panelLoader.item && panelLoader.item.opened && panelLoader.item.live
        && panelLoader.item.refresh)
      panelLoader.item.refresh()
  }

  function requestModuleRefresh() {
    if (moduleProcess.running) {
      moduleRefreshPending = true
      return
    }
    moduleRefreshPending = false
    moduleProcess.running = true
  }

  function flushModuleRefresh() {
    if (!moduleRefreshPending || moduleProcess.running) return
    moduleRefreshPending = false
    moduleProcess.running = true
  }

  function open() {
    if (panelLoader.item && panelLoader.item.open) panelLoader.item.open()
  }

  function openAdd() {
    if (panelLoader.item && panelLoader.item.openAdd) panelLoader.item.openAdd()
  }

  function editCurrentTime() {
    if (panelLoader.item && panelLoader.item.openEditor)
      panelLoader.item.openEditor()
  }

  function close() {
    if (panelLoader.item && panelLoader.item.close) panelLoader.item.close()
  }

  function closeForPopoutSwitch() {
    if (panelLoader.item && panelLoader.item.closeForPopoutSwitch)
      panelLoader.item.closeForPopoutSwitch()
  }

  function togglePanel() {
    if (panelLoader.item && panelLoader.item.toggle) panelLoader.item.toggle()
  }

  function openTimezoneSelector() {
    Quickshell.execDetached(["omarchy-menu-timezone"])
  }

  function handlePress(buttonCode) {
    if (buttonCode === Qt.RightButton) root.openTimezoneSelector()
    else if (buttonCode === Qt.LeftButton) root.togglePanel()
  }

  function markBackendUnavailable(detail) {
    backendChecked = true
    backendAvailable = false
    backendFailureDetail = String(detail || "The bundled backend could not be started")
    pinnedClocks = []
  }

  function validPinnedClocks(value) {
    if (!Array.isArray(value)) return false
    for (var index = 0; index < value.length; index++) {
      var clock = value[index]
      if (!clock || typeof clock !== "object" || Array.isArray(clock)
          || typeof clock.code !== "string"
          || typeof clock.label !== "string"
          || typeof clock.time !== "string") return false
    }
    return true
  }

  function applyModulePayload(raw) {
    var payload
    var protocol
    var tooltip
    var nextPinnedClocks
    try {
      payload = JSON.parse(String(raw || ""))
      if (!payload || typeof payload !== "object" || Array.isArray(payload)
          || typeof payload.protocol_version !== "number")
        throw new Error("Invalid World Clock backend response")
      protocol = Number(payload.protocol_version)
    } catch (error) {
      markBackendUnavailable("The bundled backend returned an invalid response")
      return
    }
    if (protocol !== supportedBackendProtocol) {
      // Omarchy releases without revisioned plugin URLs can keep the previous
      // QML component cached after `omarchy plugin update`, while replacing
      // this binary in place. A shell restart loads both halves of one release.
      markBackendUnavailable("Run omarchy restart shell to finish updating")
      return
    }
    try {
      if (typeof payload.tooltip !== "string"
          || !root.validPinnedClocks(payload.pinned_clocks))
        throw new Error("Invalid World Clock backend response")
      tooltip = payload.tooltip
      nextPinnedClocks = payload.pinned_clocks
    } catch (error) {
      markBackendUnavailable("The bundled backend returned an invalid response")
      return
    }
    backendChecked = true
    backendAvailable = true
    backendFailureDetail = ""
    clockTooltip = tooltip || "World Clock"
    pinnedClocks = nextPinnedClocks
    injectPanel()
  }

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()
  onBackendCommandChanged: injectPanel()
  onActiveButtonChanged: injectPanel()

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  IpcHandler {
    target: root.moduleName

    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.togglePanel() }
    function add(): void { root.openAdd() }
    function edit(): void { root.editCurrentTime() }
    function refresh(): void { root.refresh() }
    function status(): string { return root.opened ? "open" : "closed" }
  }

  Process {
    id: moduleProcess
    command: [root.backendCommand, "module"]
    stdout: StdioCollector {
      id: moduleOutput
      waitForEnd: true
    }
    onExited: function(exitCode) {
      if (exitCode === 0) root.applyModulePayload(moduleOutput.text)
      else root.markBackendUnavailable("The bundled backend could not be started")
      Qt.callLater(root.flushModuleRefresh)
    }
  }

  SystemClock {
    id: minuteClock
    precision: SystemClock.Minutes
    onDateChanged: root.refresh()
  }

  Timer {
    interval: 1
    running: true
    onTriggered: root.refresh()
  }

  BarIconButton {
    id: iconButton
    anchors.fill: parent
    bar: root.bar
    visible: !root.hasPinnedClocks || root.vertical
    text: "\uf017"
    slotSize: Style.bar.statusSlot
    useActiveColor: false
    dimmed: root.backendChecked && !root.backendAvailable
    tooltipText: root.backendChecked && !root.backendAvailable
      ? root.unavailableTooltip : root.clockTooltip
    onPressed: function(buttonCode) { root.handlePress(buttonCode) }
  }

  WidgetButton {
    id: pinnedButton
    anchors.fill: parent
    bar: root.bar
    visible: root.hasPinnedClocks && !root.vertical
    labelVisible: false
    hasVisualContent: visible
    useActiveColor: false
    fixedWidth: pinnedContent.implicitWidth + Style.space(11)
    dimmed: root.backendChecked && !root.backendAvailable
    tooltipText: root.backendChecked && !root.backendAvailable
      ? root.unavailableTooltip
      : root.clockTooltip
    onPressed: function(buttonCode) { root.handlePress(buttonCode) }

    Row {
      id: pinnedContent
      anchors.centerIn: parent
      spacing: Style.space(5)

      Item {
        width: Style.bar.iconCanvas
        height: Style.bar.iconCanvas
        anchors.verticalCenter: parent.verticalCenter

        OpticalGlyph {
          anchors.fill: parent
          text: "\uf017"
          fontFamily: pinnedButton.fontFamily
          fontSize: Style.bar.iconFont
          color: pinnedButton.foreground
        }
      }

      Repeater {
        model: root.pinnedClocks

        Row {
          id: pinnedClock
          required property int index
          required property var modelData
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(3)

          Text {
            textFormat: Text.PlainText
            visible: pinnedClock.index > 0
            anchors.verticalCenter: parent.verticalCenter
            text: "·"
            color: pinnedButton.foreground
            opacity: 0.58
            font.family: pinnedButton.fontFamily
            font.pixelSize: Style.font.caption
            renderType: Text.NativeRendering
          }

          Text {
            textFormat: Text.PlainText
            anchors.verticalCenter: parent.verticalCenter
            text: pinnedClock.modelData.code
            color: pinnedButton.foreground
            opacity: 0.76
            font.family: pinnedButton.fontFamily
            font.pixelSize: Style.font.caption
            font.bold: true
            font.letterSpacing: 0.7
            renderType: Text.NativeRendering
          }

          Text {
            textFormat: Text.PlainText
            anchors.verticalCenter: parent.verticalCenter
            text: pinnedClock.modelData.time
            color: pinnedButton.foreground
            font.family: pinnedButton.fontFamily
            font.pixelSize: Style.font.body
            renderType: Text.NativeRendering
          }
        }
      }
    }
  }
}
