use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    assert_eq!(manifest["license"], "MIT AND ODbL-1.0");
    assert_eq!(manifest["kinds"], serde_json::json!(["bar-widget"]));
    assert!(manifest.pointer("/barWidget/defaults").is_none());
    assert!(manifest.pointer("/barWidget/schema").is_none());

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
    assert!(qml.contains("openPanelIndicatorWidth"));
    assert!(qml.contains("property bool moduleRefreshPending: false"));
    assert!(qml.contains("function flushModuleRefresh()"));
    assert!(qml.contains("Qt.callLater(root.flushModuleRefresh)"));
    assert!(!qml.contains("command: [root.backendCommand, \"open\"]"));

    let panel_path = qml_path.parent().unwrap().join("Panel.qml");
    assert!(panel_path.is_file(), "missing {}", panel_path.display());
    let panel = fs::read_to_string(panel_path).expect("read native panel");
    assert!(!panel.contains("omarchy-world-clock-bin"));
    assert!(panel.contains("KeyboardPanel"));
    assert!(panel.contains("owner: root.barIdentity"));
    assert!(panel.contains("onTabRequested"));
    assert!(panel.contains("root.switchPanel(direction)"));
    assert!(panel.contains("readonly property int maxClocks: 9"));
    assert!(panel.contains("readonly property bool localTimezoneConfigured"));
    assert!(panel.contains("readonly property int nonLocalLocationCount"));
    assert!(panel.contains("function canAddLocation(timezone)"));
    assert!(panel.contains("root.canAddLocation(resultButton.modelData.timezone)"));
    assert!(panel.contains("Globe {"));
    assert!(panel.contains("snapshot.featured_cities"));
    assert!(panel.contains("function globeLabelLayouts(width, height)"));
    assert!(panel.contains("property var markerLayouts: root.globeLabelLayouts(width, height)"));
    assert!(panel.contains("function globeLocationReveal(wrapper)"));
    assert!(panel.contains("(mapCanvas.zoom - minimumZoom) / 0.24"));
    assert!(panel.contains("layout.reveal * (searchResult || configured"));
    assert!(panel.contains("function mapLocationKey(location)"));
    assert!(!panel.contains("featuredPlaced < 7"));
    assert!(panel.contains("readonly property var mapLocations: searchHasQuery"));
    assert!(panel.contains("model: root.mapLocations"));
    assert!(panel.contains("modelData.searchResult === true"));
    assert!(panel.contains("readonly property bool selectable: !configured"));
    assert_eq!(panel.matches("if (mapMarker.selectable)").count(), 2);
    assert!(panel.contains("hoverEnabled: mapMarker.selectable"));
    assert!(panel.contains("textureEnabled: root.opened && root.mode === \"add\""));
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
    assert!(panel.contains("root.selectMapLocation(mapMarker.location)"));
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
    assert!(panel.contains(
        "onClicked: root.runAction(\"add\", resultButton.modelData.timezone, resultButton.modelData)"
    ));
    assert!(panel.contains("name === \"pin\" || name === \"remove\""));
    assert!(panel.contains("result.label !== null && result.label !== undefined"));
    assert!(panel.contains("id: timelineTickRepeater"));
    assert!(panel.contains("function focusSummaryEditor()"));
    assert!(panel.contains("property bool snapshotLoaded: false"));
    assert!(panel.contains("property bool summaryFocusPending: false"));
    assert!(panel.contains("if (!snapshotLoaded)"));
    assert!(panel.contains("if (summaryFocusPending) Qt.callLater(root.focusSummaryEditor)"));
    assert!(panel.contains("if (clocks.length === 0 && mode !== \"add\" && !summaryFocusPending)"));
    assert!(panel.contains("function focusAddField(selectExisting)"));
    assert!(panel.contains("property bool searchVisible: false"));
    assert!(panel.contains("function openSearch(initialText)"));
    assert!(panel.contains("function closeSearch()"));
    assert!(panel.contains("addField.forceActiveFocus(Qt.ShortcutFocusReason)"));
    assert!(panel.contains("if (mode === \"add\")"));
    assert!(panel.contains("if (root.mode === \"read\") root.focusSummaryEditor()"));
    assert!(panel.contains("Qt.callLater(function()"));
    assert!(panel.contains("property bool editorRefreshPending: false"));
    assert!(panel.contains("property bool snapshotRequestPending: false"));
    assert!(panel.contains("property int snapshotStateGeneration: 0"));
    assert!(panel.contains("property int convertActiveGeneration: -1"));
    assert!(panel.contains("property string invalidConversionSource: \"\""));
    assert!(panel.contains("snapshotLoaded = true\n      invalidConversionSource = \"\""));
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
    assert!(panel.contains("if (mode !== \"add\" || !searchVisible) return"));
    assert!(panel.contains("if (mode !== \"add\" || !searchVisible || !query || !canAdd"));
    assert!(panel.contains("searchDebounce.stop()"));
    assert!(panel.contains("searchResultsQuery === query"));
    assert!(panel.contains("searchSubmitQuery = query"));
    assert!(panel.contains("Qt.callLater(root.startSearch)"));
    assert!(panel.contains("if (root.mode === \"add\""));
    assert!(panel.contains("onTextChanged: root.searchTextChanged()"));
    assert!(panel.contains("id: addBackButton"));
    assert!(panel.contains("id: addSearchButton"));
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
    assert!(panel.contains("diameterRatio: 0.63"));
    assert!(panel.contains("mapCanvas.focusOnLocations(root.searchResults)"));
    assert!(panel.contains("mapCanvas.focusOnLocations([result])"));
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
    assert!(panel.contains("visible: root.mode !== \"add\""));
    assert!(panel.contains("text: root.currentLocationTitle"));
    assert!(panel.contains("text: root.currentTimezoneMetadata"));
    assert!(panel.contains("id: clockRows"));
    assert!(panel.contains("anchors.horizontalCenter: parent.horizontalCenter"));
    assert!(panel.contains("anchors.leftMargin: Style.space(10)"));
    assert!(panel.contains("id: clockSurface"));
    assert!(panel.contains("color: Style.normalFillFor(root.contentForeground, Color.accent)"));
    assert!(panel.contains("radius: Style.cornerRadius"));

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
    assert!(!qml.contains("root.pinnedLabel + \" · \""));

    let map_path = root.join("assets/world-map.png");
    assert!(map_path.is_file(), "missing {}", map_path.display());
    let map = fs::read(&map_path).expect("read globe texture");
    assert!(map.len() >= 24, "globe texture is not a complete PNG");
    assert_eq!(&map[1..4], b"PNG");
    assert_eq!(u32::from_be_bytes(map[16..20].try_into().unwrap()), 8192);
    assert_eq!(u32::from_be_bytes(map[20..24].try_into().unwrap()), 4096);

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
    assert!(key_catcher.contains("event.text.trim() !== \"\""));
    assert!(key_catcher.contains("Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier"));
    assert!(panel.contains("WorldClockKeyCatcher {"));
    assert!(panel.contains("directTextInput: root.mode === \"add\" && !addField.activeFocus"));
    assert!(panel.contains("if (!root.searchVisible) root.openSearch(text)"));

    let globe_path = qml_path.parent().unwrap().join("Globe.qml");
    assert!(globe_path.is_file(), "missing {}", globe_path.display());
    let globe = fs::read_to_string(&globe_path).expect("read globe component");
    assert!(globe.contains("ShaderEffect"));
    assert!(globe.contains("DragHandler"));
    assert!(globe.contains("id: fallbackMap"));
    assert!(globe.contains("TapHandler {\n    parent: root.shaderAvailable ? sphere : fallbackMap"));
    assert!(globe.contains("var rootPoint = parent.mapToItem(root, eventPoint.position)"));
    assert!(globe.contains("WheelHandler"));
    assert!(globe.contains("function project(latitudeDegrees, longitudeDegrees)"));
    assert!(globe.contains("function locationAt(viewX, viewY)"));
    assert!(globe.contains("function normalizedWheelDelta(angleDelta, pixelDelta)"));
    assert!(globe.contains("function focusOnLocations(locations)"));
    assert!(globe.contains("function minimumAngularCenter(vectors)"));
    assert!(globe.contains("minimumDepth: minimumDepth"));
    assert!(globe.contains("event.pixelDelta.y"));
    assert!(globe.contains("property real openingZoom:"));
    assert!(globe.contains("property real maximumZoom: 4.8"));
    assert!(globe.contains("sourceSize.width: 8192"));
    assert!(globe.contains("sourceSize.height: 4096"));
    assert_eq!(
        globe
            .matches(
                "source: root.textureEnabled ? Qt.resolvedUrl(\"../assets/world-map.png\") : \"\""
            )
            .count(),
        2
    );
    assert!(globe.contains("asynchronous: true"));
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
