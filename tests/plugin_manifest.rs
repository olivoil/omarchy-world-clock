use serde_json::Value;
use std::fs;
use std::path::Path;

#[test]
fn quattro_manifest_declares_a_loadable_world_clock_widget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join("manifest.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read plugin manifest"))
            .expect("parse plugin manifest");

    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["id"], "io.github.olivoil.world-clock");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["license"], "MIT");
    assert_eq!(manifest["kinds"], serde_json::json!(["bar-widget"]));

    let entry_point = manifest
        .pointer("/entryPoints/barWidget")
        .and_then(Value::as_str)
        .expect("bar widget entry point");
    assert!(!entry_point.starts_with('/'));
    assert!(!entry_point.contains(".."));

    let qml_path = root.join(entry_point);
    assert!(qml_path.is_file(), "missing {}", qml_path.display());
    let qml = fs::read_to_string(&qml_path).expect("read QML entry point");
    assert!(qml.contains("moduleName: \"io.github.olivoil.world-clock\""));
    assert!(qml.contains("source: Qt.resolvedUrl(\"Panel.qml\")"));
    assert!(qml.contains("slotSize: Style.bar.statusSlot"));
    assert!(qml.contains("useActiveColor: false"));
    assert!(qml.contains("openPanelIndicatorWidth"));
    assert!(qml.contains("property bool moduleRefreshPending: false"));
    assert!(qml.contains("function flushModuleRefresh()"));
    assert!(qml.contains("Qt.callLater(root.flushModuleRefresh)"));
    assert!(!qml.contains("command: [root.backendCommand, \"open\"]"));

    let panel_path = qml_path.parent().unwrap().join("Panel.qml");
    assert!(panel_path.is_file(), "missing {}", panel_path.display());
    let panel = fs::read_to_string(panel_path).expect("read native panel");
    assert!(panel.contains("KeyboardPanel"));
    assert!(panel.contains("owner: root.barIdentity"));
    assert!(panel.contains("onTabRequested"));
    assert!(panel.contains("root.switchPanel(direction)"));
    assert!(panel.contains("readonly property int maxClocks: 9"));
    assert!(panel.contains("source: Qt.resolvedUrl(\"../assets/world-map.png\")"));
    assert!(panel.contains("backendCommand, \"locate\""));
    assert!(panel.contains("id: searchResultOverlay"));
    assert!(panel.contains(
        "height: Math.min(root.searchResults.length * Style.space(48), mapCanvas.height)"
    ));
    assert!(panel.contains("id: searchResultList"));
    assert!(panel.contains("name === \"pin\" || name === \"remove\""));
    assert!(panel.contains("result.label !== null && result.label !== undefined"));
    assert!(panel.contains("id: timelineTickRepeater"));
    assert!(panel.contains("function focusSummaryEditor()"));
    assert!(panel.contains("property bool snapshotLoaded: false"));
    assert!(panel.contains("property bool summaryFocusPending: false"));
    assert!(panel.contains("if (!snapshotLoaded)"));
    assert!(panel.contains("if (summaryFocusPending) Qt.callLater(root.focusSummaryEditor)"));
    assert!(panel.contains("if (clocks.length === 0 && mode !== \"add\" && !summaryFocusPending)"));
    assert!(panel.contains("function focusAddField()"));
    assert!(panel.contains("addField.forceActiveFocus(Qt.ShortcutFocusReason)"));
    assert!(panel.contains("if (mode === \"add\")"));
    assert!(panel.contains("if (root.mode === \"read\") root.focusSummaryEditor()"));
    assert!(panel.contains("Qt.callLater(function()"));
    assert!(panel.contains("property bool editorRefreshPending: false"));
    assert!(panel.contains("property bool snapshotRequestPending: false"));
    assert!(panel.contains("property int snapshotStateGeneration: 0"));
    assert!(panel.contains("property int convertActiveGeneration: -1"));
    assert!(panel.contains("property string invalidConversionSource: \"\""));
    assert!(panel.contains("if (!snapshotLoaded || !timezoneName || !reference"));
    assert!(panel.contains("if (timeEditorActive)"));
    assert!(panel.contains("snapshotActiveGeneration === snapshotStateGeneration"));
    assert!(panel.contains("var manual = root.snapshotActiveReference !== \"\""));
    assert!(panel
        .contains("var current = root.convertActiveGeneration === root.snapshotStateGeneration"));
    assert!(panel.contains("function conversionSource(clock)"));
    assert!(panel.contains("function timeInputEdited(source)"));
    assert!(panel.contains("if (convertProcess.running) convertActiveGeneration = -1"));
    assert!(panel.contains("readOnly: convertProcess.running"));
    assert!(panel.contains("onTextEdited: root.timeInputEdited(conversionSource)"));
    assert!(panel.contains("conversionInvalid ? Color.urgent : root.contentForeground"));
    assert!(panel.contains("Qt.callLater(root.flushSnapshotRequest)"));
    assert!(panel.contains("Qt.callLater(root.flushEditorRefresh)"));
    assert!(panel.contains("property string searchResultsQuery: \"\""));
    assert!(panel.contains("property string searchSubmitQuery: \"\""));
    assert!(panel.contains("searchResultsQuery === query"));
    assert!(panel.contains("searchSubmitQuery = query"));
    assert!(panel.contains("Qt.callLater(root.startSearch)"));
    assert!(panel.contains("onTextChanged: root.searchTextChanged()"));
    assert!(panel.contains("visible: root.summary.pinned === true"));
    assert!(panel.contains("onClicked: root.togglePin(root.summary)"));
    assert!(panel.contains("https://open-meteo.com/"));
    assert!(panel.contains("onLinkActivated: function(link) { Qt.openUrlExternally(link) }"));
    assert!(panel.contains("readonly property string currentLocationTitle"));
    assert!(panel.contains("readonly property string currentTimezoneMetadata"));
    assert!(
        panel.contains("root.mode === \"add\" ? \"Add a Location\" : root.currentLocationTitle")
    );
    assert!(panel.contains("text: root.currentTimezoneMetadata"));
    assert!(panel.contains("id: clockRows"));
    assert!(panel.contains("anchors.horizontalCenter: parent.horizontalCenter"));
    assert!(panel.contains("anchors.leftMargin: Style.space(10)"));
    assert!(panel.contains("id: clockSurface"));
    assert!(panel.contains("color: Style.normalFillFor(root.contentForeground, Color.accent)"));
    assert!(panel.contains("radius: Style.cornerRadius"));

    assert!(qml.contains("function editCurrentTime()"));
    assert!(qml.contains("function edit(): void { root.editCurrentTime() }"));
    assert!(qml.contains("panelLoader.item.opened && panelLoader.item.live"));
    assert!(qml.contains("onDateChanged: root.refresh()"));
    assert!(!qml.contains("root.pinnedLabel + \" · \""));

    let map_path = root.join("assets/world-map.png");
    assert!(map_path.is_file(), "missing {}", map_path.display());
}
