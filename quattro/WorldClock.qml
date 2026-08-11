import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Native Quattro host for the world-clock panel. The panel stays loaded in
// omarchy-shell, so opening it is a local state change rather than a GTK
// process launch.
BarWidget {
  id: root
  moduleName: "io.github.olivoil.world-clock"

  property bool backendChecked: false
  property bool backendAvailable: false
  property string clockTooltip: "World Clock"
  property string pinnedTime: ""
  property string pinnedLabel: ""

  readonly property string backendCommand: {
    var configured = String(setting("command", "omarchy-world-clock")).trim()
    return configured === "" ? "omarchy-world-clock" : configured
  }
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  readonly property bool popoutSwitchClosing: panelLoader.item
    ? panelLoader.item.popoutSwitchClosing === true : false
  readonly property Item activeButton: pinnedTime !== "" && !vertical
    ? pinnedButton : iconButton
  // Match the clock widget's painted-content convention. The generic bar
  // fallback only marks 55% of a slot, which is correct for one glyph but
  // looked truncated once the pinned time widened this widget.
  readonly property real openPanelIndicatorWidth: pinnedTime !== "" && !vertical
    ? pinnedContent.implicitWidth
    : Math.max(Style.space(10), Math.round(activeButton.implicitWidth * 0.55))
  readonly property string unavailableTooltip:
    "World Clock unavailable\nInstall omarchy-world-clock-bin"

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
    if (!moduleProcess.running) moduleProcess.running = true
    if (panelLoader.item && panelLoader.item.refresh) panelLoader.item.refresh()
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

  function applyModulePayload(raw) {
    try {
      var payload = JSON.parse(String(raw || ""))
      backendChecked = true
      backendAvailable = true
      clockTooltip = String(payload.tooltip || "World Clock")
      pinnedTime = String(payload.pinned_time || "")
      pinnedLabel = String(payload.pinned_label || "")
      injectPanel()
    } catch (error) {
      backendChecked = true
      backendAvailable = false
      pinnedTime = ""
      pinnedLabel = ""
    }
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
      else {
        root.backendChecked = true
        root.backendAvailable = false
      }
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
    visible: root.pinnedTime === "" || root.vertical
    text: "\uf0ac"
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
    visible: root.pinnedTime !== "" && !root.vertical
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
          text: "\uf0ac"
          fontFamily: pinnedButton.fontFamily
          fontSize: Style.bar.iconFont
          color: pinnedButton.foreground
        }
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        text: root.pinnedTime
        color: pinnedButton.foreground
        font.family: pinnedButton.fontFamily
        font.pixelSize: Style.font.body
        renderType: Text.NativeRendering
      }
    }
  }
}
