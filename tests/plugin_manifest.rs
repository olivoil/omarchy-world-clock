use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn assert_text_items_are_plain_text(source: &str, filename: &str) {
    let text_items = source.matches("Text {").count();
    let plain_text_items = source.matches("textFormat: Text.PlainText").count();
    assert!(text_items > 0, "{filename} should contain Text items");
    assert_eq!(
        plain_text_items, text_items,
        "every Text item in {filename} must explicitly use Text.PlainText"
    );
}

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
    assert_eq!(manifest["license"], "MIT AND ODbL-1.0");
    assert_eq!(manifest["kinds"], serde_json::json!(["bar-widget"]));
    assert_eq!(manifest["barWidget"]["defaults"]["showWeather"], true);
    assert_eq!(manifest["barWidget"]["schema"][0]["key"], "showWeather");
    assert_eq!(manifest["barWidget"]["schema"][0]["type"], "boolean");
    assert_eq!(
        manifest["barWidget"]["schema"][0]["label"],
        "Show current weather"
    );
    assert_eq!(manifest["barWidget"]["schema"][0]["defaultValue"], true);

    let entry_point = manifest
        .pointer("/entryPoints/barWidget")
        .and_then(Value::as_str)
        .expect("bar widget entry point");
    assert!(!entry_point.starts_with('/'));
    assert!(!entry_point.contains(".."));

    let qml_path = root.join(entry_point);
    assert!(qml_path.is_file(), "missing {}", qml_path.display());
    let qml = fs::read_to_string(&qml_path).expect("read QML entry point");
    assert_text_items_are_plain_text(&qml, "quattro/WorldClock.qml");
    assert!(qml.contains("moduleName: \"io.github.olivoil.world-clock\""));
    assert!(qml.contains("Qt.resolvedUrl(\"../bin/omarchy-world-clock-backend\")"));
    assert!(!qml.contains("setting(\"command\""));
    assert!(!qml.contains("Install omarchy-world-clock-bin"));
    let backend_path = root.join("bin/omarchy-world-clock-backend");
    assert!(backend_path.is_file(), "missing {}", backend_path.display());
    assert_ne!(
        fs::metadata(&backend_path)
            .expect("read bundled backend metadata")
            .permissions()
            .mode()
            & 0o111,
        0,
        "bundled backend must be executable"
    );
    assert!(qml.contains("source: Qt.resolvedUrl(\"Panel.qml\")"));
    assert!(qml.contains("slotSize: Style.bar.statusSlot"));
    assert!(qml.contains("useActiveColor: false"));
    assert_eq!(
        qml.matches("text: \"\\uf017\"").count(),
        2,
        "the normal and pinned widget states should use the clock glyph"
    );
    assert!(!qml.contains("text: \"\\uf0ac\""));
    assert!(qml.contains("openPanelIndicatorWidth"));
    assert!(qml.contains("property var pinnedClocks: []"));
    assert!(qml.contains("readonly property bool hasPinnedClocks:"));
    assert!(qml.contains("property bool moduleRefreshPending: false"));
    assert!(qml.contains("readonly property int supportedBackendProtocol: 5"));
    assert!(qml.contains("function markBackendUnavailable(detail)"));
    assert!(qml.contains("function validPinnedClocks(value)"));
    assert!(qml.contains("!payload || typeof payload !== \"object\" || Array.isArray(payload)"));
    assert!(qml.contains("typeof payload.protocol_version !== \"number\""));
    assert!(qml.contains("typeof payload.tooltip !== \"string\""));
    assert!(qml.contains("!root.validPinnedClocks(payload.pinned_clocks)"));
    assert!(qml.contains("typeof clock.code !== \"string\""));
    assert!(qml.contains("typeof clock.label !== \"string\""));
    assert!(qml.contains("typeof clock.time !== \"string\""));
    assert!(!qml.contains("String(payload.tooltip"));
    assert!(!qml.contains("String(payload.pinned_clocks"));
    assert!(qml.contains("Run omarchy restart shell to finish updating"));
    assert!(
        qml.find("if (protocol !== supportedBackendProtocol)")
            .expect("protocol mismatch branch")
            < qml
                .find("typeof payload.tooltip !== \"string\"")
                .expect("supported-protocol display field validation"),
        "protocol mismatches must show restart guidance before version-specific fields are validated"
    );
    assert!(qml.contains("The bundled backend returned an invalid response"));
    assert!(qml.contains("function flushModuleRefresh()"));
    assert!(qml.contains("Qt.callLater(root.flushModuleRefresh)"));
    assert!(!qml.contains("command: [root.backendCommand, \"open\"]"));

    let panel_path = qml_path.parent().unwrap().join("Panel.qml");
    assert!(panel_path.is_file(), "missing {}", panel_path.display());
    let panel = fs::read_to_string(panel_path).expect("read native panel");
    assert_text_items_are_plain_text(&panel, "quattro/Panel.qml");
    assert!(!panel.contains("omarchy-world-clock-bin"));
    assert!(panel.contains("KeyboardPanel"));
    assert!(panel.contains("owner: root.barIdentity"));
    assert!(panel.contains("onTabRequested"));
    assert!(panel.contains("root.switchPanel(direction)"));
    assert!(!panel.contains("maxClocks"));
    assert!(!panel.contains("canAddLocation"));
    assert!(panel.contains("readonly property bool autoCompactDensity"));
    assert!(panel.contains("readonly property bool compactDensity"));
    let compact_density = panel
        .split("readonly property bool compactDensity:")
        .nth(1)
        .and_then(|source| {
            source
                .split("readonly property int clockColumnCount:")
                .next()
        })
        .expect("compact density binding");
    assert!(
        compact_density.contains("(mode === \"read\" || mode === \"edit\") && autoCompactDensity")
    );
    assert!(!panel.contains("densityPreference"));
    assert!(panel.contains("readonly property int clockColumnCount"));
    assert!(panel
        .contains("readonly property real clockRowHeight: Style.space(compactDensity ? 92 : 104)"));
    assert!(panel.contains("rows * Style.space(104)"));
    assert!(panel
        .contains("readonly property real clockRowSpacing: Style.space(compactDensity ? 8 : 14)"));
    assert!(!panel.contains("root.mode === \"edit\" ? 110 : 100"));
    assert!(!panel.contains("locationView"));
    assert!(!panel.contains("id: densityButton"));
    assert!(!panel.contains("id: scheduleViewButton"));
    assert!(!panel.contains("id: scheduleRows"));
    assert!(!panel.contains("availabilitySlotIndexAt"));
    assert!(panel.contains("readonly property bool localTimezoneConfigured"));
    assert!(panel.contains("Globe {"));
    assert!(panel.contains("snapshot.featured_cities"));
    assert!(panel.contains("function globeLabelLayouts(width, height)"));
    assert!(panel.contains("property var markerLayouts: root.globeLabelLayouts(width, height)"));
    assert!(panel.contains("function globeLocationReveal(wrapper)"));
    assert!(panel.contains("(mapCanvas.zoom - minimumZoom) / 0.24"));
    assert!(panel.contains("layout.reveal * (searchResult || configured"));
    assert!(panel.contains("function mapLocationKey(location)"));
    assert!(panel.contains("function mapLocationSelected(location)"));
    assert!(panel.contains("readonly property int maximumSavedGlobeMarkers: 48"));
    assert!(panel.contains("var preferredSavedMarkers = []"));
    assert!(panel.contains("saved.pinned === true"));
    assert!(panel.contains(".slice(0, maximumSavedGlobeMarkers)"));
    assert!(panel.contains("if (savedKey) seenPlaces[savedKey] = true"));
    assert!(panel.contains("var mayPlaceLabel = !mapLocationSelected(location)"));
    assert!(panel.contains("visible: mapMarker.layout.labelVisible && !mapMarker.selected"));
    assert!(!panel.contains("featuredPlaced < 7"));
    assert!(panel.contains("readonly property var mapLocations: searchHasQuery"));
    assert!(panel.contains("model: root.mapLocations"));
    assert!(panel.contains("modelData.searchResult === true"));
    assert!(panel.contains("opacity: mapCanvas.openingFlightRunning ? 0 : 1"));
    let city_mouse = panel
        .split("id: cityMouse")
        .nth(1)
        .and_then(|source| source.split("onClicked:").next())
        .expect("city label mouse area");
    assert!(city_mouse.contains("enabled: !mapCanvas.openingFlightRunning"));
    assert!(panel.contains("readonly property bool selectable: !configured"));
    assert_eq!(panel.matches("if (mapMarker.selectable)").count(), 2);
    assert!(panel.contains("hoverEnabled: mapMarker.selectable"));
    assert!(panel.contains("property bool globeDetailRequested: false"));
    assert!(panel.contains("function globeOpeningTarget()"));
    assert!(panel.contains("root.conversionSource(snapshot.pinned_location)"));
    assert!(panel.contains("pinned.pinned === true"));
    assert!(panel.contains("root.conversionSource(pinned) === pinnedIdentity"));
    assert!(!panel.contains("String(snapshot.pinned_timezone || \"\")"));
    assert!(panel.contains("|| !mapCanvas.previewReady) return"));
    assert!(panel.contains("mapCanvas.settleOn(target.latitude, target.longitude)"));
    assert!(panel.contains("mapCanvas.stopMotion()"));
    assert!(panel.contains("function requestGlobeDetailWhenReady()"));
    assert!(panel.contains("!mapCanvas.previewReady"));
    assert!(panel.contains("onPreviewReadyChanged:"));
    assert!(panel.contains("if (previewReady) {"));
    assert!(panel
        .contains("root.initializeGlobe()\n                  root.requestGlobeDetailWhenReady()"));
    assert!(panel.contains("if (!opened) {\n      globeDetailRequested = false"));
    assert!(panel.contains("highResolutionEnabled: root.opened && root.globeDetailRequested"));
    assert!(panel.contains("if (!root.mapClickPending) return\n      if (exitCode !== 0)"));
    assert!(panel.contains("backendCommand, \"locate\""));
    assert!(panel.contains("function selectMapLocation(location)"));
    assert!(panel.contains("function dismissMapSelection()"));
    assert!(panel.contains("if (searchVisible) {\n      closeSearch()\n      return\n    }"));
    assert!(panel.contains(
        "keyCatcher.forceActiveFocus(Qt.ShortcutFocusReason)\n    Qt.callLater(function()"
    ));
    assert!(panel.contains("id: mapSelectionCard"));
    assert!(panel.contains("id: mapSelectionAddButton"));
    assert!(panel.contains("root.selectMapLocation(mapMarker.selection)"));
    assert!(panel.contains("property int searchResultIndex: -1"));
    assert!(panel.contains("function focusSearchResult(index)"));
    assert!(panel.contains("function moveSearchResultSelection(direction)"));
    assert!(panel.contains("function acceptSearchResult()"));
    assert!(panel.contains("onAccepted: root.acceptSearchResult()"));
    assert!(panel.contains("Keys.priority: Keys.BeforeItem"));
    assert!(panel.contains("Keys.onDownPressed: function(event)"));
    assert!(panel.contains("Keys.onUpPressed: function(event)"));
    assert!(panel.contains("root.moveSearchResultSelection(1)"));
    assert!(panel.contains("root.moveSearchResultSelection(-1)"));
    assert!(panel.contains("searchResultList.positionViewAtIndex(boundedIndex, ListView.Contain)"));
    assert!(panel.contains("function mapFocusLocation(entry)"));
    assert!(panel.contains("var focusLocation = mapFocusLocation(result)"));
    assert!(panel.contains("delegate: CursorSurface {"));
    assert!(panel.contains("hasCursor: resultButton.index === root.searchResultIndex"));
    assert!(panel.contains(
        "onPositionChanged: {\n                        if (resultButton.index !== root.searchResultIndex)\n                          root.focusSearchResult(resultButton.index)"
    ));
    assert!(!panel.contains("hasCursor: selected"));
    assert!(panel.contains("root.focusSearchResult(resultButton.index)"));
    assert!(panel.contains("root.selectMapLocation(resultButton.modelData)"));
    assert!(panel.contains("selectMapLocation(searchResults[activeIndex])"));
    assert!(panel.contains("root.selectMapLocation(root.searchResults[0])"));
    assert!(panel.contains("root.runAction(\"add\", root.mapSelection.timezone,"));
    assert!(panel.contains("message = \"That location is already added.\""));
    assert!(!panel.contains("root.runAction(\"add\", mapMarker.location.timezone"));
    assert!(!panel.contains("root.runAction(\"add\", payload.timezone, payload)"));
    assert!(panel.contains("if (root.mapSelection !== null) root.dismissMapSelection()"));
    assert!(panel.contains("onViewInteractionStarted: root.dismissMapSelection()"));
    assert!(panel.contains("id: searchResultOverlay"));
    assert!(panel.contains("id: searchResultTitleMetrics"));
    assert!(panel.contains("id: searchResultSubtitleMetrics"));
    assert!(panel.contains("function measuredSearchResultWidth()"));
    assert!(panel.contains("function searchModuleWidth(availableWidth)"));
    assert!(panel.contains("width: root.searchModuleWidth(Math.max(1,"));
    assert!(panel.contains("anchors.right: parent.right"));
    assert!(panel.contains("x: addSearchSurface.x"));
    assert!(panel.contains("width: addSearchSurface.width"));
    assert!(panel.contains("available * 0.38"));
    assert!(panel.contains("available * 0.75"));
    assert!(!panel.contains("function searchResultOverlayWidth(availableWidth)"));
    assert!(!panel.contains("width: Math.min(addField.width, Style.space(520))"));
    assert!(panel.contains(
        "height: Math.min(root.searchResults.length * Style.space(48), Style.space(240))"
    ));
    assert!(panel.contains("id: searchResultList"));
    assert!(!panel.contains("root.runAction(\"add\", searchResults[0].timezone"));
    assert!(!panel.contains("root.runAction(\"add\", root.searchResults[0].timezone"));
    assert!(!panel.contains("root.runAction(\"add\", resultButton.modelData.timezone"));
    assert!(panel.contains("name === \"pin\" || name === \"unpin\" || name === \"remove\""));
    assert!(panel.contains("command.push(String(timezone || \"\"))"));
    assert!(panel.contains("name === \"rename\""));
    assert!(panel.contains("result.label !== null && result.label !== undefined"));
    assert!(panel.contains("command.push(\"--new-label\", String(value || \"\"))"));
    assert!(panel.contains("function renameClock(clock, label)"));
    assert!(panel.contains("function focusSummaryLabelEditor()"));
    assert!(panel.contains("cell.focusLabelEditor(Qt.ShortcutFocusReason)"));
    assert!(panel.contains("id: summaryLabelInput"));
    assert!(panel.contains("id: cardLabelInput"));
    assert!(panel.contains("id: editModeButton"));
    assert!(panel.contains("property bool labelEditing: false"));
    assert!(panel.contains("id: summaryLabelMouse"));
    assert!(panel.contains("id: cardLabelMouse"));
    assert!(panel.contains("cursorShape: Qt.IBeamCursor"));
    assert!(panel.contains("font.capitalization: Font.AllUppercase"));
    assert!(panel.contains("font.letterSpacing: 1"));
    assert!(panel.contains("if (reason === Qt.MouseFocusReason)"));
    assert!(panel.contains("cardLabelInput.cursorPosition = cardLabelInput.text.length"));
    assert!(!panel.contains("visible: cardLabelInput.activeFocus"));
    assert!(!panel.contains("visible: summaryLabelInput.activeFocus"));
    assert!(!panel.contains("enabled: !root.labelEditorActive && !actionProcess.running"));
    assert!(panel.contains("function handlePanelPointerTap(panelX, panelY)"));
    assert!(panel.contains("function pointerInsideActiveEditor(panelX, panelY)"));
    assert!(panel.contains("id: focusDismissHandler"));
    assert!(panel.contains("parent: keyCatcher"));
    assert!(panel.contains("onTapped: function(eventPoint)"));
    assert!(
        panel.contains("root.handlePanelPointerTap(eventPoint.position.x, eventPoint.position.y)")
    );
    assert!(panel.contains(
        "keyboardCursorActive = false\n    keyboardClockIndex = -1\n    var pointerX = Number(panelX)"
    ));
    assert!(panel.contains("root.pointerInsideActiveEditor(pointerX, pointerY)"));
    assert!(panel.contains("keyCatcher.forceActiveFocus(Qt.MouseFocusReason)"));
    assert!(panel.contains("Accessible.description: \"Press Enter to save or Escape to cancel\""));
    assert!(panel.contains("id: timelineTickRepeater"));
    assert!(panel.contains("property var timelineHoverOwners: ({})"));
    assert!(panel.contains("import \"TimeRail.js\" as TimeRail"));
    assert!(panel.contains("function timelineHoverMatches(clock)"));
    assert!(panel.contains("TimelineHoverState.matchesIdentity("));
    assert!(panel.contains("timelineHoverOwners, conversionSource(clock)"));
    assert!(!panel.contains("id: timelinePointHover"));
    assert!(panel.contains("id: cardHoverHandler"));
    assert!(panel.contains("root.timelineHoverMatches(clockData)"));
    assert_eq!(
        panel
            .matches("root.updateTimelineHover(hoverOwner, null, false)")
            .count(),
        2,
        "destroyed and pooled card delegates must release hover ownership"
    );
    assert!(panel.contains("backendCommand,\n      \"scrub\""));
    assert!(panel.contains("function applyScrubSlot(slotIndex)"));
    assert!(panel.contains("if (scrubPreviewActive && scrubSelectedSlotIndex === index) return"));
    let apply_scrub_slot = panel
        .split("function applyScrubSlot(slotIndex) {")
        .nth(1)
        .and_then(|source| source.split("function cancelScrubPreview() {").next())
        .expect("applyScrubSlot function body");
    let invalidate_snapshot = apply_scrub_slot
        .find("invalidateSnapshotRequests()")
        .expect("starting a scrub preview must invalidate snapshot work");
    let capture_snapshot = apply_scrub_slot
        .find("scrubBaseSnapshot = snapshot")
        .expect("starting a scrub preview must capture its base snapshot");
    assert!(
        invalidate_snapshot < capture_snapshot,
        "in-flight snapshots must be invalidated before capturing the scrub preview base"
    );
    assert!(apply_scrub_slot.contains("var resumeSnapshotRequest = snapshotRequestPending"));
    assert!(apply_scrub_slot.contains("snapshotRequestReference = resumeSnapshotReference"));
    let cancel_scrub_preview = panel
        .split("function cancelScrubPreview() {")
        .nth(1)
        .and_then(|source| source.split("function lockScrubSelection() {").next())
        .expect("cancelScrubPreview function body");
    assert!(cancel_scrub_preview.contains("Qt.callLater(root.flushSnapshotRequest)"));
    let request_snapshot = panel
        .split("function requestSnapshot(referenceUtc) {")
        .nth(1)
        .and_then(|source| source.split("function flushSnapshotRequest() {").next())
        .expect("requestSnapshot function body");
    assert!(request_snapshot.contains(
        "if (scrubPreviewActive) {\n      snapshotRequestPending = true\n      snapshotRequestReference = reference\n      return"
    ));
    assert!(panel.contains("TimeRail.mergeSnapshot(scrubBaseSnapshot, scrubPayload, index)"));
    assert!(panel.contains("property real scrubAnchorMinute: 0"));
    assert!(panel.contains("readonly property bool scrubSourceIsSummary"));
    assert!(panel.contains("function clockDayLabel(clock)"));
    assert!(panel.contains("if (root.live && !root.scrubPreviewActive) return liveLabel"));
    assert!(panel.contains("TimeRail.relativeDayLabel(clock, root.clockForScrubSource())"));
    assert!(panel.contains("root.clockDayLabel(clockCell.clockData).toUpperCase()"));
    let scrub_ready = panel
        .split("readonly property bool scrubReady: {")
        .nth(1)
        .and_then(|source| source.split("readonly property var scrubAxisTicks:").next())
        .expect("scrubReady binding body");
    assert!(scrub_ready.contains("TimeRail.scrubPayloadReady(scrubPayload, snapshot,"));
    assert!(scrub_ready.contains("scrubBaseSnapshot, scrubSourceTimezone, scrubSourceKey"));
    let request_scrub = panel
        .split("function requestScrubFor(clock) {")
        .nth(1)
        .and_then(|source| source.split("function startScrubRequest() {").next())
        .expect("requestScrubFor function body");
    assert!(request_scrub.contains("String(scrubPayload.time_format || \"\") === timeFormat"));
    assert!(request_scrub.contains("TimeRail.payloadMatchesSnapshot(scrubPayload, snapshot)"));
    assert!(panel.contains("|| !TimeRail.payloadMatchesSnapshot(payload, root.snapshot)"));
    assert!(!panel.contains("scrubPayloadLocationSignature = root.scrubActiveLocationSignature"));
    assert!(!panel.contains("WORK HOURS 09:00–17:00"));
    assert!(panel.contains("readonly property bool localDayRulersVisible:"));
    assert!(panel.contains("mode === \"read\" && (scrubPreviewActive || !live)"));
    assert!(panel.contains("id: cardLocalDayRuler"));
    assert!(panel.contains("id: localDayTrack"));
    assert!(panel.contains("id: localSolarGlow"));
    assert!(panel.contains("Canvas {"));
    assert!(panel.contains("readonly property var localDaylight:"));
    assert!(panel.contains("TimeRail.localDaylight(clockData, root.snapshot.reference_utc)"));
    assert!(panel.contains("clockCell.localDaylight.curve_segments"));
    assert!(panel.contains("clockCell.localDaylight.daylight_intervals"));
    assert!(panel.contains("id: localDaylightEdgeRepeater"));
    assert!(!panel.contains("id: localSunriseCap"));
    assert!(!panel.contains("id: localSunsetCap"));
    assert!(panel.contains("clockCell.localDaylight.marker_light"));
    assert!(panel.contains("Behavior on color { ColorAnimation { duration: 150 } }"));
    assert!(!panel.contains("orientation: Gradient.Horizontal"));
    assert!(panel.contains("TimeRail.localDayPosition(clockCell.clockData.local_minutes)"));
    let local_day_ruler = panel
        .split("id: cardLocalDayRuler")
        .nth(1)
        .and_then(|source| source.split("id: localDayMarker").next())
        .expect("passive local-day ruler body");
    assert!(!local_day_ruler.contains("MouseArea"));
    let local_day_marker = panel
        .split("id: localDayMarker")
        .nth(1)
        .and_then(|source| source.split("Button {").next())
        .expect("local-day ruler marker body");
    assert!(local_day_marker.contains("height: Math.round(parent.height * 0.5)"));
    assert!(local_day_marker.contains("readonly property color nightTint:"));
    assert!(local_day_marker.contains("nightTint, clockSurface.color"));
    assert!(local_day_marker.contains("readonly property real restingOpacity:"));
    assert!(local_day_marker.contains("0.68 + sunlight * 0.26"));
    assert!(!local_day_marker.contains("Behavior on x"));
    assert!(!panel.contains("TimeRail.centeredSlotIndexAt("));
    assert!(!panel.contains("TimeRail.framePosition("));
    assert!(panel.contains("readonly property real scrubViewportMinute:"));
    assert!(
        panel.contains("TimeRail.buildMarkers(snapshot, scrubSourceTimezone, scrubViewportMinute)")
    );
    assert!(panel.contains(
        "TimeRail.axisTicks(scrubViewportMinute, String(snapshot.time_format || \"24h\"))"
    ));
    assert!(!panel.contains("HOVER TO COMPARE"));
    assert!(!panel.contains("CLICK TO SET"));
    assert!(panel.contains("id: scrubPlayhead"));
    assert!(panel.contains("readonly property real selectedX: railInset + railWidth / 2"));
    assert_eq!(
        panel
            .matches("visible: !(timelinePoint.sourcePoint && scrubPlayhead.visible)")
            .count(),
        2,
        "the fixed playhead must replace the overlapping source dot and stem"
    );
    let scrub_playhead = panel
        .split("id: scrubPlayhead")
        .nth(1)
        .and_then(|source| source.split("id: scrubValueBubble").next())
        .expect("fixed scrub playhead body");
    assert!(scrub_playhead.contains("x: Math.round(timelineView.selectedX - width / 2)"));
    assert!(panel.contains("readonly property bool sourceMarkerHovered:"));
    assert!(scrub_playhead
        .contains("readonly property bool linkedHovered: timelineView.sourceMarkerHovered"));
    assert!(scrub_playhead.contains("id: fixedPlayheadDot"));
    assert!(scrub_playhead.contains("scale: scrubPlayhead.linkedHovered ? 1.45 : 1"));
    assert!(scrub_playhead.contains(
        "y: scrubValueBubble.visible\n                  ? Style.space(34) : timelineView.railY - Style.space(5)"
    ));
    assert!(scrub_playhead.contains("y: timelineView.railY - scrubPlayhead.y - height / 2"));
    assert!(!scrub_playhead.contains("Behavior on x"));
    assert!(panel.contains("id: scrubValueBubble"));
    assert!(panel.contains("root.scrubPreviewActive || railMouse.activeFocus || !root.live"));
    assert!(panel.contains("TimeRail.selectionLabel(root.scrubPayload,"));
    assert!(panel.contains("Accessible.name: \"Compare times across the day\""));
    assert!(panel.contains("property real pressX: 0"));
    assert!(panel.contains("property int pressSlotIndex: -1"));
    assert!(panel.contains("TimeRail.draggedSlotIndexAt("));
    assert!(panel.contains("dragged = nextSlotIndex !== pressSlotIndex"));
    assert!(!panel.contains("Math.abs(mouse.x - pressX) >= 1"));
    assert!(panel.contains("previewAtDelta(mouse.x - pressX)"));
    assert!(panel.contains("onReleased: function(mouse)"));
    assert!(panel.contains("TimeRail.wheelSlotMotion("));
    assert!(panel.contains("parent: timelineView"));
    assert!(panel.contains("acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad"));
    assert!(panel.contains("id: verticalScrubWheel"));
    assert!(panel.contains("id: horizontalScrubWheel"));
    assert!(panel.contains("orientation: Qt.Vertical"));
    assert!(panel.contains("orientation: Qt.Horizontal"));
    assert_eq!(panel.matches("railMouse.handleWheel(event)").count(), 2);
    assert!(panel.contains("id: scrubWheelCommitTimer"));
    let lock_scrub_selection = panel
        .split("function lockScrubSelection() {")
        .nth(1)
        .and_then(|source| source.split("function moveScrubSelection(").next())
        .expect("lock scrub selection body");
    assert!(lock_scrub_selection.contains("live = false"));
    assert!(lock_scrub_selection.contains("requestSnapshot("));
    assert!(lock_scrub_selection.contains("if (!scrubSelectedFrame.reference_utc) {"));
    assert!(lock_scrub_selection
        .contains("TimeRail.mergeSnapshot(base, scrubPayload, scrubSelectedSlotIndex)"));
    assert!(lock_scrub_selection.contains("cancelScrubPreview()"));
    assert!(!panel.contains("function toggleLocationView()"));
    assert!(!panel.contains("function cycleLocationLayout()"));
    let shared_read_chrome = panel
        .split("id: readPage")
        .nth(1)
        .and_then(|source| source.split("id: timelineView").next())
        .expect("shared read-mode chrome");
    assert!(shared_read_chrome.contains("spacing: Style.space(18)"));
    assert!(shared_read_chrome.contains("height: Style.space(92)"));
    assert!(shared_read_chrome.contains("font.pixelSize: Style.space(52)"));
    assert!(shared_read_chrome.contains("anchors.topMargin: Style.space(7)"));
    let rail_mouse = panel
        .split("id: railMouse")
        .nth(1)
        .and_then(|source| source.split("hoverEnabled: true").next())
        .expect("time rail hit target geometry");
    assert!(rail_mouse.contains("x: timelineView.railInset"));
    assert!(rail_mouse.contains("y: timelineView.railY - Style.space(24)"));
    assert!(rail_mouse.contains("width: timelineView.railWidth"));
    assert!(rail_mouse.contains("height: Style.space(48)"));
    assert!(panel.contains("cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor"));
    assert!(!panel.contains("cursorShape: Qt.SizeHorCursor"));
    assert!(!panel.contains("stableLocationViewHeight"));
    let clock_rows = panel
        .split("id: clockRows")
        .nth(1)
        .and_then(|source| source.split("Repeater {").next())
        .expect("clock rows geometry");
    assert!(clock_rows.contains("height: root.clockViewportHeight"));
    assert!(clock_rows.contains("spacing: root.clockRowSpacing"));
    assert!(clock_rows.contains("model: Math.ceil(root.clocks.length / root.clockColumnCount)"));
    assert!(clock_rows.contains("reuseItems: true"));
    assert!(clock_rows.contains("cacheBuffer: root.clockRowHeight + root.clockRowSpacing"));
    assert!(clock_rows.contains("QQC.ScrollBar.vertical: QQC.ScrollBar"));
    assert!(panel.contains("ListView.onPooled: clockRow.resetRecycledState()"));
    assert!(panel.contains("ListView.onReused: clockRow.resetRecycledState()"));
    assert!(panel.contains("keyCatcher.forceActiveFocus(Qt.OtherFocusReason)"));
    assert!(panel.contains("cardTimeInput.text = String(clockData.time || \"\")"));
    assert!(panel.contains("readonly property real clockViewportHeight:"));
    assert!(panel
        .contains("Math.min(clockGridHeight, Math.max(0, readHeightLimit - readChromeHeight))"));
    assert!(panel.contains("clockRows.itemAtIndex("));
    assert!(!panel.contains("id: clockRowRepeater"));
    let clock_row = panel
        .split("id: clockRow\n")
        .nth(1)
        .and_then(|source| source.split("Repeater {").next())
        .expect("clock row geometry");
    assert!(clock_row.contains("anchors.left: parent.left"));
    assert!(clock_row.contains("height: root.clockRowHeight"));
    assert!(!clock_row.contains("anchors.horizontalCenter: parent.horizontalCenter"));
    assert!(panel.contains("readonly property string overflowDirection:"));
    assert!(panel.contains("root.timelineMarkerHovered(modelData)"));
    assert!(panel.contains("readonly property real markerCenterX:"));
    assert!(panel.contains("timelineView.railInset - Style.space(18)"));
    assert!(panel.contains("timelineView.railInset + timelineView.railWidth + Style.space(18)"));
    assert!(panel.contains("\"← \""));
    assert!(panel.contains("\"→ \""));
    let convert_process = panel
        .split("id: convertProcess")
        .nth(1)
        .and_then(|source| source.split("id: actionProcess").next())
        .expect("convert process body");
    assert!(convert_process.contains("root.live = false"));
    assert!(panel.contains("Keys.onLeftPressed"));
    assert!(panel.contains("Keys.onRightPressed"));
    assert!(panel.contains("function focusTimeRail()"));
    assert!(panel.contains("onTimelineRequested: root.focusTimeRail()"));
    assert!(panel.contains("root.lockScrubSelection()"));
    assert!(panel.contains("if (activeFocus) root.selectScrubSource(root.summary)"));
    assert!(panel.contains("root.selectScrubSource(clockCell.clockData)"));
    assert!(root.join("quattro/TimeRail.js").is_file());
    assert!(root.join("tests/time-rail.mjs").is_file());
    let apply_snapshot = panel
        .split("function applySnapshot(raw, manual)")
        .nth(1)
        .and_then(|source| {
            source
                .split("function requestSnapshot(referenceUtc)")
                .next()
        })
        .expect("applySnapshot function body");
    assert!(
        !apply_snapshot.contains("clearTimelineHover()"),
        "snapshot refreshes must preserve active card and timeline hover state"
    );
    let mode_changed = panel
        .split("onModeChanged: {")
        .nth(1)
        .and_then(|source| source.split("onCanAddChanged:").next())
        .expect("onModeChanged handler body");
    assert!(
        mode_changed.starts_with("\n    clearStatus()"),
        "mode transitions must discard status from the previous panel surface"
    );
    assert!(
        mode_changed.contains(
            "if (mode === \"add\") {\n      cancelScrubPreview()\n      clearTimelineHover()"
        ),
        "hover state should clear when leaving the clock and timeline view"
    );
    assert!(
        !mode_changed.starts_with("\n    clearTimelineHover()"),
        "switching between read and edit mode must preserve active hover state"
    );
    assert!(
        mode_changed.contains("if (mode === \"read\") Qt.callLater(root.restoreReadModeFocus)"),
        "every transition back to read mode must restore quick-entry focus"
    );
    assert!(panel.contains("function focusSummaryEditor(initialText)"));
    assert!(panel.contains("property bool snapshotLoaded: false"));
    assert!(panel.contains("property bool summaryFocusPending: false"));
    assert!(panel.contains("property bool summaryFocusScheduled: false"));
    assert!(panel.contains("summaryFocusSeed += String(initialText || \"\")"));
    assert!(panel.contains("if (summaryFocusScheduled) return"));
    assert!(panel.contains("if (!snapshotLoaded)"));
    assert!(panel.contains("if (summaryFocusPending) Qt.callLater(root.focusSummaryEditor)"));
    assert!(panel.contains("if (clocks.length === 0 && mode !== \"add\" && !summaryFocusPending)"));
    assert!(panel.contains("function focusAddField(selectExisting)"));
    assert!(panel.contains("property bool searchVisible: false"));
    assert!(panel.contains("function openSearch(initialText)"));
    assert!(panel.contains("function closeSearch()"));
    assert!(panel.contains("addField.forceActiveFocus(Qt.ShortcutFocusReason)"));
    assert!(panel.contains("if (mode === \"add\")"));
    assert!(panel.contains("if (mode === \"read\") focusSummaryEditor()"));
    assert!(panel.contains("Qt.callLater(function()"));
    assert!(panel.contains("property bool editorRefreshPending: false"));
    assert!(panel.contains("property bool labelEditorActive: false"));
    assert!(panel.contains("property bool snapshotRequestPending: false"));
    assert!(panel.contains("property int snapshotStateGeneration: 0"));
    assert!(panel.contains("property int convertActiveGeneration: -1"));
    assert!(panel.contains("property string invalidConversionSource: \"\""));
    assert!(panel.contains("snapshotLoaded = true\n      invalidConversionSource = \"\""));
    assert!(panel.contains("if (!snapshotLoaded || !timezoneName || !reference"));
    assert!(panel.contains("if (timeEditorActive || labelEditorActive)"));
    assert!(panel.contains("(root.timeEditorActive || root.labelEditorActive)"));
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
    assert!(panel.contains("if (mode !== \"add\" || !searchVisible) return"));
    assert!(panel.contains("if (mode !== \"add\" || !searchVisible || !query"));
    assert!(panel.contains("searchDebounce.stop()"));
    assert!(panel.contains("searchResultsQuery === query"));
    assert!(panel.contains("searchSubmitQuery = query"));
    assert!(panel.contains("Qt.callLater(root.startSearch)"));
    assert!(panel.contains("if (root.mode === \"add\""));
    assert!(panel.contains("onTextChanged: root.searchTextChanged()"));
    assert!(panel.contains("id: addBackButton"));
    assert!(panel.contains("id: addBackButtonSurface"));
    assert!(panel.contains("anchors.fill: addBackButton"));
    assert!(panel.contains("id: addSearchButton"));
    assert!(panel.contains("id: addSearchButtonSurface"));
    assert!(panel.contains("visible: addSearchButton.visible"));
    assert!(panel.contains("anchors.fill: addSearchButton"));
    assert!(panel.contains("id: addSearchSurface"));
    assert!(panel.contains("id: addSearchCloseButton"));
    assert!(panel.contains("id: addSearchCloseHover"));
    assert!(panel.contains("id: addSearchCloseGlyph"));
    assert!(panel.contains("id: addSearchCloseMouse"));
    assert!(
        panel.contains("rightPadding: addSearchCloseButton.width + Style.spacing.controlPaddingX")
    );
    assert!(panel.contains("Color.popups.background"));
    assert!(panel.contains("readonly property color searchSurfaceColor"));
    assert!(panel.contains("return Qt.rgba(mixed.r, mixed.g, mixed.b, 1)"));
    assert!(panel.contains("color: root.searchSurfaceColor"));
    assert!(panel.contains("visible: root.searchVisible"));
    assert!(panel.contains("visible: !root.searchVisible"));
    assert!(panel.contains("width: Style.space(28)"));
    assert!(panel.contains("? Style.hoverFillFor(root.contentForeground, Color.accent)"));
    assert!(panel.contains("opacity: addSearchCloseMouse.containsMouse ? 1 : 0.82"));
    assert!(!panel.contains("tooltipText: \"Close search\""));
    assert!(panel.contains("tooltipText: \"Search locations\""));
    assert!(!panel
        .contains("tooltipText: root.searchVisible ? \"Close search\" : \"Search locations\""));
    assert!(!panel.contains("diameterRatio:"));
    assert!(panel.contains("root.focusSearchResult(0)"));
    assert!(!panel.contains("mapCanvas.focusOnLocations(root.searchResults)"));
    assert!(panel.contains("mapCanvas.focusOnLocations([focusLocation])"));
    assert!(panel.contains("command.push(\"--at\", String(snapshot.reference_utc))"));
    assert!(panel.contains("root.mode = \"read\""));
    assert!(panel.contains("font.pixelSize: Style.font.title"));
    assert!(panel.contains("font.pixelSize: Style.font.bodySmall"));
    assert!(panel.contains("font.bold: false"));
    assert!(panel.contains("readonly property color mapLabelForeground"));
    assert!(panel.contains("Qt.lighter(contentForeground, 1.20)"));
    assert!(panel.contains("opacity: 0.88"));
    assert!(!panel.contains("onHovered: function(isHovered)"));
    assert!(!panel.contains("Drag to rotate  ·  Scroll to zoom"));
    assert!(panel.contains("visible: root.summary.pinned === true"));
    assert!(panel.contains("onClicked: root.togglePin(root.summary)"));
    assert!(panel.contains("readonly property bool showOpenMeteoAttribution"));
    assert!(panel.contains("id: openMeteoAttribution"));
    assert!(panel.contains("visible: root.showOpenMeteoAttribution"));
    assert!(panel.contains("text: \"Location data by Open-Meteo\""));
    assert!(panel.contains("https://open-meteo.com/"));
    assert!(!panel.contains("<a href=\"https://open-meteo.com/\">"));
    assert!(!panel.contains("resultButton.modelData.open_meteo_attribution"));
    assert!(panel.contains("readonly property string currentLocationTitle"));
    assert!(panel.contains("readonly property string currentTimezoneMetadata"));
    assert!(panel.contains("root.weatherDetailOpen ? 0 : 8"));
    assert!(panel.contains("visible: root.mode !== \"add\" && !root.weatherDetailOpen"));
    assert!(panel.contains("text: root.currentLocationTitle"));
    assert!(panel.contains("font.pixelSize: Style.space(18)"));
    assert!(panel.contains("text: root.currentTimezoneMetadata"));
    assert!(panel.contains("id: clockRows"));
    assert!(panel.contains("anchors.horizontalCenter: parent.horizontalCenter"));
    assert!(panel.contains("id: clockSurface"));
    assert!(panel.contains("readonly property color restingFill:"));
    assert!(panel.contains("root.mixColorWithAlpha(restingFill, hoverFill, 0.18)"));
    assert!(panel.contains("border.width: clockCell.hasKeyboardCursor"));
    assert!(panel.contains("radius: Style.cornerRadius"));
    assert!(panel.contains("backendCommand, \"weather\""));
    assert!(panel.contains("property bool weatherRequestPending: false"));
    assert!(panel.contains("weatherRefreshMilliseconds: 15 * 60 * 1000"));
    assert!(panel.contains("weatherFreshnessCheckMilliseconds: 30 * 1000"));
    assert!(panel.contains("function weatherFor(clock)"));
    assert!(panel.contains("function weatherGlyph(item)"));
    assert!(panel.contains("function weatherGlyphColor(item)"));
    assert!(panel.contains("function weatherTemperatureCompact(value)"));
    assert!(panel.contains("readonly property bool weatherEnabled:"));
    let weather_presentation = panel
        .split("readonly property bool weatherPresentationActive:")
        .nth(1)
        .and_then(|source| {
            source
                .split("readonly property int weatherRefreshMilliseconds:")
                .next()
        })
        .expect("weather presentation binding");
    assert!(weather_presentation.contains("root.weatherEnabled && root.live"));
    assert!(panel.contains("root.setting(\"showWeather\", true) !== false"));
    assert!(panel.contains("function clearWeatherState()"));
    assert!(panel.contains("if (!weatherEnabled)"));
    assert!(panel.contains("onWeatherEnabledChanged:"));
    assert!(panel.contains("readonly property string weatherUnitOverride"));
    assert!(panel.contains("snapshot.weather_unit"));
    assert!(panel.contains("weatherUnitOverride === \"imperial\""));
    assert!(panel.contains("weatherUnitOverride !== \"metric\""));
    assert!(panel.contains("function localeTerritory(localeName)"));
    assert!(panel.contains("var parts = name.split(/[-_]/)"));
    assert!(panel.contains("[\"US\", \"LR\", \"MM\"].indexOf(territory) !== -1"));
    assert!(!panel.contains("/^en[_-]US"));
    assert!(panel.contains("id: summaryMetadataLine"));
    assert!(panel.contains("height: Style.space(92)"));
    assert!(panel.contains("id: summaryWeatherLine"));
    assert!(panel.contains("enabled: summaryClock.weatherData !== null"));
    assert!(panel.contains("id: summaryWeatherGlyph"));
    assert!(panel.contains("color: root.weatherGlyphColor(summaryClock.weatherData)"));
    assert!(panel.contains("onClicked: root.showWeatherDetail(root.summary)"));
    assert!(panel.contains("id: cardTitle"));
    assert!(panel.contains("id: cardMetadataRow"));
    assert!(panel.contains("id: cardWeatherBlock"));
    assert!(panel.contains("id: cardWeatherButton"));
    assert!(panel.contains("text: clockCell.weatherData\n                                    ? root.weatherTemperatureCompact("));
    assert!(panel.contains("width: cardWeatherContent.implicitWidth + Style.space(10)"));
    assert!(panel.contains("id: cardWeatherTemperature"));
    assert!(panel.contains("id: cardWeatherGlyph"));
    assert!(panel.contains("color: root.weatherGlyphColor(clockCell.weatherData)"));
    assert!(panel.contains("onClicked: root.showWeatherDetail(clockCell.clockData)"));
    assert!(panel.contains("anchors.verticalCenter: parent.verticalCenter"));
    assert!(panel.contains("property string weatherDetailKey: \"\""));
    assert!(!panel.contains("weatherDetailRetainedPanelHeight"));
    assert!(panel.contains("readonly property bool weatherDetailOpen:"));
    assert!(panel.contains("readonly property var weatherDetailHourlyForecast:"));
    assert!(panel.contains("root.weatherUtcMoment(forecast[i].reference_utc)"));
    assert!(panel.contains("forecastStart + 3600000 > currentMoment"));
    assert!(panel.contains("currentAndFuture.push(forecast[i])"));
    assert!(panel.contains("readonly property var weatherDetailDailyForecast:"));
    assert!(panel.contains("readonly property string weatherDetailLocalDate:"));
    assert!(panel.contains("readonly property int weatherDetailTodayIndex:"));
    assert!(panel.contains("readonly property var weatherDetailToday:"));
    assert!(panel.contains("function showWeatherDetail(clock)"));
    assert!(panel.contains("function dismissWeatherDetail()"));
    assert!(panel.contains("Style.space(root.weatherDetailOpen ? 840 : 960)"));
    assert!(panel.contains("contentHeight: panel.fittedContentHeight("));
    assert!(panel.contains("if (root.weatherDetailOpen) root.dismissWeatherDetail()"));
    assert!(panel.contains("id: weatherDetailPage"));
    assert!(panel.contains("id: weatherDetailBack"));
    assert!(panel.contains("id: weatherDetailStats"));
    assert!(panel.contains("id: weatherDetailFacts"));
    assert!(panel.contains("id: weatherDetailFactGrid"));
    assert!(panel.contains("id: weatherDetailHourlyRow"));
    assert!(panel.contains("id: weatherDetailForecastRow"));
    assert!(panel.contains("text: \"Hourly\""));
    assert!(panel.contains("text: \"4-day forecast\""));
    assert!(panel.contains("root.weatherDetailData.apparent_temperature_celsius"));
    assert!(panel.contains("root.weatherDetailData.wind_speed_kmh"));
    assert!(panel.contains("root.weatherDetailData.wind_direction_degrees"));
    assert!(panel.contains("root.weatherDetailData.wind_gusts_kmh"));
    assert!(panel.contains("root.weatherDetailData.relative_humidity_percent"));
    assert!(panel.contains("root.weatherDetailData.pressure_hpa"));
    assert!(panel.contains("root.weatherDetailData.visibility_meters"));
    assert!(panel.contains("function weatherProbability(value)"));
    let weather_signature = panel
        .split("function weatherSignature()")
        .nth(1)
        .and_then(|source| source.split("function weatherFor(clock)").next())
        .expect("weather signature body");
    assert!(weather_signature.contains("String(entry.date || \"\")"));
    assert!(weather_signature.contains("Math.floor(localMinutes / 60)"));
    assert!(weather_signature.contains("entry.utc_offset_seconds"));
    assert!(panel.contains("function weatherUtcMoment(value)"));
    assert!(panel.contains("function weatherHourlyIsCurrent(item)"));
    assert!(panel.contains("item ? item.reference_utc : \"\""));
    assert!(panel.contains("function weatherNextHourForecast()"));
    assert!(panel.contains("var nextHour = weatherNextHourForecast()"));
    assert!(!panel.contains("if (index === 0) return \"NOW\""));
    assert!(!panel.contains("weatherDetailHourlyForecast[0].precipitation_probability_percent"));
    assert!(panel.contains("function weatherPressure(value)"));
    assert!(panel.contains("function weatherVisibility(value)"));
    assert!(panel.contains("function weatherUv(value)"));
    assert!(panel.contains("id: weatherProviderAttribution"));
    assert!(panel.contains(
        "id: weatherProviderAttribution\n                anchors.verticalCenter: parent.verticalCenter"
    ));
    assert!(panel.contains("visible: root.mode !== \"add\" && root.weatherEnabled && root.live"));
    assert!(panel.contains("? \"Open-Meteo\""));
    assert!(panel.contains("tooltipText: \"Weather data by Open-Meteo\""));
    assert!(panel.contains("background: \"transparent\""));
    assert!(panel.contains("fontSize: Style.fontPx(0.75)"));
    assert!(panel.contains("horizontalPadding: Style.space(4)"));
    assert!(panel.contains("verticalPadding: Style.space(1)"));
    assert!(!panel.contains("id: weatherAttributionSlot"));
    let header_start = panel.find("id: headerStart").unwrap();
    let attribution = panel.find("id: weatherProviderAttribution").unwrap();
    let read_page = panel.find("id: readPage").unwrap();
    assert!(header_start < attribution && attribution < read_page);
    assert!(panel.contains("function mixColor("));
    assert!(panel.contains("onClicked: Qt.openUrlExternally(\"https://open-meteo.com/\")"));
    assert!(!panel.contains("&& (root.weatherLocations.length > 0 || root.weatherError)"));
    assert!(panel.contains("&& root.weather.disabled !== true"));
    assert!(panel.contains("interval: root.weatherFreshnessCheckMilliseconds"));
    assert!(panel.contains("onTriggered: root.requestWeather(false)"));

    let city_data_path = root.join("data/featured-cities.json");
    let city_data: Value = serde_json::from_str(
        &fs::read_to_string(&city_data_path).expect("read featured-city catalogue"),
    )
    .expect("parse featured-city catalogue");
    let cities = city_data.as_array().expect("featured-city array");
    assert!(cities.len() >= 300);
    assert!(cities.iter().any(|city| {
        city["title"] == "London" && city["minimum_zoom"].as_f64().is_some_and(|zoom| zoom < 1.0)
    }));
    assert!(cities.iter().any(|city| {
        city["title"] == "Bern" && city["minimum_zoom"].as_f64().is_some_and(|zoom| zoom > 4.0)
    }));
    assert!(root.join("scripts/build-featured-cities.mjs").is_file());

    assert!(qml.contains("function editCurrentTime()"));
    assert!(qml.contains("function edit(): void { root.editCurrentTime() }"));
    assert!(qml.contains("panelLoader.item.opened && panelLoader.item.live"));
    assert!(qml.contains("onDateChanged: root.refresh()"));
    assert!(qml.contains("readonly property int maximumVisiblePinnedClocks: 3"));
    assert!(qml.contains("pinnedClocks.slice(0, maximumVisiblePinnedClocks)"));
    assert!(qml.contains("model: root.visiblePinnedClocks"));
    assert!(qml.contains("visible: root.hiddenPinnedClockCount > 0"));
    assert!(qml.contains("text: \"+\" + String(root.hiddenPinnedClockCount)"));
    assert!(!qml.contains("model: root.pinnedClocks"));
    assert!(qml.contains("text: pinnedClock.modelData.code"));
    assert!(qml.contains("text: pinnedClock.modelData.time"));

    for (filename, width, height) in [
        ("world-map-preview.png", 2048, 1024),
        ("world-map.png", 8192, 4096),
    ] {
        let map_path = root.join("assets").join(filename);
        assert!(map_path.is_file(), "missing {}", map_path.display());
        let map = fs::read(&map_path).expect("read globe texture");
        assert!(map.len() >= 24, "globe texture is not a complete PNG");
        assert_eq!(&map[1..4], b"PNG");
        assert_eq!(u32::from_be_bytes(map[16..20].try_into().unwrap()), width);
        assert_eq!(u32::from_be_bytes(map[20..24].try_into().unwrap()), height);
    }

    let map_source_path = root.join("assets/world-map.svg");
    let map_source = fs::read_to_string(&map_source_path).expect("read globe map source");
    assert!(map_source.contains("Natural Earth v5.1.2"));
    assert!(
        map_source.matches(',').count() > 500_000,
        "globe map source has regressed to simplified geography"
    );
    let map_source_builder = root.join("scripts/build-world-map-source.mjs");
    let map_source_builder =
        fs::read_to_string(map_source_builder).expect("read globe source builder");
    assert!(map_source_builder.contains("natural-earth-vector/v5.1.2"));
    assert!(map_source_builder
        .contains("4caf607838c3cfd211a52edda259e315513390a717a11067c5eb280f766cfb78"));

    let key_catcher_path = qml_path.parent().unwrap().join("WorldClockKeyCatcher.qml");
    assert!(
        key_catcher_path.is_file(),
        "missing {}",
        key_catcher_path.display()
    );
    let key_catcher = fs::read_to_string(key_catcher_path).expect("read panel key catcher");
    assert!(key_catcher.contains("property bool directTextInput: false"));
    assert!(key_catcher.contains("typedText.trim() !== \"\""));
    assert!(key_catcher.contains("event.key >= Qt.Key_0 && event.key <= Qt.Key_9"));
    assert!(key_catcher.contains("Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier"));
    assert!(panel.contains("WorldClockKeyCatcher {"));
    assert!(panel.contains("directTextInput: (root.mode === \"read\" || root.mode === \"add\")"));
    assert!(panel.contains("onMoveRequested: function(dx, dy)"));
    assert!(panel.contains("onActivateRequested: root.activateKeyboardCursor()"));
    assert!(panel.contains("onDeleteRequested: root.deleteKeyboardCursor()"));
    assert!(panel.contains("function moveKeyboardCursor(dx, dy)"));
    assert!(panel.contains("function activateKeyboardCursor()"));
    assert!(panel.contains("function deleteKeyboardCursor()"));
    assert!(panel.contains("readonly property bool hasKeyboardCursor:"));
    assert!(panel.contains("if (!root.searchVisible) root.openSearch(text)"));
    assert!(panel.contains("property string summaryFocusSeed: \"\""));
    assert!(panel.contains("function focusSummaryEditor(initialText)"));
    assert!(panel.contains("if (root.mode === \"read\" && /^[0-9]$/.test(text))"));
    assert!(panel.contains("root.focusSummaryEditor(text)"));
    assert!(panel.contains("function isLetterKey(text)"));
    assert!(panel.contains("value.toLowerCase() !== value.toUpperCase()"));
    assert!(panel.contains("if (root.mode === \"read\" && root.isLetterKey(text))"));
    assert!(panel.contains("root.openSearchFromRead(text)"));
    assert!(panel.contains("property bool skipNextGlobeOpeningFlight: false"));
    assert!(panel.contains("function enterAddMode(skipOpeningFlight)"));
    assert!(panel.contains("function openSearchFromRead(initialText)"));
    assert!(panel.contains("enterAddMode(true)\n    openSearch(initialText)"));
    assert!(panel.contains("var skipOpeningFlight = skipNextGlobeOpeningFlight"));
    assert!(panel.contains("mapCanvas.showOpeningOverview(target.latitude, target.longitude)"));
    assert_eq!(
        panel.matches("onClicked: root.enterAddMode(false)").count(),
        2
    );

    let action_process = panel
        .split("id: actionProcess")
        .nth(1)
        .and_then(|source| source.split("id: searchProcess").next())
        .expect("action process body");
    let successful_add = action_process
        .split("if (root.actionName === \"add\")")
        .nth(1)
        .expect("successful add branch");
    assert!(
        successful_add.contains("Qt.callLater(root.restoreReadModeFocus)"),
        "a successful add must return keyboard focus to the read-mode dispatcher"
    );

    let globe_path = qml_path.parent().unwrap().join("Globe.qml");
    assert!(globe_path.is_file(), "missing {}", globe_path.display());
    let globe = fs::read_to_string(&globe_path).expect("read globe component");
    assert!(globe.contains("ShaderEffect"));
    assert!(globe.contains("DragHandler"));
    assert!(globe.contains("id: fallbackMap"));
    assert!(globe.contains("TapHandler {\n    parent: root.shaderAvailable ? sphere : fallbackMap"));
    assert!(globe.contains("var rootPoint = parent.mapToItem(root, eventPoint.position)"));
    assert!(
        globe.contains("WheelHandler {\n    parent: root.shaderAvailable ? sphere : fallbackMap")
    );
    assert!(globe.contains("function project(latitudeDegrees, longitudeDegrees)"));
    assert!(globe.contains("function locationAt(viewX, viewY)"));
    assert!(globe.contains("function normalizedWheelDelta(angleDelta, pixelDelta)"));
    assert!(globe.contains("((rawDelta + 180) % 360 + 360) % 360 - 180"));
    assert!(globe.contains("function focusOnLocations(locations)"));
    assert!(globe.contains("locations.length === 1 ? Math.max(zoom, view.zoom) : view.zoom"));
    assert!(globe.contains("focusOn(view.latitude, view.longitude, targetZoom)"));
    assert!(globe.contains("function minimumAngularCenter(vectors)"));
    assert!(globe.contains("minimumDepth: minimumDepth"));
    assert!(globe.contains("event.pixelDelta.y"));
    assert!(globe.contains("property real openingZoom: 2.2"));
    assert!(globe.contains("property real openingOverviewZoom: 0.94"));
    assert!(globe.contains("property real openingSpinDegrees: 104"));
    assert!(globe.contains("property int openingFlightDuration: 1080"));
    assert!(globe.contains("property real zoom: openingOverviewZoom"));
    assert!(globe.contains("property real diameterRatio: 0.86"));
    assert!(globe.contains("Math.min(width, height) * diameterRatio"));
    assert!(globe.contains("readonly property bool openingFlightRunning: openingMotion.running"));
    assert!(!globe.contains("openingHoldDuration"));
    assert!(globe.contains("function showOpeningOverview(latitudeDegrees, longitudeDegrees)"));
    assert!(globe.contains("function settleOn(latitudeDegrees, longitudeDegrees)"));
    assert!(globe.contains("if (!showOpeningOverview(targetLatitude, targetLongitude)) return"));
    assert!(globe.contains("longitude = targetLongitude - openingSpinDegrees"));
    assert!(globe.contains("id: openingMotion"));
    let opening_motion = globe
        .split("id: openingMotion")
        .nth(1)
        .expect("opening motion")
        .split("id: focusMotion")
        .next()
        .expect("opening motion body");
    assert!(!opening_motion.contains("PauseAnimation"));
    assert_eq!(
        opening_motion
            .matches("duration: root.openingFlightDuration")
            .count(),
        3
    );
    assert_eq!(
        opening_motion
            .matches("easing.type: Easing.InOutSine")
            .count(),
        3
    );
    assert!(globe.contains("function stopMotion()"));
    assert!(globe.contains("property real maximumZoom: 4.8"));
    assert!(globe.contains("property bool highResolutionEnabled: true"));
    assert!(globe.contains("property bool previewReady: false"));
    assert!(globe.contains("function updatePreviewReadiness()"));
    assert!(globe.contains("mapTexture.status !== Image.Ready"));
    assert!(globe.contains("fallbackMap.status !== Image.Ready"));
    assert_eq!(
        globe
            .matches("onStatusChanged: root.updatePreviewReadiness()")
            .count(),
        2
    );
    assert!(globe.contains("readonly property url textureSource: highResolutionEnabled"));
    assert!(globe.contains("Qt.resolvedUrl(\"../assets/world-map-preview.png\")"));
    assert!(globe.contains("readonly property int textureWidth: highResolutionEnabled"));
    assert!(globe.contains("readonly property int textureHeight: highResolutionEnabled"));
    assert_eq!(globe.matches("source: root.textureSource").count(), 2);
    assert_eq!(
        globe.matches("sourceSize.width: root.textureWidth").count(),
        2
    );
    assert_eq!(
        globe
            .matches("sourceSize.height: root.textureHeight")
            .count(),
        2
    );
    assert_eq!(globe.matches("retainWhileLoading: true").count(), 2);
    assert_eq!(globe.matches("asynchronous: true").count(), 2);
    assert!(globe.contains("../assets/globe.frag.qsb"));
    assert!(globe.contains("visible: !root.shaderAvailable"));

    let shader_source = root.join("assets/globe.frag");
    let shader_bundle = root.join("assets/globe.frag.qsb");
    assert!(
        shader_source.is_file(),
        "missing {}",
        shader_source.display()
    );
    assert!(
        shader_bundle.is_file(),
        "missing {}",
        shader_bundle.display()
    );
    assert!(
        fs::metadata(&shader_bundle)
            .expect("read shader bundle")
            .len()
            > 1_000,
        "compiled globe shader is unexpectedly small"
    );
    let shader = fs::read_to_string(&shader_source).expect("read globe shader source");
    assert!(shader.contains("gradientX.x -= round(gradientX.x)"));
    assert!(shader.contains("gradientY.x -= round(gradientY.x)"));
    assert!(shader.contains("textureGrad(source, textureCoordinate, gradientX, gradientY)"));
}

#[test]
fn globe_artifact_freshness_checks_are_mandatory_and_reproducible() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ci = fs::read_to_string(root.join("scripts/ci.sh")).expect("read local CI script");
    assert!(ci.contains("run node scripts/build-world-map-source.mjs --check"));
    assert!(ci.contains("run node scripts/build-featured-cities.mjs --check"));
    assert!(ci.contains("run scripts/check-globe-artifacts.sh"));
    assert!(!ci.contains("command -v qsb"));
    assert!(!ci.contains("command -v rsvg-convert"));

    let checker_path = root.join("scripts/check-globe-artifacts.sh");
    let checker = fs::read_to_string(&checker_path).expect("read globe artifact checker");
    assert!(checker.contains("ubuntu@sha256:"));
    assert!(checker.contains("qt6-shader-baker=6.10.2-1"));
    assert!(checker.contains("spirv-tools=2026.1-1"));
    assert!(checker.contains("librsvg2-bin=2.61.3+dfsg-3"));
    assert!(checker.contains("scripts/build-globe-shader.sh --check"));
    assert!(checker.contains("scripts/build-world-map.sh --check"));

    let map_builder =
        fs::read_to_string(root.join("scripts/build-world-map.sh")).expect("read map builder");
    assert!(map_builder.contains("world-map-preview.png"));
    assert!(map_builder.contains("PREVIEW_WIDTH=2048"));
    assert!(map_builder.contains("PREVIEW_HEIGHT=1024"));
    assert_ne!(
        fs::metadata(&checker_path)
            .expect("read globe artifact checker metadata")
            .permissions()
            .mode()
            & 0o111,
        0,
        "globe artifact checker must be executable"
    );

    let workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");
    assert!(workflow.contains("runs-on: ubuntu-24.04"));
    assert!(workflow.contains("node-version: 26.7.0"));
    assert!(workflow.contains("command -v node"));
}

#[test]
fn return_to_live_releases_the_active_editor_before_requesting_now() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");
    let return_to_live = panel
        .split("function returnToLive() {")
        .nth(1)
        .and_then(|source| source.split("function convertFrom(").next())
        .expect("returnToLive function body");
    let invalidate = return_to_live
        .find("invalidateSnapshotRequests()")
        .expect("returning live must invalidate converted snapshot work");
    let release_focus = return_to_live
        .find("keyCatcher.forceActiveFocus(Qt.MouseFocusReason)")
        .expect("returning live must release any active inline editor");
    let request_now = return_to_live
        .find("requestLiveSnapshot()")
        .expect("returning live must request the current instant");

    assert!(
        invalidate < release_focus && release_focus < request_now,
        "converted work must be invalidated and editor focus released before requesting now"
    );
}

#[test]
fn closing_the_panel_discards_selected_time_and_requests_now() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");
    let reset_on_close = panel
        .split("function resetTimeOnPanelClose() {")
        .nth(1)
        .and_then(|source| source.split("function convertFrom(").next())
        .expect("resetTimeOnPanelClose function body");
    let cancel_preview = reset_on_close
        .find("cancelScrubPreview()")
        .expect("closing must first cancel an in-progress preview");
    let invalidate = reset_on_close
        .find("invalidateSnapshotRequests()")
        .expect("closing must invalidate pending conversions even while live");
    let live_return = reset_on_close
        .find("if (live) return")
        .expect("closing may skip the live refresh after invalidation");
    let restore_live = reset_on_close
        .find("live = true")
        .expect("closing must discard a locked selected time");
    let request_now = reset_on_close
        .find("requestLiveSnapshot()")
        .expect("closing must refresh the live instant");

    assert!(cancel_preview < invalidate && invalidate < live_return);
    assert!(live_return < restore_live && restore_live < request_now);

    let opened_changed = panel
        .split("onOpenedChanged: {")
        .nth(1)
        .and_then(|source| source.split("onWeatherEnabledChanged:").next())
        .expect("panel opened-state handler");
    assert!(opened_changed.contains("if (!opened)"));
    assert!(opened_changed.contains("resetTimeOnPanelClose()"));
}

#[test]
fn escape_unwinds_preview_and_locked_time_before_closing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");
    let dismiss_time = panel
        .split("function dismissTransientTime() {")
        .nth(1)
        .and_then(|source| source.split("function ").next())
        .expect("dismissTransientTime function body");
    let cancel_preview = dismiss_time
        .find("if (scrubPreviewActive)")
        .expect("Escape must cancel an in-progress scrub preview");
    let return_live = dismiss_time
        .find("if (!live)")
        .expect("Escape must return a locked selection to live time");
    assert!(cancel_preview < return_live);
    assert!(dismiss_time.contains("cancelScrubPreview()"));
    assert!(dismiss_time.contains("returnToLive()"));

    let close_request = panel
        .split("onCloseRequested: {")
        .nth(1)
        .and_then(|source| source.split("onTabRequested:").next())
        .expect("panel Escape handler");
    assert!(close_request.contains("!root.dismissTransientTime()"));
    assert!(close_request.contains("root.close()"));

    let rail_escape = panel
        .split("id: railMouse")
        .nth(1)
        .and_then(|source| source.split("id: scrubWheelCommitTimer").next())
        .expect("time-rail keyboard handlers");
    assert!(rail_escape.contains("scrubWheelCommitTimer.stop()"));
    assert!(rail_escape.contains("wheelSlotRemainder = 0"));
    assert!(rail_escape.contains("wheelMovedSelection = false"));
    assert!(rail_escape.contains("root.dismissTransientTime()"));
}

#[test]
fn add_confirmation_and_core_actions_are_keyboard_reachable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");
    let key_catcher = fs::read_to_string(root.join("quattro/WorldClockKeyCatcher.qml"))
        .expect("read panel key catcher");

    let accept_search = panel
        .split("function acceptSearchResult() {")
        .nth(1)
        .and_then(|source| source.split("function hasMapCoordinate(").next())
        .expect("search acceptance body");
    assert!(accept_search.contains("if (mapSelection !== null)"));
    assert!(accept_search.contains("addMapSelection()"));
    assert!(accept_search.contains("focusMapSelectionAction()"));

    let activate_cursor = panel
        .split("function activateKeyboardCursor() {")
        .nth(1)
        .and_then(|source| source.split("function deleteKeyboardCursor() {").next())
        .expect("keyboard activation body");
    assert!(activate_cursor.contains("if (mapSelection !== null) addMapSelection()"));
    assert!(panel.contains("function addMapSelection()"));
    assert!(panel.contains("function focusMapSelectionAction()"));
    assert!(panel.contains("mapSelectionAddButton.forceActiveFocus(Qt.ShortcutFocusReason)"));
    assert!(panel.contains("onClicked: root.addMapSelection()"));

    assert!(key_catcher.contains("signal editRequested()"));
    assert!(key_catcher.contains("signal liveRequested()"));
    assert!(key_catcher.contains("signal timelineRequested()"));
    assert!(key_catcher.contains("event.key === Qt.Key_Delete"));
    assert!(key_catcher.contains("event.key === Qt.Key_F2"));
    assert!(key_catcher.contains("event.key === Qt.Key_Home"));
    assert!(key_catcher.contains("event.key === Qt.Key_T"));
    assert!(key_catcher.contains("event.modifiers & Qt.ControlModifier"));

    assert!(panel.contains("onEditRequested: root.toggleEditMode()"));
    assert!(panel.contains("onLiveRequested: root.returnToLive()"));
    assert!(panel.contains("onTimelineRequested: root.focusTimeRail()"));
    assert!(panel.contains("function toggleKeyboardCursorPin()"));
    assert!(panel.contains("root.toggleKeyboardCursorPin()"));
    assert!(panel.contains("Press Ctrl+T to focus"));
}

#[test]
fn time_editors_can_be_cancelled_with_escape() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");

    assert!(panel.contains("function cancelTimeEditor(input, currentTime, source)"));
    assert_eq!(
        panel.matches("root.cancelTimeEditor(").count(),
        2,
        "the summary and every recycled location time editor need Escape cancellation"
    );
    assert!(panel.contains("Keys.onEscapePressed: function(event)"));
}

#[test]
fn editor_focus_keeps_non_text_shortcuts_reachable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel =
        fs::read_to_string(root.join("quattro/Panel.qml")).expect("read native panel source");

    let shortcuts = panel
        .split("id: editorShortcutLayer")
        .nth(1)
        .expect("focus-independent editor shortcuts");
    assert!(shortcuts.contains("sequence: \"F2\""));
    assert!(shortcuts.contains("sequence: \"Ctrl+T\""));
    assert!(shortcuts.contains("sequence: \"Home\""));
    assert!(shortcuts.contains("root.editorActive"));
    assert!(shortcuts.contains("!root.live || root.scrubPreviewActive"));
    assert!(!shortcuts.contains("sequence: \"Delete\""));
}
