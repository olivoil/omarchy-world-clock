use crate::config::{
    all_timezones, detect_local_timezone, effective_time_format, ordered_timezones, AppConfig,
    ConfigManager, RemotePlaceSearch, TimezoneEntry, TimezoneResolver, TimezoneSearchResult,
    DEFAULT_SORT_MODE, DEFAULT_TIME_FORMAT,
};
use crate::layout::{
    load_window_border_size, load_window_gap, popup_top_margin, POPUP_TOP_CONTENT_MARGIN,
};
use crate::theme::{build_css, load_palette};
use crate::time::{
    format_display_time, format_timezone_notation, friendly_timezone_name,
    parse_manual_reference_details, row_metadata, zoned_datetime,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Offset, Timelike, Utc};
use gtk::gdk;
use gtk::glib::{self, ControlFlow, MainLoop, Propagation};
use gtk::prelude::*;
use gtk::{Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tzf_rs::Finder as TimezoneFinder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PopupScreen {
    Read,
    Edit,
    Add,
}

#[derive(Clone)]
struct RowWidgets {
    entry: TimezoneEntry,
    root: gtk::Box,
    drag_handle: gtk::Box,
    title: gtk::Label,
    context: gtk::Label,
    meta: gtk::Label,
    lock_button: gtk::Button,
    remove_button: gtk::Button,
    time_entry: gtk::Entry,
    dirty: Rc<Cell<bool>>,
    suppress_changes: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct ReadCardWidgets {
    entry: TimezoneEntry,
    root: gtk::Overlay,
    title: gtk::Label,
    time_entry: gtk::Entry,
    timezone_label: gtk::Label,
    delta_label: gtk::Label,
    controls: gtk::Box,
    remove_button: gtk::Button,
    dirty: Rc<Cell<bool>>,
    suppress_changes: Rc<Cell<bool>>,
}

struct PopupState {
    config_manager: ConfigManager,
    config: AppConfig,
    resolver: TimezoneResolver,
    place_search: Arc<Mutex<RemotePlaceSearch>>,
    remote_search_sender: mpsc::Sender<RemoteSearchMessage>,
    local_timezone: String,
    time_format: String,
    reference_utc: DateTime<Utc>,
    rows_overlay: gtk::Overlay,
    rows_box: gtk::Box,
    row_separators: Vec<gtk::Separator>,
    drag_layer: gtk::Fixed,
    insertion_marker: gtk::Box,
    rows: Vec<RowWidgets>,
    dismiss_armed: bool,
    allow_close: bool,
    live: bool,
    screen_mode: PopupScreen,
    editing_timezone: Option<String>,
    pending_apply_source: Option<glib::SourceId>,
    pending_apply_timezone: Option<String>,
    content_stack: gtk::Stack,
    panel_title: gtk::Label,
    live_button: gtk::Button,
    edit_button: gtk::Button,
    add_button: gtk::Button,
    cancel_button: gtk::Button,
    sort_mode_dropdown: gtk::DropDown,
    time_format_dropdown: gtk::DropDown,
    read_summary_time: gtk::Entry,
    read_summary_location: gtk::Label,
    read_summary_dirty: Rc<Cell<bool>>,
    read_summary_suppress_changes: Rc<Cell<bool>>,
    timeline_area: gtk::DrawingArea,
    timeline_labels: gtk::Fixed,
    cards_grid: gtk::Box,
    read_cards: Vec<ReadCardWidgets>,
    add_entry: gtk::Entry,
    search_results_scroller: gtk::ScrolledWindow,
    search_results_box: gtk::Box,
    add_map_area: gtk::DrawingArea,
    map_timezone_finder: TimezoneFinder,
    add_map_hover_layer: gtk::Fixed,
    add_map_hover_card: gtk::Box,
    add_map_hover_title: gtk::Label,
    add_map_hover_time: gtk::Label,
    add_map_hover_meta: gtk::Label,
    add_map_hover_relative: gtk::Label,
    hovered_map_result: Option<TimezoneSearchResult>,
    local_search_results: Vec<TimezoneSearchResult>,
    remote_search_results: Vec<TimezoneSearchResult>,
    search_results: Vec<TimezoneSearchResult>,
    search_generation: u64,
    drag_source_timezone: Option<String>,
    active_drop_index: Option<usize>,
    drag_start_rows_box_y: f64,
    drag_start_row_top_y: f64,
    drag_row_overlay_x: f64,
    drag_ghost: Option<gtk::Widget>,
    status_label: gtk::Label,
    self_handle: Weak<RefCell<PopupState>>,
}

struct PidGuard {
    path: PathBuf,
}

struct RemoteSearchMessage {
    generation: u64,
    query: String,
    results: Vec<TimezoneSearchResult>,
}

const READ_PANEL_TARGET_HEIGHT: i32 = 540;
const READ_PANEL_WIDTH: i32 = (READ_PANEL_TARGET_HEIGHT * 16) / 9;
const READ_TIMELINE_WIDTH: i32 = READ_PANEL_WIDTH - 60;
const READ_SECTION_SPACING: i32 = 18;
const READ_TIMELINE_TOP_MARGIN: i32 = 12;
const READ_TIMELINE_HEIGHT: i32 = 128;
const TIMELINE_LINE_Y: f64 = 64.0;
const TIMELINE_PADDING: f64 = 28.0;
const TIMELINE_LABEL_WIDTH: f64 = 76.0;
const TIMELINE_LABEL_HEIGHT: i32 = 42;
const TIMELINE_LABEL_LANE_Y: [f64; 2] = [4.0, 82.0];
const TIMELINE_LABEL_LANE_GAP: f64 = 8.0;
const TIMELINE_MIN_SIDE_HOURS: i64 = 12;
const TIMELINE_EDGE_HOUR_MARGIN: i64 = 1;
const READ_CARD_COLUMNS: i32 = 3;
const READ_CARD_LIMIT: usize = 9;
const READ_CARD_SPACING: i32 = 18;
const READ_CARD_WIDTH: i32 =
    (READ_TIMELINE_WIDTH - (READ_CARD_SPACING * (READ_CARD_COLUMNS - 1))) / READ_CARD_COLUMNS;
const ADD_SEARCH_RESULT_LIMIT: usize = 8;
const ADD_MAP_HEIGHT: i32 = READ_TIMELINE_WIDTH / 2;
const ADD_MAP_ASPECT_RATIO: f32 = 2.0;
const ADD_MAP_HOVER_CARD_WIDTH: i32 = 272;
const ADD_MAP_HOVER_CARD_HEIGHT: i32 = 140;
const WORLD_MAP_ASSET_BYTES: &[u8] = include_bytes!("../assets/world-map.png");
const SORT_MODE_VALUES: [&str; 3] = ["manual", "alpha", "time"];
const TIME_FORMAT_VALUES: [&str; 3] = ["system", "24h", "ampm"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineItem {
    relative_minutes: i64,
    time_text: String,
    zone_text: String,
    zone_tooltip: Option<String>,
    entry_count: usize,
}

struct TimelineGroupBuilder {
    time_text: String,
    anchor_labels: Vec<String>,
    other_labels: Vec<String>,
    entry_count: usize,
}

const NORTH_AMERICA_POINTS: &[(f64, f64)] = &[
    (0.03, 0.36),
    (0.06, 0.24),
    (0.12, 0.18),
    (0.19, 0.16),
    (0.25, 0.18),
    (0.29, 0.21),
    (0.33, 0.25),
    (0.35, 0.33),
    (0.31, 0.39),
    (0.27, 0.42),
    (0.20, 0.46),
    (0.16, 0.53),
    (0.10, 0.56),
    (0.06, 0.52),
    (0.03, 0.44),
];
const SOUTH_AMERICA_POINTS: &[(f64, f64)] = &[
    (0.28, 0.51),
    (0.31, 0.57),
    (0.34, 0.65),
    (0.35, 0.74),
    (0.33, 0.84),
    (0.30, 0.92),
    (0.26, 0.86),
    (0.24, 0.74),
    (0.25, 0.60),
];
const EUROPE_AFRICA_POINTS: &[(f64, f64)] = &[
    (0.46, 0.26),
    (0.50, 0.18),
    (0.56, 0.16),
    (0.61, 0.20),
    (0.63, 0.27),
    (0.61, 0.34),
    (0.59, 0.39),
    (0.58, 0.46),
    (0.60, 0.57),
    (0.58, 0.71),
    (0.55, 0.85),
    (0.49, 0.79),
    (0.46, 0.64),
    (0.45, 0.48),
    (0.43, 0.35),
];
const ASIA_POINTS: &[(f64, f64)] = &[
    (0.58, 0.22),
    (0.65, 0.17),
    (0.74, 0.16),
    (0.82, 0.20),
    (0.89, 0.26),
    (0.92, 0.35),
    (0.90, 0.44),
    (0.85, 0.48),
    (0.82, 0.58),
    (0.78, 0.60),
    (0.73, 0.56),
    (0.68, 0.58),
    (0.64, 0.52),
    (0.61, 0.48),
    (0.58, 0.40),
    (0.56, 0.30),
];
const AUSTRALIA_POINTS: &[(f64, f64)] = &[
    (0.83, 0.69),
    (0.87, 0.65),
    (0.92, 0.66),
    (0.95, 0.72),
    (0.94, 0.80),
    (0.88, 0.83),
    (0.83, 0.79),
    (0.81, 0.73),
];
const GREENLAND_POINTS: &[(f64, f64)] = &[
    (0.22, 0.05),
    (0.27, 0.03),
    (0.31, 0.06),
    (0.30, 0.15),
    (0.25, 0.17),
    (0.21, 0.12),
];
const WORLD_LANDMASSES: [&[(f64, f64)]; 6] = [
    NORTH_AMERICA_POINTS,
    SOUTH_AMERICA_POINTS,
    EUROPE_AFRICA_POINTS,
    ASIA_POINTS,
    AUSTRALIA_POINTS,
    GREENLAND_POINTS,
];
const MAP_LEGEND_LABELS: [&str; 7] = ["-12", "-8", "-4", "+0", "+4", "+8", "+12"];

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn debug_popup_event(message: &str) {
    if std::env::var_os("OMARCHY_WORLD_CLOCK_DEBUG").is_none() {
        return;
    }

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/owc-popup-debug.log")
    {
        let _ = writeln!(file, "{message}");
    }
}

fn request_window_close(
    state_handle: &Rc<RefCell<PopupState>>,
    window: &gtk::Window,
    reason: &str,
) {
    debug_popup_event(&format!("request_window_close reason={reason}"));
    state_handle.borrow_mut().allow_close = true;
    window.close();
}

fn box_children(container: &gtk::Box) -> Vec<gtk::Widget> {
    let mut children = Vec::new();
    let mut current = container.first_child();
    while let Some(child) = current {
        current = child.next_sibling();
        children.push(child);
    }
    children
}

fn cancel_pending_apply(state: &mut PopupState) {
    if let Some(source_id) = state.pending_apply_source.take() {
        source_id.remove();
    }
    state.pending_apply_timezone = None;
}

fn apply_css() -> Result<()> {
    let display = gdk::Display::default().context("GTK display is unavailable")?;
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&build_css(&load_palette()));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Ok(())
}

fn target_monitor(display: &gdk::Display) -> Option<gdk::Monitor> {
    if let Some(pointer) = display.default_seat().and_then(|seat| seat.pointer()) {
        let (surface, _, _) = pointer.surface_at_position();
        if let Some(surface) = surface {
            if let Some(monitor) = display.monitor_at_surface(&surface) {
                return Some(monitor);
            }
        }
    }

    display
        .monitors()
        .item(0)
        .and_then(|object| object.downcast::<gdk::Monitor>().ok())
}

fn time_entry_placeholder(time_format: &str) -> &'static str {
    if time_format == "ampm" {
        "h:mm AM"
    } else {
        "HH:MM"
    }
}

fn time_entry_width_chars(time_format: &str) -> i32 {
    if time_format == "ampm" {
        11
    } else {
        8
    }
}

fn selected_entries(state: &PopupState) -> Vec<TimezoneEntry> {
    ordered_timezones(
        &state.config.timezones,
        &state.config.sort_mode,
        state.reference_utc,
    )
}

fn unlocked_entry_count(state: &PopupState) -> usize {
    state
        .config
        .timezones
        .iter()
        .filter(|entry| !entry.locked)
        .count()
}

fn sort_mode_index(sort_mode: &str) -> u32 {
    SORT_MODE_VALUES
        .iter()
        .position(|value| *value == sort_mode)
        .unwrap_or(0) as u32
}

fn time_format_index(time_format: &str) -> u32 {
    TIME_FORMAT_VALUES
        .iter()
        .position(|value| *value == time_format)
        .unwrap_or(0) as u32
}

fn sync_dropdowns(state: &PopupState) {
    state
        .sort_mode_dropdown
        .set_selected(sort_mode_index(&state.config.sort_mode));
    state
        .time_format_dropdown
        .set_selected(time_format_index(&state.config.time_format));
}

fn read_entry_count(entries: &[TimezoneEntry], local_timezone: &str) -> usize {
    entries
        .iter()
        .filter(|entry| entry.timezone != local_timezone)
        .count()
}

fn visible_read_entries(entries: &[TimezoneEntry], local_timezone: &str) -> Vec<TimezoneEntry> {
    entries
        .iter()
        .filter(|entry| entry.timezone != local_timezone)
        .take(READ_CARD_LIMIT)
        .cloned()
        .collect()
}

fn sort_read_entries_by_time(
    entries: &mut [TimezoneEntry],
    reference_utc: DateTime<Utc>,
    local_timezone: &str,
) {
    let anchor = zoned_datetime(reference_utc, local_timezone);
    entries.sort_by(|left, right| {
        let left_zoned = zoned_datetime(reference_utc, &left.timezone);
        let right_zoned = zoned_datetime(reference_utc, &right.timezone);
        timeline_relative_minutes(&anchor, &left_zoned)
            .cmp(&timeline_relative_minutes(&anchor, &right_zoned))
            .then_with(|| left.display_label().cmp(&right.display_label()))
    });
}

fn read_entries(state: &PopupState) -> Vec<TimezoneEntry> {
    let mut entries = visible_read_entries(&state.config.timezones, &state.local_timezone);
    sort_read_entries_by_time(&mut entries, state.reference_utc, &state.local_timezone);
    entries
}

fn read_card_row_width(entry_count: usize) -> i32 {
    let columns = (entry_count as i32).clamp(1, READ_CARD_COLUMNS);
    (READ_CARD_WIDTH * columns) + (READ_CARD_SPACING * (columns - 1))
}

fn timeline_entries(
    entries: &[TimezoneEntry],
    local_timezone: &str,
    reference_utc: DateTime<Utc>,
) -> Vec<TimezoneEntry> {
    let mut visible = visible_read_entries(entries, local_timezone);
    if !visible.iter().any(|entry| entry.timezone == local_timezone) {
        let local_entry = entries
            .iter()
            .find(|entry| entry.timezone == local_timezone)
            .cloned()
            .unwrap_or(TimezoneEntry {
                timezone: local_timezone.to_string(),
                label: String::new(),
                locked: false,
            });
        visible.push(local_entry);
    }
    sort_read_entries_by_time(&mut visible, reference_utc, local_timezone);
    visible
}

fn row_can_reorder(state: &PopupState, _entry: &TimezoneEntry) -> bool {
    state.config.sort_mode == "manual" && !_entry.locked && unlocked_entry_count(state) > 1
}

fn set_entry_error(entry: &gtk::Entry, enabled: bool) {
    if enabled {
        entry.add_css_class("error");
    } else {
        entry.remove_css_class("error");
    }
}

fn set_row_error(row: &RowWidgets, enabled: bool) {
    set_entry_error(&row.time_entry, enabled);
}

fn set_read_card_controls(state: &PopupState) {
    let can_remove = state.config.timezones.len() > 1;
    let show_card_controls = can_remove && matches!(state.screen_mode, PopupScreen::Edit);
    for card in &state.read_cards {
        card.controls.set_visible(show_card_controls);
        card.controls
            .set_opacity(if show_card_controls { 1.0 } else { 0.0 });
        card.controls.set_sensitive(show_card_controls);
        card.remove_button.set_sensitive(show_card_controls);
    }
}

fn set_status(state: &PopupState, message: &str, error: bool) {
    if message.is_empty() {
        state.status_label.set_text("");
        state.status_label.set_visible(false);
        state.status_label.remove_css_class("error");
        return;
    }

    state.status_label.set_text(message);
    state.status_label.set_visible(true);
    if error {
        state.status_label.add_css_class("error");
    } else {
        state.status_label.remove_css_class("error");
    }
}

fn clear_status(state: &PopupState) {
    set_status(state, "", false);
}

fn update_time_entry_focus_state(
    state_handle: &Rc<RefCell<PopupState>>,
    focus_controller: &gtk::EventControllerFocus,
    timezone_name: String,
) {
    match state_handle.try_borrow_mut() {
        Ok(mut state) => {
            state.editing_timezone = Some(timezone_name);
            clear_status(&state);
        }
        Err(_) => {
            debug_popup_event("time_entry_focus_enter deferred busy_state");
            let state_for_idle = state_handle.clone();
            let focus_for_idle = focus_controller.clone();
            glib::idle_add_local_once(move || {
                if !focus_for_idle.is_focus() {
                    debug_popup_event("time_entry_focus_enter skipped stale_focus");
                    return;
                }

                let Ok(mut state) = state_for_idle.try_borrow_mut() else {
                    debug_popup_event("time_entry_focus_enter skipped busy_state");
                    return;
                };
                state.editing_timezone = Some(timezone_name);
                clear_status(&state);
            });
        }
    }
}

fn refresh_time_entry_focus_leave(state_handle: &Rc<RefCell<PopupState>>) {
    match state_handle.try_borrow_mut() {
        Ok(mut state) => update_row_widgets(&mut state),
        Err(_) => {
            debug_popup_event("time_entry_focus_leave_refresh deferred busy_state");
            let state_for_idle = state_handle.clone();
            glib::idle_add_local_once(move || {
                let Ok(mut state) = state_for_idle.try_borrow_mut() else {
                    debug_popup_event("time_entry_focus_leave_refresh skipped busy_state");
                    return;
                };
                update_row_widgets(&mut state);
            });
        }
    }
}

fn defer_time_entry_focus_leave<F>(callback: F)
where
    F: FnOnce() + 'static,
{
    let mut callback = Some(callback);
    // Flush the old focused entry before later key/change events mark a new entry dirty.
    glib::idle_add_local_full(glib::Priority::HIGH, move || {
        if let Some(callback) = callback.take() {
            callback();
        }
        ControlFlow::Break
    });
}

fn clear_time_entry_focus_state(
    state_handle: &Rc<RefCell<PopupState>>,
    focus_controller: &gtk::EventControllerFocus,
    timezone_name: String,
    dirty: Rc<Cell<bool>>,
) {
    let Ok(mut state) = state_handle.try_borrow_mut() else {
        debug_popup_event("time_entry_focus_leave deferred busy_state");
        let state_for_idle = state_handle.clone();
        let focus_for_idle = focus_controller.clone();
        defer_time_entry_focus_leave(move || {
            if focus_for_idle.is_focus() {
                debug_popup_event("time_entry_focus_leave skipped stale_focus");
                return;
            }

            clear_time_entry_focus_state(&state_for_idle, &focus_for_idle, timezone_name, dirty);
        });
        return;
    };

    if state.editing_timezone.as_deref() == Some(timezone_name.as_str()) {
        state.editing_timezone = None;
    }
    drop(state);

    if dirty.get() {
        let applied = flush_live_apply(state_handle, &timezone_name, false);
        if !applied {
            dirty.set(false);
            refresh_time_entry_focus_leave(state_handle);
        }
    } else {
        refresh_time_entry_focus_leave(state_handle);
    }
}

fn schedule_live_apply(state_handle: &Rc<RefCell<PopupState>>, timezone_name: &str) {
    let mut state = state_handle.borrow_mut();
    if let Some(source_id) = state.pending_apply_source.take() {
        source_id.remove();
    }
    state.pending_apply_timezone = Some(timezone_name.to_string());

    let state_for_timeout = state_handle.clone();
    let timezone_name = timezone_name.to_string();
    let source_id = glib::timeout_add_local(Duration::from_millis(120), move || {
        {
            let mut state = state_for_timeout.borrow_mut();
            state.pending_apply_source = None;
            state.pending_apply_timezone = None;
        }
        let _ = apply_manual_entry(&state_for_timeout, &timezone_name, false);
        ControlFlow::Break
    });
    state.pending_apply_source = Some(source_id);
}

fn flush_live_apply(
    state_handle: &Rc<RefCell<PopupState>>,
    timezone_name: &str,
    show_errors: bool,
) -> bool {
    {
        let mut state = state_handle.borrow_mut();
        if state.pending_apply_timezone.as_deref() == Some(timezone_name) {
            if let Some(source_id) = state.pending_apply_source.take() {
                source_id.remove();
            }
            state.pending_apply_timezone = None;
        }
    }
    apply_manual_entry(state_handle, timezone_name, show_errors)
}

fn update_live_button(state: &PopupState) {
    if state.live {
        state.live_button.set_sensitive(false);
        state.live_button.set_tooltip_text(Some("Clocks are live."));
        state.live_button.remove_css_class("active");
    } else {
        state.live_button.set_sensitive(true);
        state
            .live_button
            .set_tooltip_text(Some("Return to the current time."));
        state.live_button.add_css_class("active");
    }
}

fn anchor_label(state: &PopupState) -> String {
    state
        .config
        .timezones
        .iter()
        .find(|entry| entry.timezone == state.local_timezone)
        .map(TimezoneEntry::display_label)
        .unwrap_or_else(|| friendly_timezone_name(&state.local_timezone))
}

fn first_location_segment(label: &str) -> String {
    let trimmed = label.trim();
    label
        .split(',')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

fn trailing_location_segments(label: &str) -> Option<String> {
    let parts = label
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        None
    } else {
        Some(parts[1..].join(", "))
    }
}

fn summary_search_result(
    timezone_name: &str,
    label: &str,
    resolver_result: Option<TimezoneSearchResult>,
) -> TimezoneSearchResult {
    let mut result = resolver_result.unwrap_or_else(|| TimezoneSearchResult {
        timezone: timezone_name.to_string(),
        title: label.to_string(),
        subtitle: timezone_name.to_string(),
    });

    if let Some(location_context) = trailing_location_segments(label) {
        result.subtitle = format!("{}  ·  {}", result.timezone, location_context);
    } else if result.subtitle.trim().is_empty() {
        result.subtitle = result.timezone.clone();
    }

    result
}

fn read_summary_title(state: &PopupState) -> String {
    first_location_segment(&anchor_label(state))
}

fn read_summary_subtitle(state: &PopupState) -> String {
    let label = anchor_label(state);
    let result = summary_search_result(
        &state.local_timezone,
        &label,
        state.resolver.describe_timezone(&state.local_timezone),
    );
    search_result_subtitle(&result, &state.reference_utc)
}

fn relative_time_label(
    anchor: &DateTime<chrono_tz::Tz>,
    value: &DateTime<chrono_tz::Tz>,
) -> String {
    let difference_minutes =
        (value.offset().fix().local_minus_utc() - anchor.offset().fix().local_minus_utc()) / 60;
    if difference_minutes == 0 {
        return "Same time".to_string();
    }

    let direction = if difference_minutes > 0 {
        "ahead"
    } else {
        "behind"
    };
    let absolute_minutes = difference_minutes.abs();
    let hours = absolute_minutes / 60;
    let minutes = absolute_minutes % 60;

    if minutes == 0 {
        if hours == 1 {
            format!("1 hour {direction}")
        } else {
            format!("{hours} hours {direction}")
        }
    } else if hours == 0 {
        format!("{minutes} min {direction}")
    } else {
        format!("{hours}h {minutes:02}m {direction}")
    }
}

fn timeline_relative_minutes(
    anchor: &DateTime<chrono_tz::Tz>,
    value: &DateTime<chrono_tz::Tz>,
) -> i64 {
    value
        .naive_local()
        .signed_duration_since(anchor.naive_local())
        .num_minutes()
}

fn push_timeline_label(labels: &mut Vec<String>, label: &str) {
    if labels.iter().all(|existing| existing != label) {
        labels.push(label.to_string());
    }
}

fn format_timeline_zone_text(labels: &[String], entry_count: usize) -> String {
    let Some(first_label) = labels.first() else {
        return String::new();
    };

    if entry_count <= 1 {
        return first_label.clone();
    }

    if labels.len() == 1 {
        return format!("{first_label} +{}", entry_count - 1);
    }

    if entry_count == 2 {
        return format!("{} / {}", labels[0], labels[1]);
    }

    format!("{} / {} +{}", labels[0], labels[1], entry_count - 2)
}

fn timeline_zone_tooltip(labels: &[String], entry_count: usize) -> Option<String> {
    let joined = labels.join(" / ");
    if entry_count > labels.len() {
        let zone_word = if entry_count == 1 { "zone" } else { "zones" };
        if joined.is_empty() {
            Some(format!("{entry_count} {zone_word}"))
        } else {
            Some(format!("{joined} ({entry_count} {zone_word})"))
        }
    } else if labels.len() > 1 {
        Some(joined)
    } else {
        None
    }
}

fn build_timeline_items(
    entries: &[TimezoneEntry],
    local_timezone: &str,
    reference_utc: DateTime<Utc>,
    time_format: &str,
) -> Vec<TimelineItem> {
    let anchor = zoned_datetime(reference_utc, local_timezone);
    let mut groups = BTreeMap::<i64, TimelineGroupBuilder>::new();

    for entry in timeline_entries(entries, local_timezone, reference_utc) {
        let zoned = zoned_datetime(reference_utc, &entry.timezone);
        let relative_minutes = timeline_relative_minutes(&anchor, &zoned);
        let abbreviation = format_timezone_notation(&zoned);
        let group = groups
            .entry(relative_minutes)
            .or_insert_with(|| TimelineGroupBuilder {
                time_text: format_display_time(&zoned, time_format),
                anchor_labels: Vec::new(),
                other_labels: Vec::new(),
                entry_count: 0,
            });
        group.entry_count += 1;
        if entry.timezone == local_timezone {
            push_timeline_label(&mut group.anchor_labels, &abbreviation);
        } else {
            push_timeline_label(&mut group.other_labels, &abbreviation);
        }
    }

    groups
        .into_iter()
        .map(|(relative_minutes, mut group)| {
            group.other_labels.sort();
            let mut labels = group.anchor_labels;
            for label in group.other_labels {
                push_timeline_label(&mut labels, &label);
            }
            let zone_text = format_timeline_zone_text(&labels, group.entry_count);
            TimelineItem {
                relative_minutes,
                time_text: group.time_text,
                zone_tooltip: timeline_zone_tooltip(&labels, group.entry_count),
                zone_text,
                entry_count: group.entry_count,
            }
        })
        .collect()
}

fn timeline_extent_minutes(items: &[TimelineItem]) -> i64 {
    items
        .iter()
        .map(|item| item.relative_minutes.abs())
        .max()
        .unwrap_or(60)
        .max(60)
}

fn timeline_side_hours(items: &[TimelineItem]) -> i64 {
    ((timeline_extent_minutes(items) + 59) / 60 + TIMELINE_EDGE_HOUR_MARGIN)
        .max(TIMELINE_MIN_SIDE_HOURS)
}

fn timeline_anchor_minute_offset(anchor: &DateTime<chrono_tz::Tz>) -> f64 {
    f64::from(anchor.minute())
        + f64::from(anchor.second()) / 60.0
        + f64::from(anchor.nanosecond()) / 60_000_000_000.0
}

fn timeline_position_x(relative_minutes: f64, side_hours: i64, width: f64) -> f64 {
    let usable_width = (width - TIMELINE_PADDING * 2.0).max(1.0);
    let side_span_minutes = (side_hours * 60) as f64;
    TIMELINE_PADDING
        + (((relative_minutes + side_span_minutes) / (side_span_minutes * 2.0)) * usable_width)
}

fn timeline_tick_relative_minutes(anchor: &DateTime<chrono_tz::Tz>, side_hours: i64) -> Vec<f64> {
    let side_span_minutes = (side_hours * 60) as f64;
    let anchor_minute_offset = timeline_anchor_minute_offset(anchor);
    let first_tick_hour = ((-side_span_minutes + anchor_minute_offset) / 60.0).ceil() as i64;
    let last_tick_hour = ((side_span_minutes + anchor_minute_offset) / 60.0).floor() as i64;

    (first_tick_hour..=last_tick_hour)
        .map(|hour_offset| (hour_offset as f64 * 60.0) - anchor_minute_offset)
        .collect()
}

fn color_components(hex_value: &str, fallback: (f64, f64, f64)) -> (f64, f64, f64) {
    gdk::RGBA::parse(hex_value)
        .ok()
        .map(|rgba| {
            (
                f64::from(rgba.red()),
                f64::from(rgba.green()),
                f64::from(rgba.blue()),
            )
        })
        .unwrap_or(fallback)
}

fn load_world_map_texture() -> Option<gdk::Texture> {
    match gdk::Texture::from_bytes(&glib::Bytes::from_static(WORLD_MAP_ASSET_BYTES)) {
        Ok(texture) => Some(texture),
        Err(error) => {
            debug_popup_event(&format!("world_map_texture_load_failed error={error}"));
            None
        }
    }
}

fn draw_polygon(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    points: &[(f64, f64)],
    fill_color: (f64, f64, f64, f64),
    stroke_color: (f64, f64, f64, f64),
) {
    let Some((start_x, start_y)) = points.first().copied() else {
        return;
    };

    context.new_path();
    context.move_to(start_x * width, start_y * height);
    for (x, y) in points.iter().copied().skip(1) {
        context.line_to(x * width, y * height);
    }
    context.close_path();
    context.set_source_rgba(fill_color.0, fill_color.1, fill_color.2, fill_color.3);
    let _ = context.fill_preserve();
    context.set_source_rgba(
        stroke_color.0,
        stroke_color.1,
        stroke_color.2,
        stroke_color.3,
    );
    context.set_line_width(1.0);
    let _ = context.stroke();
}

fn draw_add_map_fallback(context: &gtk::cairo::Context, width: f64, height: f64) {
    let palette = load_palette();
    let background = color_components(&palette.background, (0.04, 0.09, 0.18));
    let foreground = color_components(&palette.foreground, (0.85, 0.88, 0.94));

    context.set_source_rgba(background.0, background.1, background.2, 0.12);
    context.rectangle(0.0, 0.0, width, height);
    let _ = context.fill();

    for points in WORLD_LANDMASSES {
        draw_polygon(
            context,
            width,
            height,
            points,
            (foreground.0, foreground.1, foreground.2, 0.10),
            (foreground.0, foreground.1, foreground.2, 0.16),
        );
    }
}

fn draw_add_map_overlay(context: &gtk::cairo::Context, width: f64, height: f64) {
    let palette = load_palette();
    let foreground = color_components(&palette.foreground, (0.85, 0.88, 0.94));

    context.set_source_rgba(foreground.0, foreground.1, foreground.2, 0.10);
    context.set_line_width(1.0);
    for index in 1..12 {
        let x = width * (index as f64 / 12.0);
        context.move_to(x, 10.0);
        context.line_to(x, height - 10.0);
        let _ = context.stroke();
    }
}

fn configure_manual_time_entry(entry: &gtk::Entry, time_format: &str) {
    entry.set_width_chars(time_entry_width_chars(time_format));
    entry.set_max_length(19);
    entry.set_placeholder_text(Some(time_entry_placeholder(time_format)));
}

fn set_time_entry_text(entry: &gtk::Entry, suppress_changes: &Rc<Cell<bool>>, text: &str) {
    suppress_changes.set(true);
    entry.set_text(text);
    suppress_changes.set(false);
}

fn build_read_card(entry: &TimezoneEntry, time_format: &str) -> ReadCardWidgets {
    let card_shell = gtk::Overlay::new();
    card_shell.set_overflow(gtk::Overflow::Visible);
    card_shell.set_size_request(READ_CARD_WIDTH, -1);
    card_shell.add_css_class("timezone-card-shell");

    let card = gtk::Box::new(Orientation::Vertical, 16);
    card.add_css_class("timezone-card");
    card.set_margin_top(18);
    card.set_size_request(READ_CARD_WIDTH, -1);
    card_shell.set_child(Some(&card));

    let header = gtk::Box::new(Orientation::Horizontal, 12);
    header.set_halign(Align::Fill);
    card.append(&header);

    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_wrap(false);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    title.add_css_class("timezone-card-title");
    header.append(&title);

    let timezone_label = gtk::Label::new(None);
    timezone_label.set_xalign(1.0);
    timezone_label.set_halign(Align::End);
    timezone_label.set_wrap(false);
    timezone_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    timezone_label.set_single_line_mode(true);
    timezone_label.add_css_class("timezone-card-meta");
    header.append(&timezone_label);

    let time_entry = gtk::Entry::new();
    gtk::prelude::EditableExt::set_alignment(&time_entry, 0.0);
    configure_manual_time_entry(&time_entry, time_format);
    time_entry.set_halign(Align::Start);
    time_entry.add_css_class("timezone-card-time");
    time_entry.set_tooltip_text(Some("Enter a time in this timezone."));
    card.append(&time_entry);

    let footer = gtk::Box::new(Orientation::Horizontal, 0);
    footer.set_halign(Align::Fill);

    let delta_label = gtk::Label::new(None);
    delta_label.set_xalign(0.0);
    delta_label.set_hexpand(true);
    delta_label.set_halign(Align::Start);
    delta_label.set_wrap(false);
    delta_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    delta_label.set_single_line_mode(true);
    delta_label.add_css_class("timezone-card-meta");
    footer.append(&delta_label);

    card.append(&footer);

    let controls = gtk::Box::new(Orientation::Horizontal, 0);
    controls.set_halign(Align::Start);
    controls.set_valign(Align::Start);
    controls.set_size_request(36, 36);
    controls.set_overflow(gtk::Overflow::Visible);
    card_shell.connect_get_child_position(|overlay, _| {
        Some(gdk::Rectangle::new(
            (overlay.allocated_width() - 18).max(0),
            0,
            36,
            36,
        ))
    });
    card_shell.add_overlay(&controls);
    card_shell.set_measure_overlay(&controls, false);
    card_shell.set_clip_overlay(&controls, false);

    let remove_button = gtk::Button::from_icon_name("edit-delete-symbolic");
    remove_button.add_css_class("icon-button");
    remove_button.add_css_class("card-control-button");
    remove_button.add_css_class("card-hover-delete");
    remove_button.add_css_class("destructive");
    remove_button.set_size_request(36, 36);
    remove_button.set_tooltip_text(Some("Remove timezone."));
    controls.append(&remove_button);

    ReadCardWidgets {
        entry: entry.clone(),
        root: card_shell,
        title,
        time_entry,
        timezone_label,
        delta_label,
        controls,
        remove_button,
        dirty: Rc::new(Cell::new(false)),
        suppress_changes: Rc::new(Cell::new(false)),
    }
}

fn rebuild_read_cards(state: &mut PopupState, entries: &[TimezoneEntry]) {
    clear_box(&state.cards_grid);
    state.read_cards.clear();

    let state_handle = state.self_handle.upgrade();
    let columns = READ_CARD_COLUMNS as usize;
    let mut row: Option<gtk::Box> = None;

    for (index, entry) in entries.iter().enumerate() {
        if index % columns == 0 {
            let row_entry_count = entries.len().saturating_sub(index).min(columns);
            let next_row = gtk::Box::new(Orientation::Horizontal, READ_CARD_SPACING);
            next_row.set_halign(Align::Center);
            next_row.set_width_request(read_card_row_width(row_entry_count));
            state.cards_grid.append(&next_row);
            row = Some(next_row);
        }

        let widgets = build_read_card(entry, &state.time_format);
        if let Some(state_handle) = state_handle.as_ref() {
            bind_time_entry_events(
                state_handle,
                &widgets.time_entry,
                entry.timezone.clone(),
                widgets.dirty.clone(),
                widgets.suppress_changes.clone(),
            );

            let state_for_remove = state_handle.clone();
            let timezone_name_for_remove = entry.timezone.clone();
            widgets.remove_button.connect_clicked(move |_| {
                remove_timezone_entry(&state_for_remove, &timezone_name_for_remove);
            });
        }

        if let Some(row) = row.as_ref() {
            row.append(&widgets.root);
        }
        state.read_cards.push(widgets);
    }
}

fn update_read_cards(
    state: &mut PopupState,
    entries: &[TimezoneEntry],
    anchor: &DateTime<chrono_tz::Tz>,
) {
    let current_order = state
        .read_cards
        .iter()
        .map(|card| card.entry.timezone.clone())
        .collect::<Vec<_>>();
    let desired_order = entries
        .iter()
        .map(|entry| entry.timezone.clone())
        .collect::<Vec<_>>();

    if current_order != desired_order && state.editing_timezone.is_none() {
        rebuild_read_cards(state, entries);
    }

    let update_entries = if current_order == desired_order {
        entries.to_vec()
    } else {
        state
            .read_cards
            .iter()
            .map(|card| card.entry.clone())
            .collect::<Vec<_>>()
    };

    let reference_utc = state.reference_utc;
    let time_format = state.time_format.clone();
    let editing_timezone = state.editing_timezone.clone();

    for (card, entry) in state.read_cards.iter_mut().zip(update_entries.iter()) {
        card.entry = entry.clone();
        let zoned = zoned_datetime(reference_utc, &entry.timezone);
        card.title.set_text(&read_card_title(entry));
        card.timezone_label
            .set_text(&format_timezone_notation(&zoned));
        card.delta_label
            .set_text(&relative_time_label(anchor, &zoned));
        configure_manual_time_entry(&card.time_entry, &time_format);

        if editing_timezone.as_deref() == Some(entry.timezone.as_str()) {
            continue;
        }

        set_entry_error(&card.time_entry, false);
        set_time_entry_text(
            &card.time_entry,
            &card.suppress_changes,
            &format_display_time(&zoned, &time_format),
        );
        card.dirty.set(false);
    }

    set_read_card_controls(state);
}

fn render_read_view(state: &mut PopupState) {
    let anchor = zoned_datetime(state.reference_utc, &state.local_timezone);
    configure_manual_time_entry(&state.read_summary_time, &state.time_format);
    if state.editing_timezone.as_deref() != Some(state.local_timezone.as_str()) {
        set_entry_error(&state.read_summary_time, false);
        set_time_entry_text(
            &state.read_summary_time,
            &state.read_summary_suppress_changes,
            &format_display_time(&anchor, &state.time_format),
        );
        state.read_summary_dirty.set(false);
    }
    state
        .read_summary_location
        .set_text(&read_summary_subtitle(state));

    while let Some(child) = state.timeline_labels.first_child() {
        state.timeline_labels.remove(&child);
    }

    let entries = read_entries(state);
    let timeline_items = build_timeline_items(
        &state.config.timezones,
        &state.local_timezone,
        state.reference_utc,
        &state.time_format,
    );
    let side_hours = timeline_side_hours(&timeline_items);

    let timeline_width = f64::from(READ_TIMELINE_WIDTH);
    let label_width = TIMELINE_LABEL_WIDTH;
    let mut lane_last_right = [f64::NEG_INFINITY; TIMELINE_LABEL_LANE_Y.len()];
    for timeline_item in &timeline_items {
        let x = timeline_position_x(
            timeline_item.relative_minutes as f64,
            side_hours,
            timeline_width,
        );
        let left = (x - label_width / 2.0).clamp(0.0, timeline_width - label_width);
        let right = left + label_width;
        let lane_index = lane_last_right
            .iter()
            .position(|last_right| left >= *last_right + TIMELINE_LABEL_LANE_GAP)
            .unwrap_or_else(|| {
                lane_last_right
                    .iter()
                    .enumerate()
                    .min_by(|(_, left), (_, right)| {
                        left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            });
        lane_last_right[lane_index] = right;

        let item = gtk::Box::new(Orientation::Vertical, 6);
        item.set_size_request(label_width as i32, TIMELINE_LABEL_HEIGHT);

        let time_label = gtk::Label::new(Some(&timeline_item.time_text));
        time_label.set_xalign(0.5);
        time_label.add_css_class("timeline-time");
        item.append(&time_label);

        let abbreviation_label = gtk::Label::new(Some(&timeline_item.zone_text));
        abbreviation_label.set_xalign(0.5);
        abbreviation_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        abbreviation_label.set_tooltip_text(timeline_item.zone_tooltip.as_deref());
        abbreviation_label.add_css_class("timeline-zone");
        item.append(&abbreviation_label);

        state
            .timeline_labels
            .put(&item, left, TIMELINE_LABEL_LANE_Y[lane_index]);
    }
    state.timeline_area.queue_draw();

    update_read_cards(state, &entries, &anchor);
}

fn can_add_more_locations(state: &PopupState) -> bool {
    read_entry_count(&state.config.timezones, &state.local_timezone) < READ_CARD_LIMIT
}

fn screen_mode_for_read_entry_count(
    requested: PopupScreen,
    read_entry_count: usize,
) -> PopupScreen {
    if read_entry_count == 0 {
        PopupScreen::Add
    } else {
        requested
    }
}

fn is_dismissible_screen(state: &PopupState) -> bool {
    matches!(state.screen_mode, PopupScreen::Read)
        || (matches!(state.screen_mode, PopupScreen::Add)
            && read_entry_count(&state.config.timezones, &state.local_timezone) == 0)
}

fn update_row_separators(state: &PopupState) {
    let show_separators = !matches!(state.screen_mode, PopupScreen::Edit);
    for separator in &state.row_separators {
        separator.set_visible(show_separators);
    }
}

fn remove_timezone_entry(state_handle: &Rc<RefCell<PopupState>>, timezone_name: &str) {
    let config_manager = {
        let state = state_handle.borrow();
        if state.config.timezones.len() <= 1 {
            set_status(&state, "Keep at least one timezone in the popup.", true);
            return;
        }
        state.config_manager.clone()
    };

    match config_manager.remove_timezone(timezone_name) {
        Ok(config) => {
            let mut state = state_handle.borrow_mut();
            refresh_config_state(&mut state, config);
        }
        Err(error) => {
            let state = state_handle.borrow();
            set_status(&state, &error.to_string(), true);
        }
    }
}

fn toggle_timezone_locked(state_handle: &Rc<RefCell<PopupState>>, timezone_name: &str) {
    let (config_manager, next_locked) = {
        let state = state_handle.borrow();
        let Some(entry) = state
            .config
            .timezones
            .iter()
            .find(|entry| entry.timezone == timezone_name)
        else {
            return;
        };
        (state.config_manager.clone(), !entry.locked)
    };

    match config_manager.set_timezone_locked(timezone_name, next_locked) {
        Ok(config) => {
            let mut state = state_handle.borrow_mut();
            refresh_config_state(&mut state, config);
        }
        Err(error) => {
            let state = state_handle.borrow();
            set_status(&state, &error.to_string(), true);
        }
    }
}

fn sync_map_hover_card(state: &PopupState) {
    if !matches!(state.screen_mode, PopupScreen::Add) {
        state.add_map_hover_card.set_visible(false);
        return;
    }

    let Some(result) = state.hovered_map_result.as_ref() else {
        state.add_map_hover_card.set_visible(false);
        return;
    };

    let zoned = zoned_datetime(state.reference_utc, &result.timezone);
    let anchor = zoned_datetime(state.reference_utc, &state.local_timezone);
    state.add_map_hover_title.set_text(&result.title);
    state
        .add_map_hover_time
        .set_text(&format_display_time(&zoned, &state.time_format));
    state.add_map_hover_meta.set_text(&format!(
        "{}  ·  {}",
        result.timezone,
        format_timezone_notation(&zoned)
    ));
    state
        .add_map_hover_relative
        .set_text(&relative_time_label(&anchor, &zoned));
    state.add_map_hover_card.set_visible(true);
}

fn update_screen_mode(state: &PopupState) {
    // Edit mode reuses the read/card layout and only reveals edit affordances.
    let page_name = match state.screen_mode {
        PopupScreen::Add => "add",
        PopupScreen::Read | PopupScreen::Edit => "read",
    };
    let in_add = matches!(state.screen_mode, PopupScreen::Add);
    let in_edit = matches!(state.screen_mode, PopupScreen::Edit);
    let has_read_entries = read_entry_count(&state.config.timezones, &state.local_timezone) > 0;
    let can_add_more = can_add_more_locations(state);

    state.content_stack.set_visible_child_name(page_name);
    let title = if in_add {
        "Add a Location".to_string()
    } else {
        read_summary_title(state)
    };
    state.panel_title.set_text(&title);

    state.live_button.set_visible(!in_add);
    state.edit_button.set_visible(!in_add);
    if in_edit {
        state.edit_button.add_css_class("active");
        state
            .edit_button
            .set_tooltip_text(Some("Done editing locations."));
    } else {
        state.edit_button.remove_css_class("active");
        state.edit_button.set_tooltip_text(Some("Edit locations."));
    }
    state.add_button.set_visible(!in_add);
    state.cancel_button.set_visible(in_add && has_read_entries);

    state
        .add_button
        .set_tooltip_text(Some("Add a new location."));
    state.add_entry.set_sensitive(can_add_more);
    state.search_results_scroller.set_sensitive(can_add_more);
    state.search_results_box.set_sensitive(can_add_more);
    state.add_map_area.set_sensitive(can_add_more);
    state.add_entry.set_placeholder_text(Some(if can_add_more {
        "Search for a city or timezone"
    } else {
        "Maximum of 9 locations reached"
    }));

    if !in_add || state.add_entry.text().trim().is_empty() || state.search_results.is_empty() {
        state.search_results_scroller.set_visible(false);
    }

    let can_remove = state.config.timezones.len() > 1;
    for row in &state.rows {
        row.drag_handle
            .set_visible(in_edit && row_can_reorder(state, &row.entry));
        row.lock_button.set_visible(in_edit);
        row.remove_button.set_visible(in_edit && can_remove);
        row.remove_button.set_sensitive(can_remove);
    }
    set_read_card_controls(state);
    update_row_separators(state);
    sync_map_hover_card(state);
}

fn merge_search_results(
    local_results: &[TimezoneSearchResult],
    remote_results: &[TimezoneSearchResult],
    limit: usize,
) -> Vec<TimezoneSearchResult> {
    let mut seen_timezones = HashSet::new();
    let mut results = Vec::new();
    for result in local_results.iter().chain(remote_results.iter()) {
        if !seen_timezones.insert(result.timezone.clone()) {
            continue;
        }
        results.push(result.clone());
        if results.len() >= limit {
            break;
        }
    }
    results
}

fn clear_search_results(state: &mut PopupState) {
    clear_box(&state.search_results_box);
    state.local_search_results.clear();
    state.remote_search_results.clear();
    state.search_results.clear();
    state.search_results_scroller.set_visible(false);
}

fn map_coordinates_to_lng_lat(
    area_width: f64,
    area_height: f64,
    x: f64,
    y: f64,
) -> Option<(f64, f64)> {
    if area_width <= 0.0 || area_height <= 0.0 {
        return None;
    }

    let normalized_x = (x / area_width).clamp(0.0, 1.0);
    let normalized_y = (y / area_height).clamp(0.0, 1.0);
    let lng = (normalized_x * 360.0 - 180.0).clamp(-179.999_999, 179.999_999);
    let lat = (90.0 - normalized_y * 180.0).clamp(-89.999_999, 89.999_999);
    Some((lng, lat))
}

fn map_hover_result_at_position(
    state: &PopupState,
    area_width: f64,
    area_height: f64,
    x: f64,
    y: f64,
) -> Option<TimezoneSearchResult> {
    let (lng, lat) = map_coordinates_to_lng_lat(area_width, area_height, x, y)?;
    let timezone_name = state.map_timezone_finder.get_tz_name(lng, lat);
    if timezone_name.is_empty() || timezone_name.starts_with("Etc/") {
        return None;
    }
    state.resolver.describe_timezone(timezone_name)
}

fn set_map_hover_result(
    state_handle: &Rc<RefCell<PopupState>>,
    hover_result: Option<TimezoneSearchResult>,
    cursor_x: f64,
    cursor_y: f64,
    area_width: f64,
    area_height: f64,
) {
    let Ok(mut state) = state_handle.try_borrow_mut() else {
        debug_popup_event("set_map_hover_result skipped busy_state");
        return;
    };
    if state.hovered_map_result == hover_result {
        return;
    }

    state.hovered_map_result = hover_result;
    sync_map_hover_card(&state);
    if state.hovered_map_result.is_none() {
        state.add_map_area.queue_draw();
        return;
    }

    let card_x = (cursor_x + 20.0).clamp(
        18.0,
        (area_width - f64::from(ADD_MAP_HOVER_CARD_WIDTH) - 18.0).max(18.0),
    );
    let card_y = (cursor_y + 18.0).clamp(
        18.0,
        (area_height - f64::from(ADD_MAP_HOVER_CARD_HEIGHT) - 18.0).max(18.0),
    );
    state
        .add_map_hover_layer
        .move_(&state.add_map_hover_card, card_x, card_y);
    state.add_map_area.queue_draw();
}

fn set_screen_mode(state_handle: &Rc<RefCell<PopupState>>, screen_mode: PopupScreen) {
    debug_popup_event(&format!("set_screen_mode screen={screen_mode:?}"));
    let (focus_widget, entry_to_clear, queue_map_draw, rearm_dismiss_after_transition) = {
        let mut state = state_handle.borrow_mut();
        let screen_mode = screen_mode_for_read_entry_count(
            screen_mode,
            read_entry_count(&state.config.timezones, &state.local_timezone),
        );
        let leaving_add = matches!(state.screen_mode, PopupScreen::Add)
            && !matches!(screen_mode, PopupScreen::Add);
        let reentering_read_from_add = leaving_add && matches!(screen_mode, PopupScreen::Read);
        state.screen_mode = screen_mode;

        if leaving_add {
            clear_search_results(&mut state);
        }
        if reentering_read_from_add {
            state.dismiss_armed = false;
        }
        if !matches!(screen_mode, PopupScreen::Add) {
            state.hovered_map_result = None;
        }
        render_read_view(&mut state);
        update_screen_mode(&state);
        (
            match screen_mode {
                PopupScreen::Add => Some(state.add_entry.clone().upcast::<gtk::Widget>()),
                PopupScreen::Read if reentering_read_from_add => {
                    Some(state.add_button.clone().upcast::<gtk::Widget>())
                }
                _ => None,
            },
            leaving_add.then(|| state.add_entry.clone()),
            state.add_map_area.clone(),
            reentering_read_from_add,
        )
    };

    if let Some(entry) = entry_to_clear {
        entry.set_text("");
    }

    if let Some(widget) = focus_widget {
        glib::idle_add_local_once(move || {
            let _ = widget.grab_focus();
        });
    }
    if rearm_dismiss_after_transition {
        let state_for_rearm = state_handle.clone();
        glib::timeout_add_local_once(Duration::from_millis(150), move || {
            state_for_rearm.borrow_mut().dismiss_armed = true;
        });
    }
    queue_map_draw.queue_draw();
}

fn refresh_config_state(state: &mut PopupState, config: AppConfig) {
    cancel_pending_apply(state);
    state.config = config;
    state.time_format = effective_time_format(&state.config.time_format);
    state.screen_mode = screen_mode_for_read_entry_count(
        state.screen_mode,
        read_entry_count(&state.config.timezones, &state.local_timezone),
    );
    sync_dropdowns(state);
    if state.editing_timezone.as_ref().is_some_and(|timezone| {
        !state
            .config
            .timezones
            .iter()
            .any(|entry| entry.timezone == *timezone)
    }) {
        state.editing_timezone = None;
    }
    clear_status(state);
    render_rows(state);
}

fn build_row(entry: &TimezoneEntry, time_format: &str) -> RowWidgets {
    let row = gtk::Box::new(Orientation::Horizontal, 16);
    row.add_css_class("clock-row");

    let drag_handle = gtk::Box::new(Orientation::Horizontal, 0);
    drag_handle.set_visible(false);
    drag_handle.set_valign(Align::Center);
    drag_handle.set_margin_end(4);
    drag_handle.add_css_class("drag-handle");
    drag_handle.set_tooltip_text(Some("Drag to reorder."));

    let drag_label = gtk::Label::new(Some("≡"));
    drag_label.add_css_class("drag-handle-label");
    drag_handle.append(&drag_label);
    row.append(&drag_handle);

    let info = gtk::Box::new(Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.set_valign(Align::Center);

    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.add_css_class("clock-title");
    info.append(&title);

    let context = gtk::Label::new(None);
    context.set_xalign(0.0);
    context.add_css_class("clock-context");
    info.append(&context);

    let meta = gtk::Label::new(None);
    meta.set_xalign(0.0);
    meta.add_css_class("clock-meta");
    info.append(&meta);

    row.append(&info);

    let controls = gtk::Box::new(Orientation::Horizontal, 8);
    controls.set_halign(Align::End);
    controls.set_valign(Align::Center);

    let time_entry = gtk::Entry::new();
    gtk::prelude::EditableExt::set_alignment(&time_entry, 1.0);
    time_entry.set_width_chars(time_entry_width_chars(time_format));
    time_entry.set_max_length(19);
    time_entry.set_placeholder_text(Some(time_entry_placeholder(time_format)));
    time_entry.add_css_class("time-entry");
    controls.append(&time_entry);

    let lock_button = gtk::Button::from_icon_name("view-pin-symbolic");
    lock_button.add_css_class("icon-button");
    lock_button.set_size_request(32, 32);
    lock_button.set_valign(Align::Center);
    lock_button.set_tooltip_text(Some("Pin this timezone above unlocked rows."));
    lock_button.set_visible(false);
    controls.append(&lock_button);

    let remove_button = gtk::Button::from_icon_name("edit-delete-symbolic");
    remove_button.add_css_class("icon-button");
    remove_button.add_css_class("remove-button");
    remove_button.add_css_class("destructive");
    remove_button.set_size_request(32, 32);
    remove_button.set_valign(Align::Center);
    remove_button.set_tooltip_text(Some("Remove timezone."));
    remove_button.set_visible(false);
    controls.append(&remove_button);

    row.append(&controls);

    RowWidgets {
        entry: entry.clone(),
        root: row,
        drag_handle,
        title,
        context,
        meta,
        lock_button,
        remove_button,
        time_entry,
        dirty: Rc::new(Cell::new(false)),
        suppress_changes: Rc::new(Cell::new(false)),
    }
}

fn bind_time_entry_events(
    state_handle: &Rc<RefCell<PopupState>>,
    time_entry: &gtk::Entry,
    timezone_name: String,
    dirty: Rc<Cell<bool>>,
    suppress_changes: Rc<Cell<bool>>,
) {
    let state_for_change = state_handle.clone();
    let timezone_name_for_change = timezone_name.clone();
    let dirty_for_change = dirty.clone();
    let suppress_changes_for_change = suppress_changes.clone();
    time_entry.connect_changed(move |time_entry| {
        if suppress_changes_for_change.get() {
            return;
        }

        dirty_for_change.set(true);
        set_entry_error(time_entry, false);
        if let Ok(state) = state_for_change.try_borrow() {
            clear_status(&state);
        }
        schedule_live_apply(&state_for_change, &timezone_name_for_change);
    });

    let focus_controller = gtk::EventControllerFocus::new();
    let timezone_name_for_enter = timezone_name.clone();
    let dirty_for_enter = dirty.clone();
    let state_for_enter = state_handle.clone();
    let time_entry_for_enter = time_entry.clone();
    focus_controller.connect_enter(move |focus_controller| {
        dirty_for_enter.set(false);
        set_entry_error(&time_entry_for_enter, false);
        time_entry_for_enter.select_region(0, -1);
        update_time_entry_focus_state(
            &state_for_enter,
            focus_controller,
            timezone_name_for_enter.clone(),
        );
    });

    let timezone_name_for_leave = timezone_name.clone();
    let dirty_for_leave = dirty.clone();
    let state_for_leave = state_handle.clone();
    focus_controller.connect_leave(move |focus_controller| {
        clear_time_entry_focus_state(
            &state_for_leave,
            focus_controller,
            timezone_name_for_leave.clone(),
            dirty_for_leave.clone(),
        );
    });
    time_entry.add_controller(focus_controller);

    let state_for_activate = state_handle.clone();
    time_entry.connect_activate(move |_| {
        let _ = flush_live_apply(&state_for_activate, &timezone_name, true);
    });
}

fn bind_row_events(state_handle: &Rc<RefCell<PopupState>>, row: &RowWidgets) {
    let timezone_name = row.entry.timezone.clone();
    bind_time_entry_events(
        state_handle,
        &row.time_entry,
        timezone_name.clone(),
        row.dirty.clone(),
        row.suppress_changes.clone(),
    );

    let timezone_name_for_remove = row.entry.timezone.clone();
    let state_for_remove = state_handle.clone();
    row.remove_button.connect_clicked(move |_| {
        remove_timezone_entry(&state_for_remove, &timezone_name_for_remove);
    });

    let timezone_name_for_lock = row.entry.timezone.clone();
    let state_for_lock = state_handle.clone();
    row.lock_button.connect_clicked(move |_| {
        toggle_timezone_locked(&state_for_lock, &timezone_name_for_lock);
    });

    let drag_gesture = gtk::GestureDrag::new();
    drag_gesture.set_button(1);

    let timezone_name_for_drag_begin = row.entry.timezone.clone();
    let state_for_drag_begin = state_handle.clone();
    drag_gesture.connect_drag_begin(move |_, _, _| {
        begin_drag(&state_for_drag_begin, &timezone_name_for_drag_begin);
    });

    let timezone_name_for_drag_update = row.entry.timezone.clone();
    let state_for_drag_update = state_handle.clone();
    drag_gesture.connect_drag_update(move |_, _, offset_y| {
        let mut state = state_for_drag_update.borrow_mut();
        if state.drag_source_timezone.as_deref() != Some(timezone_name_for_drag_update.as_str()) {
            return;
        }
        set_drag_ghost_position(&state, offset_y);
        let rows_box_y = state.drag_start_rows_box_y + offset_y;
        update_drag_position(&mut state, rows_box_y);
    });

    let timezone_name_for_drag_end = row.entry.timezone.clone();
    let state_for_drag_end = state_handle.clone();
    drag_gesture.connect_drag_end(move |_, _, offset_y| {
        let insert_index = {
            let mut state = state_for_drag_end.borrow_mut();
            if state.drag_source_timezone.as_deref() != Some(timezone_name_for_drag_end.as_str()) {
                return;
            }
            set_drag_ghost_position(&state, offset_y);
            let rows_box_y = state.drag_start_rows_box_y + offset_y;
            update_drag_position(&mut state, rows_box_y);
            let insert_index = state.active_drop_index;
            end_drag(&mut state);
            insert_index
        };

        if let Some(insert_index) = insert_index {
            let _ = reorder_timezone_to_index(
                &state_for_drag_end,
                &timezone_name_for_drag_end,
                insert_index,
            );
        }
    });

    row.drag_handle.add_controller(drag_gesture);
}

fn format_title(entry: &TimezoneEntry, local_timezone: &str) -> String {
    let mut title = entry.display_label();
    if entry.timezone == local_timezone {
        title = format!("{title}  ·  Local");
    }
    title
}

fn read_card_title(entry: &TimezoneEntry) -> String {
    let label = entry.display_label();
    label
        .split(',')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or(&label)
        .to_string()
}

fn update_row_widgets(state: &mut PopupState) {
    let ordered = selected_entries(state);
    sync_dropdowns(state);
    let current_order: Vec<String> = state
        .rows
        .iter()
        .map(|row| row.entry.timezone.clone())
        .collect();
    let desired_order: Vec<String> = ordered.iter().map(|entry| entry.timezone.clone()).collect();
    if current_order != desired_order {
        render_rows(state);
        return;
    }

    for (row, entry) in state.rows.iter_mut().zip(ordered.iter()) {
        row.entry = entry.clone();
        let zoned = zoned_datetime(state.reference_utc, &entry.timezone);
        row.title
            .set_text(&format_title(entry, &state.local_timezone));
        row.context.set_text(&entry.timezone);
        row.meta.set_text(&row_metadata(&zoned));
        row.time_entry
            .set_placeholder_text(Some(time_entry_placeholder(&state.time_format)));
        row.time_entry
            .set_width_chars(time_entry_width_chars(&state.time_format));
        row.lock_button.set_icon_name("view-pin-symbolic");
        row.lock_button.set_tooltip_text(Some(if entry.locked {
            "Pinned above unlocked rows."
        } else {
            "Pin this timezone above unlocked rows."
        }));
        if entry.locked {
            row.lock_button.add_css_class("active");
        } else {
            row.lock_button.remove_css_class("active");
        }
        row.remove_button
            .set_sensitive(state.config.timezones.len() > 1);

        if state.editing_timezone.as_deref() == Some(row.entry.timezone.as_str()) {
            continue;
        }

        set_row_error(row, false);
        row.suppress_changes.set(true);
        row.time_entry
            .set_text(&format_display_time(&zoned, &state.time_format));
        row.suppress_changes.set(false);
        row.dirty.set(false);
    }
    render_read_view(state);
    sync_map_hover_card(state);
}

fn render_rows(state: &mut PopupState) {
    clear_drop_slot(state);
    clear_box(&state.rows_box);
    state.rows.clear();
    state.row_separators.clear();

    let entries = selected_entries(state);
    if entries.is_empty() {
        state.screen_mode = PopupScreen::Add;
        update_screen_mode(state);
        return;
    }

    let state_handle = state.self_handle.upgrade();
    for (index, entry) in entries.iter().enumerate() {
        let widgets = build_row(entry, &state.time_format);
        if let Some(handle) = &state_handle {
            bind_row_events(handle, &widgets);
        }
        state.rows_box.append(&widgets.root);
        state.rows.push(widgets);

        if index + 1 < entries.len() {
            let separator = gtk::Separator::new(Orientation::Horizontal);
            state.rows_box.append(&separator);
            state.row_separators.push(separator);
        }
    }

    update_row_widgets(state);
    update_screen_mode(state);
}

fn render_search_results(state_handle: &Rc<RefCell<PopupState>>) {
    let results = state_handle.borrow().search_results.clone();
    let state = state_handle.borrow();
    clear_box(&state.search_results_box);

    if results.is_empty() {
        state.search_results_scroller.set_visible(false);
        return;
    }

    for result in results {
        let button = gtk::Button::new();
        button.set_halign(Align::Fill);
        button.set_hexpand(true);
        button.add_css_class("search-result-button");

        let content = gtk::Box::new(Orientation::Vertical, 2);
        content.set_halign(Align::Start);

        let title = gtk::Label::new(Some(&result.title));
        title.set_xalign(0.0);
        title.add_css_class("search-result-title");
        content.append(&title);

        let meta = gtk::Label::new(Some(&search_result_subtitle(&result, &state.reference_utc)));
        meta.set_xalign(0.0);
        meta.add_css_class("search-result-meta");
        content.append(&meta);

        button.set_child(Some(&content));

        let state_for_click = state_handle.clone();
        let result_for_click = result.clone();
        button.connect_clicked(move |_| {
            add_timezone(
                &state_for_click,
                &result_for_click.timezone,
                &result_for_click.title,
            );
        });

        state.search_results_box.append(&button);
    }

    state.search_results_scroller.set_visible(true);
}

fn update_search_results(state_handle: &Rc<RefCell<PopupState>>) {
    let query = state_handle.borrow().add_entry.text().trim().to_string();
    let mut remote_search = None;
    {
        let mut state = state_handle.borrow_mut();
        state.search_generation = state.search_generation.wrapping_add(1);
        state.remote_search_results.clear();

        if query.is_empty() {
            clear_search_results(&mut state);
            return;
        }

        state.local_search_results = state.resolver.search(&query, ADD_SEARCH_RESULT_LIMIT);
        state.search_results = merge_search_results(
            &state.local_search_results,
            &state.remote_search_results,
            ADD_SEARCH_RESULT_LIMIT,
        );

        if state.local_search_results.is_empty() && TimezoneResolver::normalize(&query).len() >= 3 {
            remote_search = Some((
                state.search_generation,
                state.remote_search_sender.clone(),
                state.place_search.clone(),
                query.clone(),
            ));
        }
    }
    render_search_results(state_handle);

    if let Some((generation, sender, place_search, query)) = remote_search {
        thread::spawn(move || {
            let results = place_search
                .lock()
                .map(|mut search| search.search(&query, ADD_SEARCH_RESULT_LIMIT))
                .unwrap_or_default();
            let _ = sender.send(RemoteSearchMessage {
                generation,
                query,
                results,
            });
        });
    }
}

fn label_for_input(state: &PopupState, raw_value: &str, timezone_name: &str) -> String {
    let value = raw_value.trim();
    if value.is_empty() {
        return String::new();
    }
    if value.eq_ignore_ascii_case(timezone_name) {
        return value.to_string();
    }
    if value
        .replace('_', " ")
        .eq_ignore_ascii_case(&timezone_name.replace('_', " "))
    {
        return value.to_string();
    }
    let matches = state.resolver.search(value, 1);
    if matches
        .first()
        .is_some_and(|result| result.timezone == timezone_name)
    {
        return matches[0].title.clone();
    }
    value.to_string()
}

fn search_result_subtitle(result: &TimezoneSearchResult, reference_utc: &DateTime<Utc>) -> String {
    let abbreviation = format_timezone_notation(&zoned_datetime(*reference_utc, &result.timezone));
    let mut parts = result
        .subtitle
        .split("  ·  ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if parts.is_empty() {
        parts.push(result.timezone.clone());
    }

    if parts.first().is_some_and(|part| part == &abbreviation) {
        return parts.join("  ·  ");
    }

    if parts.len() > 1 && parts[1].split('/').any(|part| part.trim() == abbreviation) {
        parts[1] = abbreviation;
    } else if parts.first().is_some_and(|part| part != &abbreviation) {
        parts.insert(1, abbreviation);
    }

    parts.join("  ·  ")
}

fn single_visible_search_match(
    state: &PopupState,
    raw_value: &str,
) -> Option<TimezoneSearchResult> {
    if state.search_results.len() != 1 {
        return None;
    }
    let normalized_value = TimezoneResolver::normalize(raw_value);
    if normalized_value.is_empty() {
        return None;
    }
    let result = state.search_results[0].clone();
    if TimezoneResolver::normalize(&result.title).starts_with(&normalized_value) {
        return Some(result);
    }
    None
}

fn submit_add_timezone(state_handle: &Rc<RefCell<PopupState>>) {
    let raw_value = state_handle.borrow().add_entry.text().trim().to_string();
    let timezone_name = {
        let state = state_handle.borrow();
        state.resolver.resolve(&raw_value)
    };

    let Some(timezone_name) = timezone_name else {
        let single_match = {
            let state = state_handle.borrow();
            single_visible_search_match(&state, &raw_value)
        };
        if let Some(result) = single_match {
            add_timezone(state_handle, &result.timezone, &result.title);
            return;
        }

        let state = state_handle.borrow();
        if state.search_results.is_empty() {
            set_status(
                &state,
                "Enter a valid timezone, city, or abbreviation like IST.",
                true,
            );
        } else {
            set_status(&state, "Pick one of the matching timezones below.", true);
        }
        return;
    };

    let label = {
        let state = state_handle.borrow();
        label_for_input(&state, &raw_value, &timezone_name)
    };
    add_timezone(state_handle, &timezone_name, &label);
}

fn add_timezone(state_handle: &Rc<RefCell<PopupState>>, timezone_name: &str, label: &str) {
    let display_name = if label.trim().is_empty() {
        timezone_name.to_string()
    } else {
        label.trim().to_string()
    };

    {
        let state = state_handle.borrow();
        if state
            .config
            .timezones
            .iter()
            .any(|entry| entry.timezone == timezone_name)
        {
            set_status(
                &state,
                &format!("{display_name} is already in the list."),
                true,
            );
            return;
        }

        if read_entry_count(&state.config.timezones, &state.local_timezone) >= READ_CARD_LIMIT {
            set_status(
                &state,
                &format!(
                    "The read view can show up to {READ_CARD_LIMIT} cards. Remove one before adding another timezone."
                ),
                true,
            );
            return;
        }
    }

    let config_manager = state_handle.borrow().config_manager.clone();
    match config_manager.add_timezone(timezone_name, label) {
        Ok(config) => {
            debug_popup_event(&format!(
                "add_timezone success timezone={timezone_name} label={display_name}"
            ));
            {
                let mut state = state_handle.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            set_screen_mode(state_handle, PopupScreen::Read);
            {
                let state = state_handle.borrow();
                set_status(&state, &format!("Added {display_name}."), false);
            }
        }
        Err(error) => {
            let state = state_handle.borrow();
            set_status(&state, &error.to_string(), true);
        }
    }
}

fn reorder_timezone(
    state_handle: &Rc<RefCell<PopupState>>,
    timezone_name: &str,
    target_timezone_name: &str,
    place_after: bool,
) -> bool {
    let state = state_handle.borrow();
    let source_entry = state
        .config
        .timezones
        .iter()
        .find(|entry| entry.timezone == timezone_name);
    let target_entry = state
        .config
        .timezones
        .iter()
        .find(|entry| entry.timezone == target_timezone_name);
    let (Some(_), Some(_)) = (source_entry, target_entry) else {
        return false;
    };
    drop(state);

    let config_manager = state_handle.borrow().config_manager.clone();
    match config_manager.reorder_timezone(timezone_name, target_timezone_name, place_after) {
        Ok(config) => {
            let mut state = state_handle.borrow_mut();
            if config.timezones == state.config.timezones {
                return false;
            }
            refresh_config_state(&mut state, config);
            true
        }
        Err(error) => {
            let state = state_handle.borrow();
            set_status(&state, &error.to_string(), true);
            false
        }
    }
}

fn reorder_timezone_to_index(
    state_handle: &Rc<RefCell<PopupState>>,
    timezone_name: &str,
    insert_index: usize,
) -> bool {
    let entries = match state_handle.try_borrow() {
        Ok(state) => selected_entries(&state),
        Err(_) => {
            debug_popup_event("reorder_timezone_to_index deferred busy_state");
            let state_for_retry = state_handle.clone();
            let timezone_name_for_retry = timezone_name.to_string();
            glib::idle_add_local_once(move || {
                let _ = reorder_timezone_to_index(
                    &state_for_retry,
                    &timezone_name_for_retry,
                    insert_index,
                );
            });
            return false;
        }
    };
    if entries.is_empty() || insert_index > entries.len() {
        return false;
    }
    let (target_timezone_name, place_after) = if insert_index == entries.len() {
        (entries.last().unwrap().timezone.clone(), true)
    } else {
        (entries[insert_index].timezone.clone(), false)
    };
    reorder_timezone(
        state_handle,
        timezone_name,
        &target_timezone_name,
        place_after,
    )
}

fn build_drag_preview(state: &PopupState, row: &RowWidgets) -> gtk::Widget {
    let preview = gtk::Box::new(Orientation::Horizontal, 14);
    preview.add_css_class("clock-row");
    preview.add_css_class("drag-preview");

    let handle_label = gtk::Label::new(Some("≡"));
    handle_label.add_css_class("drag-handle-label");
    preview.append(&handle_label);

    let info = gtk::Box::new(Orientation::Vertical, 2);
    info.set_hexpand(true);

    let title = gtk::Label::new(Some(&row.title.text()));
    title.set_xalign(0.0);
    title.add_css_class("clock-title");
    info.append(&title);

    let context = gtk::Label::new(Some(&row.entry.timezone));
    context.set_xalign(0.0);
    context.add_css_class("clock-context");
    info.append(&context);
    preview.append(&info);

    let time_label = gtk::Label::new(Some(&format_display_time(
        &zoned_datetime(state.reference_utc, &row.entry.timezone),
        &state.time_format,
    )));
    time_label.set_xalign(1.0);
    time_label.add_css_class("drag-preview-time");
    preview.append(&time_label);

    let width = row.root.allocation().width();
    if width > 0 {
        preview.set_size_request(width, -1);
    }

    preview.upcast::<gtk::Widget>()
}

fn begin_drag(state_handle: &Rc<RefCell<PopupState>>, timezone_name: &str) {
    let mut state = state_handle.borrow_mut();
    let Some(row_index) = state
        .rows
        .iter()
        .position(|row| row.entry.timezone == timezone_name)
    else {
        return;
    };
    if !matches!(state.screen_mode, PopupScreen::Edit)
        || !row_can_reorder(&state, &state.rows[row_index].entry)
    {
        return;
    }

    state.rows_overlay.queue_allocate();
    state.rows_box.queue_allocate();
    let (rows_box_y, overlay_x, overlay_y, ghost) = {
        let row = &state.rows[row_index];
        let row_allocation = row.root.allocation();
        let translated = row.root.translate_coordinates(
            &state.rows_box,
            0.0,
            f64::from(row_allocation.height()) / 2.0,
        );
        let overlay_origin = row
            .root
            .translate_coordinates(&state.rows_overlay, 0.0, 0.0);
        let (Some((_, rows_box_y)), Some((overlay_x, overlay_y))) = (translated, overlay_origin)
        else {
            return;
        };
        let ghost = build_drag_preview(&state, row);
        (rows_box_y, overlay_x, overlay_y, ghost)
    };

    end_drag(&mut state);

    state.drag_layer.put(&ghost, overlay_x, overlay_y);
    ghost.set_visible(true);
    if let Some(row) = state.rows.get(row_index) {
        row.root.add_css_class("dragging");
    }

    state.drag_source_timezone = Some(timezone_name.to_string());
    state.drag_start_rows_box_y = rows_box_y;
    state.drag_start_row_top_y = overlay_y;
    state.drag_row_overlay_x = overlay_x;
    state.drag_ghost = Some(ghost);
}

fn set_drag_ghost_position(state: &PopupState, offset_y: f64) {
    if let Some(ghost) = &state.drag_ghost {
        let ghost_y = state.drag_start_row_top_y + offset_y;
        state
            .drag_layer
            .move_(ghost, state.drag_row_overlay_x, ghost_y);
        ghost.queue_draw();
    }
}

fn update_drag_position(state: &mut PopupState, rows_box_y: f64) {
    let Some(source_timezone) = state.drag_source_timezone.as_deref() else {
        clear_drop_slot(state);
        return;
    };

    let reorderable_rows = state
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.entry.timezone != source_timezone)
        .collect::<Vec<_>>();
    if reorderable_rows.is_empty() {
        clear_drop_slot(state);
        return;
    }

    let mut insert_index = reorderable_rows
        .last()
        .map(|(index, _)| index + 1)
        .unwrap_or(0);
    for (row_index, row) in reorderable_rows {
        let midpoint = row
            .root
            .translate_coordinates(
                &state.rows_box,
                0.0,
                f64::from(row.root.allocation().height()) / 2.0,
            )
            .map(|(_, midpoint)| midpoint);
        let Some(midpoint) = midpoint else {
            continue;
        };
        if rows_box_y < midpoint {
            insert_index = row_index;
            break;
        }
    }

    if !can_drop_at_index(state, insert_index) {
        clear_drop_slot(state);
        return;
    }
    show_drop_marker(state, insert_index);
}

fn can_reorder_entry_at_index(
    state: &PopupState,
    source_timezone: &str,
    insert_index: usize,
) -> bool {
    let entries = selected_entries(state);
    let Some(source_index) = entries
        .iter()
        .position(|entry| entry.timezone == source_timezone)
    else {
        return false;
    };
    if entries[source_index].locked || state.config.sort_mode != "manual" {
        return false;
    }
    let locked_count = entries.iter().take_while(|entry| entry.locked).count();
    if insert_index < locked_count {
        return false;
    }
    let effective_index = if source_index < insert_index {
        insert_index.saturating_sub(1)
    } else {
        insert_index
    };
    effective_index != source_index
}

fn can_drop_at_index(state: &PopupState, insert_index: usize) -> bool {
    let Some(source_timezone) = state.drag_source_timezone.as_deref() else {
        return false;
    };
    can_reorder_entry_at_index(state, source_timezone, insert_index)
}

fn clear_drop_slot(state: &mut PopupState) {
    if state.active_drop_index.is_none() && state.insertion_marker.parent().is_none() {
        return;
    }
    if state.insertion_marker.parent().is_some() {
        state.rows_box.remove(&state.insertion_marker);
    }
    state.insertion_marker.set_visible(false);
    state.active_drop_index = None;
}

fn show_drop_marker(state: &mut PopupState, insert_index: usize) {
    if state.active_drop_index == Some(insert_index) && state.insertion_marker.parent().is_some() {
        return;
    }

    let insertion_marker: &gtk::Widget = state.insertion_marker.upcast_ref();
    let children = box_children(&state.rows_box)
        .into_iter()
        .filter(|child| child != insertion_marker)
        .collect::<Vec<_>>();
    let marker_position = if insert_index < state.rows.len() {
        let row_root: &gtk::Widget = state.rows[insert_index].root.upcast_ref();
        children
            .iter()
            .position(|child| child == row_root)
            .unwrap_or(children.len())
    } else {
        children.len()
    };
    let previous_sibling = if marker_position == 0 {
        None
    } else {
        Some(children[marker_position - 1].clone())
    };

    if state.insertion_marker.parent().is_some() {
        state.rows_box.remove(&state.insertion_marker);
    }
    state
        .rows_box
        .insert_child_after(&state.insertion_marker, previous_sibling.as_ref());
    state.insertion_marker.set_visible(true);
    state.rows_box.queue_allocate();
    state.active_drop_index = Some(insert_index);
}

fn end_drag(state: &mut PopupState) {
    clear_drop_slot(state);
    if let Some(ghost) = state.drag_ghost.take() {
        state.drag_layer.remove(&ghost);
    }
    if let Some(source_timezone) = state.drag_source_timezone.take() {
        if let Some(row) = state
            .rows
            .iter()
            .find(|row| row.entry.timezone == source_timezone)
        {
            row.root.remove_css_class("dragging");
        }
    }
}

fn configure_layer_shell(window: &gtk::Window) -> Option<(i32, i32)> {
    window.init_layer_shell();
    window.set_namespace(Some("omarchy-world-clock"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    let top_margin = popup_top_margin(
        load_window_gap(),
        load_window_border_size(),
        POPUP_TOP_CONTENT_MARGIN,
    );
    window.set_margin(Edge::Top, top_margin);

    let display = gdk::Display::default()?;
    let monitor = target_monitor(&display)?;
    let geometry = monitor.geometry();
    let width = geometry.width().max(200);
    let height = (geometry.height() - top_margin).max(200);

    window.set_monitor(Some(&monitor));
    window.set_default_size(width, height);
    Some((width, height))
}

fn reset_live_now(state_handle: &Rc<RefCell<PopupState>>) {
    let mut state = state_handle.borrow_mut();
    cancel_pending_apply(&mut state);
    state.live = true;
    state.reference_utc = Utc::now();
    state.read_summary_dirty.set(false);
    set_entry_error(&state.read_summary_time, false);
    for card in &state.read_cards {
        card.dirty.set(false);
        set_entry_error(&card.time_entry, false);
    }
    for row in &state.rows {
        row.dirty.set(false);
        set_row_error(row, false);
    }
    clear_status(&state);
    update_live_button(&state);
    update_row_widgets(&mut state);
}

#[derive(Clone, Copy)]
enum ManualEntryTarget {
    Summary,
    Card(usize),
    Row(usize),
}

fn apply_manual_entry(
    state_handle: &Rc<RefCell<PopupState>>,
    timezone_name: &str,
    show_errors: bool,
) -> bool {
    let (raw_value, dirty, target) = {
        let state = state_handle.borrow();
        if timezone_name == state.local_timezone && state.read_summary_dirty.get() {
            (
                state.read_summary_time.text().to_string(),
                state.read_summary_dirty.get(),
                ManualEntryTarget::Summary,
            )
        } else if let Some((index, card)) = state
            .read_cards
            .iter()
            .enumerate()
            .find(|(_, card)| card.entry.timezone == timezone_name && card.dirty.get())
        {
            (
                card.time_entry.text().to_string(),
                card.dirty.get(),
                ManualEntryTarget::Card(index),
            )
        } else if let Some((index, row)) = state
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.entry.timezone == timezone_name)
        {
            (
                row.time_entry.text().to_string(),
                row.dirty.get(),
                ManualEntryTarget::Row(index),
            )
        } else {
            return false;
        }
    };
    if !dirty {
        return false;
    }

    let reference_utc = state_handle.borrow().reference_utc;
    let parsed_reference =
        match parse_manual_reference_details(&raw_value, timezone_name, reference_utc) {
            Ok(parsed) => parsed,
            Err(error) => {
                if show_errors {
                    let state = state_handle.borrow();
                    match target {
                        ManualEntryTarget::Summary => {
                            set_entry_error(&state.read_summary_time, true);
                        }
                        ManualEntryTarget::Card(index) => {
                            if let Some(card) = state.read_cards.get(index) {
                                set_entry_error(&card.time_entry, true);
                            }
                        }
                        ManualEntryTarget::Row(index) => {
                            if let Some(row) = state.rows.get(index) {
                                set_row_error(row, true);
                            }
                        }
                    }
                    set_status(&state, error, true);
                }
                return false;
            }
        };

    let mut state = state_handle.borrow_mut();
    state.reference_utc = parsed_reference.reference_utc;
    state.live = false;
    clear_status(&state);
    update_live_button(&state);

    if show_errors {
        let rendered = format_display_time(
            &zoned_datetime(parsed_reference.reference_utc, timezone_name),
            &state.time_format,
        );
        match target {
            ManualEntryTarget::Summary => {
                set_time_entry_text(
                    &state.read_summary_time,
                    &state.read_summary_suppress_changes,
                    &rendered,
                );
                state.read_summary_time.set_position(-1);
            }
            ManualEntryTarget::Card(index) => {
                if let Some(active_card) = state.read_cards.get(index) {
                    set_time_entry_text(
                        &active_card.time_entry,
                        &active_card.suppress_changes,
                        &rendered,
                    );
                    active_card.time_entry.set_position(-1);
                }
            }
            ManualEntryTarget::Row(index) => {
                if let Some(active_row) = state.rows.get(index) {
                    set_time_entry_text(
                        &active_row.time_entry,
                        &active_row.suppress_changes,
                        &rendered,
                    );
                    active_row.time_entry.set_position(-1);
                }
            }
        }
    }

    let preserve_target_dirty =
        !show_errors && state.editing_timezone.as_deref() == Some(timezone_name);
    state
        .read_summary_dirty
        .set(preserve_target_dirty && matches!(target, ManualEntryTarget::Summary));
    set_entry_error(&state.read_summary_time, false);
    for (index, card) in state.read_cards.iter().enumerate() {
        card.dirty.set(
            preserve_target_dirty
                && matches!(target, ManualEntryTarget::Card(target_index) if target_index == index),
        );
        set_entry_error(&card.time_entry, false);
    }
    for (index, row) in state.rows.iter().enumerate() {
        row.dirty.set(
            preserve_target_dirty
                && matches!(target, ManualEntryTarget::Row(target_index) if target_index == index),
        );
        set_row_error(row, false);
    }
    update_row_widgets(&mut state);
    true
}

fn build_window(
    config_manager: ConfigManager,
    config: AppConfig,
    local_timezone: String,
    window: &gtk::Window,
) -> Rc<RefCell<PopupState>> {
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);

    let dismiss_area = gtk::Box::new(Orientation::Vertical, 0);
    dismiss_area.set_hexpand(true);
    dismiss_area.set_vexpand(true);
    overlay.set_child(Some(&dismiss_area));

    let top_band = gtk::Box::new(Orientation::Vertical, 0);
    top_band.set_halign(Align::Center);
    top_band.set_valign(Align::Center);
    overlay.add_overlay(&top_band);

    let panel = gtk::Box::new(Orientation::Vertical, 14);
    panel.add_css_class("world-clock-panel");
    panel.set_width_request(READ_PANEL_WIDTH);
    panel.set_size_request(READ_PANEL_WIDTH, -1);
    panel.set_halign(Align::Center);
    top_band.append(&panel);

    let header = gtk::CenterBox::new();
    panel.append(&header);

    let title = gtk::Label::new(Some("World Clock"));
    title.set_xalign(0.5);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    title.set_max_width_chars(32);
    title.add_css_class("panel-title");
    header.set_center_widget(Some(&title));

    let header_start = gtk::Box::new(Orientation::Horizontal, 8);
    header.set_start_widget(Some(&header_start));

    let live_button = gtk::Button::from_icon_name("view-refresh-symbolic");
    live_button.add_css_class("icon-button");
    live_button.set_valign(Align::Center);
    header_start.append(&live_button);

    let header_actions = gtk::Box::new(Orientation::Horizontal, 8);
    header.set_end_widget(Some(&header_actions));

    let cancel_button = gtk::Button::with_label("Cancel");
    cancel_button.add_css_class("flat-button");
    cancel_button.set_valign(Align::Center);
    cancel_button.set_visible(false);
    header_actions.append(&cancel_button);

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");
    add_button.add_css_class("icon-button");
    add_button.set_valign(Align::Center);
    header_actions.append(&add_button);

    let edit_button = gtk::Button::from_icon_name("document-edit-symbolic");
    edit_button.add_css_class("icon-button");
    edit_button.set_valign(Align::Center);
    header_actions.append(&edit_button);

    let content_stack = gtk::Stack::new();
    content_stack.set_hhomogeneous(false);
    content_stack.set_vhomogeneous(false);
    panel.append(&content_stack);

    let read_root = gtk::Box::new(Orientation::Vertical, READ_SECTION_SPACING);
    read_root.add_css_class("read-mode");
    content_stack.add_named(&read_root, Some("read"));

    let read_summary = gtk::Box::new(Orientation::Vertical, 0);
    read_summary.set_halign(Align::Center);
    read_root.append(&read_summary);

    let read_summary_time = gtk::Entry::new();
    gtk::prelude::EditableExt::set_alignment(&read_summary_time, 0.5);
    configure_manual_time_entry(&read_summary_time, DEFAULT_TIME_FORMAT);
    read_summary_time.set_halign(Align::Center);
    read_summary_time.add_css_class("read-summary-time");
    read_summary_time.set_tooltip_text(Some("Enter a time in your current timezone."));
    read_summary.append(&read_summary_time);

    let read_summary_location = gtk::Label::new(None);
    read_summary_location.set_xalign(0.5);
    read_summary_location.set_halign(Align::Center);
    read_summary_location.set_ellipsize(gtk::pango::EllipsizeMode::End);
    read_summary_location.set_single_line_mode(true);
    read_summary_location.set_max_width_chars(64);
    read_summary_location.set_margin_bottom(12);
    read_summary_location.add_css_class("read-summary-location");
    read_summary.append(&read_summary_location);

    let timeline_overlay = gtk::Overlay::new();
    timeline_overlay.add_css_class("timeline-shell");
    timeline_overlay.set_halign(Align::Center);
    timeline_overlay.set_margin_top(READ_TIMELINE_TOP_MARGIN);
    timeline_overlay.set_width_request(READ_TIMELINE_WIDTH);
    read_root.append(&timeline_overlay);

    let timeline_area = gtk::DrawingArea::new();
    timeline_area.set_content_width(READ_TIMELINE_WIDTH);
    timeline_area.set_width_request(READ_TIMELINE_WIDTH);
    timeline_area.set_content_height(READ_TIMELINE_HEIGHT);
    timeline_overlay.set_child(Some(&timeline_area));

    let timeline_labels = gtk::Fixed::new();
    timeline_labels.set_can_target(false);
    timeline_labels.set_width_request(READ_TIMELINE_WIDTH);
    timeline_labels.set_height_request(READ_TIMELINE_HEIGHT);
    timeline_overlay.add_overlay(&timeline_labels);
    timeline_overlay.set_measure_overlay(&timeline_labels, false);

    let cards_grid = gtk::Box::new(Orientation::Vertical, READ_CARD_SPACING);
    cards_grid.set_halign(Align::Center);
    cards_grid.set_width_request(READ_TIMELINE_WIDTH);
    cards_grid.add_css_class("timezone-card-grid");
    read_root.append(&cards_grid);

    // Legacy list-based edit UI is intentionally detached while edit mode
    // moves onto the read/card layout.
    let edit_root = gtk::Box::new(Orientation::Vertical, 14);

    let edit_controls = gtk::Box::new(Orientation::Horizontal, 12);
    edit_controls.set_halign(Align::Fill);
    edit_root.append(&edit_controls);

    let sort_mode_dropdown =
        gtk::DropDown::from_strings(&["Manual order", "Alphabetical", "By time"]);
    sort_mode_dropdown.add_css_class("popup-select");
    sort_mode_dropdown.set_halign(Align::Start);
    edit_controls.append(&sort_mode_dropdown);

    let time_format_dropdown = gtk::DropDown::from_strings(&["System", "24h", "AM/PM"]);
    time_format_dropdown.add_css_class("popup-select");
    time_format_dropdown.set_halign(Align::Start);
    edit_controls.append(&time_format_dropdown);

    let rows_overlay = gtk::Overlay::new();
    rows_overlay.set_margin_top(6);
    edit_root.append(&rows_overlay);

    let rows_box = gtk::Box::new(Orientation::Vertical, 10);
    rows_overlay.set_child(Some(&rows_box));

    let drag_layer = gtk::Fixed::new();
    drag_layer.set_hexpand(true);
    drag_layer.set_vexpand(true);
    drag_layer.set_can_target(false);
    rows_overlay.add_overlay(&drag_layer);
    rows_overlay.set_measure_overlay(&drag_layer, false);

    let insertion_marker = gtk::Box::new(Orientation::Horizontal, 0);
    insertion_marker.set_visible(false);
    insertion_marker.set_size_request(-1, 4);
    insertion_marker.set_hexpand(true);
    insertion_marker.set_halign(Align::Fill);
    insertion_marker.set_margin_top(2);
    insertion_marker.set_margin_bottom(2);
    insertion_marker.add_css_class("drag-insert-marker");

    let add_root = gtk::Box::new(Orientation::Vertical, 16);
    add_root.add_css_class("add-screen");
    content_stack.add_named(&add_root, Some("add"));

    let add_overlay = gtk::Overlay::new();
    add_overlay.set_halign(Align::Fill);
    add_overlay.set_valign(Align::Start);
    add_root.append(&add_overlay);

    let add_body = gtk::Box::new(Orientation::Vertical, 16);
    add_overlay.set_child(Some(&add_body));

    let add_entry = gtk::Entry::new();
    add_entry.set_hexpand(true);
    add_entry.add_css_class("search-entry");
    add_entry.add_css_class("add-search-entry");
    add_entry.set_placeholder_text(Some("Search for a city or timezone"));
    add_entry.set_icon_from_icon_name(
        gtk::EntryIconPosition::Primary,
        Some("system-search-symbolic"),
    );
    add_body.append(&add_entry);

    let search_results_scroller = gtk::ScrolledWindow::new();
    search_results_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    search_results_scroller.set_overlay_scrolling(true);
    search_results_scroller.set_propagate_natural_height(true);
    search_results_scroller.set_max_content_height(180);
    search_results_scroller.set_hexpand(true);
    search_results_scroller.set_halign(Align::Fill);
    search_results_scroller.set_valign(Align::Start);
    search_results_scroller.set_margin_top(72);
    search_results_scroller.add_css_class("search-results-overlay");
    search_results_scroller.set_visible(false);
    add_overlay.add_overlay(&search_results_scroller);
    add_overlay.set_measure_overlay(&search_results_scroller, false);

    let search_results_box = gtk::Box::new(Orientation::Vertical, 6);
    search_results_box.set_margin_top(6);
    search_results_box.set_margin_bottom(6);
    search_results_box.set_margin_start(6);
    search_results_box.set_margin_end(6);
    search_results_scroller.set_child(Some(&search_results_box));

    let add_map_shell = gtk::Overlay::new();
    add_map_shell.add_css_class("add-map-shell");
    add_map_shell.set_halign(Align::Center);
    add_map_shell.set_width_request(READ_TIMELINE_WIDTH);
    add_map_shell.set_size_request(READ_TIMELINE_WIDTH, ADD_MAP_HEIGHT);
    add_map_shell.set_overflow(gtk::Overflow::Hidden);
    add_body.append(&add_map_shell);

    let add_map_frame = gtk::AspectFrame::new(0.5, 0.5, ADD_MAP_ASPECT_RATIO, false);
    add_map_frame.set_halign(Align::Center);
    add_map_frame.set_valign(Align::Center);
    add_map_frame.set_size_request(READ_TIMELINE_WIDTH, ADD_MAP_HEIGHT);
    add_map_frame.set_overflow(gtk::Overflow::Hidden);
    add_map_shell.set_child(Some(&add_map_frame));

    let add_map_texture = load_world_map_texture();
    let add_map_picture = if let Some(texture) = add_map_texture.as_ref() {
        gtk::Picture::for_paintable(texture)
    } else {
        gtk::Picture::new()
    };
    add_map_picture.set_can_shrink(true);
    add_map_picture.set_content_fit(gtk::ContentFit::Contain);
    add_map_picture.set_width_request(READ_TIMELINE_WIDTH);
    add_map_picture.set_height_request(ADD_MAP_HEIGHT);
    add_map_picture.set_halign(Align::Center);
    add_map_picture.set_valign(Align::Center);
    add_map_picture.add_css_class("add-map-picture");
    add_map_frame.set_child(Some(&add_map_picture));

    let add_map_fallback = gtk::DrawingArea::new();
    add_map_fallback.set_content_width(READ_TIMELINE_WIDTH);
    add_map_fallback.set_width_request(READ_TIMELINE_WIDTH);
    add_map_fallback.set_content_height(ADD_MAP_HEIGHT);
    add_map_fallback.set_visible(add_map_texture.is_none());
    add_map_fallback.set_can_target(false);
    add_map_fallback.set_halign(Align::Center);
    add_map_fallback.set_valign(Align::Center);
    add_map_shell.add_overlay(&add_map_fallback);
    add_map_shell.set_measure_overlay(&add_map_fallback, false);

    let add_map_area = gtk::DrawingArea::new();
    add_map_area.set_content_width(READ_TIMELINE_WIDTH);
    add_map_area.set_width_request(READ_TIMELINE_WIDTH);
    add_map_area.set_content_height(ADD_MAP_HEIGHT);
    add_map_area.set_halign(Align::Center);
    add_map_area.set_valign(Align::Center);
    add_map_shell.add_overlay(&add_map_area);
    add_map_shell.set_measure_overlay(&add_map_area, false);

    let add_map_hover_layer = gtk::Fixed::new();
    add_map_hover_layer.set_can_target(false);
    add_map_hover_layer.set_size_request(READ_TIMELINE_WIDTH, ADD_MAP_HEIGHT);
    add_map_hover_layer.set_halign(Align::Center);
    add_map_hover_layer.set_valign(Align::Center);
    add_map_shell.add_overlay(&add_map_hover_layer);
    add_map_shell.set_measure_overlay(&add_map_hover_layer, false);

    let add_map_hover_card = gtk::Box::new(Orientation::Vertical, 8);
    add_map_hover_card.add_css_class("map-hover-card");
    add_map_hover_card.set_size_request(ADD_MAP_HOVER_CARD_WIDTH, ADD_MAP_HOVER_CARD_HEIGHT);
    add_map_hover_card.set_visible(false);

    let add_map_hover_title = gtk::Label::new(None);
    add_map_hover_title.set_xalign(0.0);
    add_map_hover_title.add_css_class("map-hover-title");
    add_map_hover_card.append(&add_map_hover_title);

    let add_map_hover_time = gtk::Label::new(None);
    add_map_hover_time.set_xalign(0.0);
    add_map_hover_time.add_css_class("map-hover-time");
    add_map_hover_card.append(&add_map_hover_time);

    let add_map_hover_meta = gtk::Label::new(None);
    add_map_hover_meta.set_xalign(0.0);
    add_map_hover_meta.add_css_class("map-hover-meta");
    add_map_hover_card.append(&add_map_hover_meta);

    let add_map_hover_relative = gtk::Label::new(None);
    add_map_hover_relative.set_xalign(0.0);
    add_map_hover_relative.add_css_class("map-hover-meta");
    add_map_hover_card.append(&add_map_hover_relative);

    add_map_hover_layer.put(&add_map_hover_card, 0.0, 0.0);

    let map_legend = gtk::Box::new(Orientation::Horizontal, 0);
    map_legend.add_css_class("map-legend");
    map_legend.set_halign(Align::Fill);
    map_legend.set_width_request(READ_TIMELINE_WIDTH);
    for label_text in MAP_LEGEND_LABELS {
        let label = gtk::Label::new(Some(label_text));
        label.set_hexpand(true);
        label.set_xalign(0.5);
        label.add_css_class("map-legend-label");
        map_legend.append(&label);
    }
    add_root.append(&map_legend);

    let hint = gtk::Label::new(Some(
        "Search by timezone, city, or abbreviation, or hover the map and click a region.",
    ));
    hint.set_xalign(0.0);
    hint.add_css_class("hint-label");
    add_root.append(&hint);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("status-label");
    status_label.set_visible(false);
    panel.append(&status_label);

    window.set_child(Some(&overlay));
    let (remote_search_sender, remote_search_receiver) = mpsc::channel::<RemoteSearchMessage>();

    let read_summary_dirty = Rc::new(Cell::new(false));
    let read_summary_suppress_changes = Rc::new(Cell::new(false));
    let initial_screen_mode = screen_mode_for_read_entry_count(
        PopupScreen::Read,
        read_entry_count(&config.timezones, &local_timezone),
    );

    let state = Rc::new(RefCell::new(PopupState {
        config_manager,
        config,
        resolver: TimezoneResolver::new(Some(all_timezones())),
        place_search: Arc::new(Mutex::new(RemotePlaceSearch::new(
            Some(all_timezones()),
            None,
        ))),
        remote_search_sender,
        local_timezone,
        time_format: String::new(),
        reference_utc: Utc::now(),
        rows_overlay,
        rows_box,
        row_separators: Vec::new(),
        drag_layer,
        insertion_marker,
        rows: Vec::new(),
        dismiss_armed: false,
        allow_close: false,
        live: true,
        screen_mode: initial_screen_mode,
        editing_timezone: None,
        pending_apply_source: None,
        pending_apply_timezone: None,
        content_stack: content_stack.clone(),
        panel_title: title.clone(),
        live_button: live_button.clone(),
        edit_button: edit_button.clone(),
        add_button: add_button.clone(),
        cancel_button: cancel_button.clone(),
        sort_mode_dropdown: sort_mode_dropdown.clone(),
        time_format_dropdown: time_format_dropdown.clone(),
        read_summary_time: read_summary_time.clone(),
        read_summary_location: read_summary_location.clone(),
        read_summary_dirty: read_summary_dirty.clone(),
        read_summary_suppress_changes: read_summary_suppress_changes.clone(),
        timeline_area: timeline_area.clone(),
        timeline_labels: timeline_labels.clone(),
        cards_grid: cards_grid.clone(),
        read_cards: Vec::new(),
        add_entry: add_entry.clone(),
        search_results_scroller: search_results_scroller.clone(),
        search_results_box: search_results_box.clone(),
        add_map_area: add_map_area.clone(),
        map_timezone_finder: TimezoneFinder::new(),
        add_map_hover_layer: add_map_hover_layer.clone(),
        add_map_hover_card: add_map_hover_card.clone(),
        add_map_hover_title: add_map_hover_title.clone(),
        add_map_hover_time: add_map_hover_time.clone(),
        add_map_hover_meta: add_map_hover_meta.clone(),
        add_map_hover_relative: add_map_hover_relative.clone(),
        hovered_map_result: None,
        local_search_results: Vec::new(),
        remote_search_results: Vec::new(),
        search_results: Vec::new(),
        search_generation: 0,
        drag_source_timezone: None,
        active_drop_index: None,
        drag_start_rows_box_y: 0.0,
        drag_start_row_top_y: 0.0,
        drag_row_overlay_x: 0.0,
        drag_ghost: None,
        status_label,
        self_handle: Weak::new(),
    }));
    state.borrow_mut().self_handle = Rc::downgrade(&state);
    bind_time_entry_events(
        &state,
        &read_summary_time,
        state.borrow().local_timezone.clone(),
        read_summary_dirty,
        read_summary_suppress_changes,
    );

    let state_for_timeline = state.clone();
    timeline_area.set_draw_func(move |_, context, width, height| {
        let state = state_for_timeline.borrow();
        let timeline_items = build_timeline_items(
            &state.config.timezones,
            &state.local_timezone,
            state.reference_utc,
            &state.time_format,
        );
        let anchor = zoned_datetime(state.reference_utc, &state.local_timezone);
        let side_hours = timeline_side_hours(&timeline_items);
        let stroke = gdk::RGBA::parse(&load_palette().foreground)
            .ok()
            .map(|rgba| {
                (
                    f64::from(rgba.red()),
                    f64::from(rgba.green()),
                    f64::from(rgba.blue()),
                )
            })
            .unwrap_or((0.6, 0.6, 0.6));

        let line_y = TIMELINE_LINE_Y.min(height as f64 - 12.0);
        let center_x = timeline_position_x(0.0, side_hours, width as f64);

        context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.16);
        context.set_line_width(1.0);
        context.move_to(TIMELINE_PADDING, line_y);
        context.line_to(width as f64 - TIMELINE_PADDING, line_y);
        let _ = context.stroke();

        for relative_minutes in timeline_tick_relative_minutes(&anchor, side_hours) {
            let x = timeline_position_x(relative_minutes, side_hours, width as f64);
            context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.12);
            context.move_to(x, line_y - 5.0);
            context.line_to(x, line_y + 5.0);
            let _ = context.stroke();
        }

        context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.22);
        context.move_to(center_x, line_y - 8.0);
        context.line_to(center_x, line_y + 8.0);
        let _ = context.stroke();

        for item in timeline_items {
            let x = timeline_position_x(item.relative_minutes as f64, side_hours, width as f64);
            context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.22);
            let radius = if item.entry_count > 1 { 6.5 } else { 5.5 };
            context.arc(x, line_y, radius, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        }
    });

    add_map_area.set_draw_func(move |_, context, width, height| {
        draw_add_map_overlay(context, width as f64, height as f64);
    });

    add_map_fallback.set_draw_func(move |_, context, width, height| {
        draw_add_map_fallback(context, width as f64, height as f64);
    });

    {
        let mut state_mut = state.borrow_mut();
        state_mut.time_format = effective_time_format(&state_mut.config.time_format);
        render_rows(&mut state_mut);
        update_live_button(&state_mut);
        update_screen_mode(&state_mut);
    }

    let state_for_remote_results = state.clone();
    let window_weak_for_remote_results = window.downgrade();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        if window_weak_for_remote_results.upgrade().is_none() {
            return ControlFlow::Break;
        }

        let mut should_render = false;
        while let Ok(message) = remote_search_receiver.try_recv() {
            let mut state = state_for_remote_results.borrow_mut();
            if message.generation != state.search_generation
                || state.add_entry.text().trim() != message.query
            {
                continue;
            }

            state.remote_search_results = message.results;
            state.search_results = merge_search_results(
                &state.local_search_results,
                &state.remote_search_results,
                ADD_SEARCH_RESULT_LIMIT,
            );
            should_render = true;
        }

        if should_render {
            render_search_results(&state_for_remote_results);
        }

        ControlFlow::Continue
    });

    let state_for_now = state.clone();
    live_button.connect_clicked(move |_| {
        reset_live_now(&state_for_now);
    });

    let state_for_add_screen = state.clone();
    add_button.connect_clicked(move |_| {
        {
            let state = state_for_add_screen.borrow();
            clear_status(&state);
        }
        set_screen_mode(&state_for_add_screen, PopupScreen::Add);
    });

    let state_for_edit = state.clone();
    edit_button.connect_clicked(move |_| {
        {
            let state = state_for_edit.borrow();
            clear_status(&state);
        }
        let next_mode = {
            let state = state_for_edit.borrow();
            if matches!(state.screen_mode, PopupScreen::Edit) {
                PopupScreen::Read
            } else {
                PopupScreen::Edit
            }
        };
        set_screen_mode(&state_for_edit, next_mode);
    });

    let state_for_sort_mode = state.clone();
    sort_mode_dropdown.connect_selected_notify(move |dropdown| {
        let sort_mode = SORT_MODE_VALUES
            .get(dropdown.selected() as usize)
            .copied()
            .unwrap_or(DEFAULT_SORT_MODE);
        let config_manager = {
            let state = state_for_sort_mode.borrow();
            if state.config.sort_mode == sort_mode {
                return;
            }
            state.config_manager.clone()
        };
        match config_manager.set_sort_mode(sort_mode) {
            Ok(config) => {
                let mut state = state_for_sort_mode.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            Err(error) => {
                let state = state_for_sort_mode.borrow();
                set_status(&state, &error.to_string(), true);
            }
        }
    });

    let state_for_time_format = state.clone();
    time_format_dropdown.connect_selected_notify(move |dropdown| {
        let time_format = TIME_FORMAT_VALUES
            .get(dropdown.selected() as usize)
            .copied()
            .unwrap_or(DEFAULT_TIME_FORMAT);
        let config_manager = {
            let state = state_for_time_format.borrow();
            if state.config.time_format == time_format {
                return;
            }
            state.config_manager.clone()
        };
        match config_manager.set_time_format(time_format) {
            Ok(config) => {
                let mut state = state_for_time_format.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            Err(error) => {
                let state = state_for_time_format.borrow();
                set_status(&state, &error.to_string(), true);
            }
        }
    });

    let state_for_cancel = state.clone();
    cancel_button.connect_clicked(move |_| {
        {
            let state = state_for_cancel.borrow();
            clear_status(&state);
        }
        set_screen_mode(&state_for_cancel, PopupScreen::Read);
    });

    let state_for_add_change = state.clone();
    add_entry.connect_changed(move |_| {
        update_search_results(&state_for_add_change);
    });

    let state_for_add_activate = state.clone();
    add_entry.connect_activate(move |_| {
        submit_add_timezone(&state_for_add_activate);
    });

    let map_motion = gtk::EventControllerMotion::new();
    let state_for_map_motion = state.clone();
    let add_map_area_for_motion = add_map_area.clone();
    map_motion.connect_motion(move |_, x, y| {
        let allocation = add_map_area_for_motion.allocation();
        let hovered_result = {
            let state = state_for_map_motion.borrow();
            map_hover_result_at_position(
                &state,
                f64::from(allocation.width()),
                f64::from(allocation.height()),
                x,
                y,
            )
        };
        set_map_hover_result(
            &state_for_map_motion,
            hovered_result,
            x,
            y,
            f64::from(allocation.width()),
            f64::from(allocation.height()),
        );
    });
    let state_for_map_leave = state.clone();
    let add_map_area_for_leave = add_map_area.clone();
    map_motion.connect_leave(move |_| {
        let allocation = add_map_area_for_leave.allocation();
        set_map_hover_result(
            &state_for_map_leave,
            None,
            0.0,
            0.0,
            f64::from(allocation.width()),
            f64::from(allocation.height()),
        );
    });
    add_map_area.add_controller(map_motion);

    let map_click = gtk::GestureClick::new();
    map_click.set_button(0);
    let state_for_map_click = state.clone();
    let add_map_area_for_click = add_map_area.clone();
    map_click.connect_pressed(move |_, _, x, y| {
        let allocation = add_map_area_for_click.allocation();
        let hovered_result = {
            let state = state_for_map_click.borrow();
            map_hover_result_at_position(
                &state,
                f64::from(allocation.width()),
                f64::from(allocation.height()),
                x,
                y,
            )
        };
        if let Some(result) = hovered_result {
            add_timezone(&state_for_map_click, &result.timezone, &result.title);
        }
    });
    add_map_area.add_controller(map_click);

    let state_for_click = state.clone();
    let window_for_click = window.clone();
    let overlay_for_click = overlay.clone();
    let panel_for_click = panel.clone().upcast::<gtk::Widget>();
    let dismiss_click = gtk::GestureClick::new();
    dismiss_click.set_button(0);
    dismiss_click.connect_pressed(move |_, _, x, y| {
        let state = state_for_click.borrow();
        debug_popup_event(&format!(
            "dismiss_click pressed x={x:.1} y={y:.1} dismiss_armed={} screen={:?}",
            state.dismiss_armed, state.screen_mode
        ));
        if !state.dismiss_armed || !is_dismissible_screen(&state) {
            return;
        }
        drop(state);

        if let Some(picked) = overlay_for_click.pick(x, y, gtk::PickFlags::DEFAULT) {
            if picked == panel_for_click || picked.is_ancestor(&panel_for_click) {
                debug_popup_event("dismiss_click inside_panel");
                return;
            }
        }

        request_window_close(&state_for_click, &window_for_click, "dismiss_click");
    });
    overlay.add_controller(dismiss_click);

    state
}

pub fn run_popup(pid_path: &Path, config_path: Option<PathBuf>) -> Result<()> {
    gtk::init()?;
    apply_css()?;

    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime directory {}", parent.display()))?;
    }
    fs::write(pid_path, std::process::id().to_string())
        .with_context(|| format!("failed to write {}", pid_path.display()))?;
    let _pid_guard = PidGuard {
        path: pid_path.to_path_buf(),
    };

    let config_manager = ConfigManager::new(config_path);
    let config = config_manager.load()?;
    let local_timezone = detect_local_timezone();

    let window = gtk::Window::new();
    window.set_title(Some("Omarchy World Clock"));
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_focusable(true);
    window.set_can_focus(true);
    let state = build_window(config_manager, config, local_timezone, &window);
    let _ = configure_layer_shell(&window);

    let key_controller = gtk::EventControllerKey::new();
    let state_for_escape = state.clone();
    let window_for_escape = window.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            let should_close = {
                let state = state_for_escape.borrow();
                is_dismissible_screen(&state)
            };
            if should_close {
                request_window_close(&state_for_escape, &window_for_escape, "escape");
            } else {
                {
                    let state = state_for_escape.borrow();
                    clear_status(&state);
                }
                set_screen_mode(&state_for_escape, PopupScreen::Read);
            }
            return Propagation::Stop;
        }
        Propagation::Proceed
    });
    window.add_controller(key_controller);

    let state_for_focus = state.clone();
    window.connect_is_active_notify(move |window| {
        let state = state_for_focus.borrow();
        debug_popup_event(&format!(
            "is_active_notify active={} dismiss_armed={} screen={:?}",
            window.is_active(),
            state.dismiss_armed,
            state.screen_mode
        ));
        if state.dismiss_armed && is_dismissible_screen(&state) && !window.is_active() {
            drop(state);
            request_window_close(&state_for_focus, window, "focus_lost_read_mode");
        }
    });

    let main_loop = MainLoop::new(None, false);
    let state_for_close = state.clone();
    let main_loop_for_close = main_loop.clone();
    window.connect_close_request(move |_| {
        debug_popup_event(&format!(
            "close_request allow_close={}",
            state_for_close.borrow().allow_close
        ));
        if !state_for_close.borrow().allow_close {
            return Propagation::Stop;
        }
        main_loop_for_close.quit();
        Propagation::Proceed
    });

    let state_for_arm = state.clone();
    glib::timeout_add_local_once(Duration::from_millis(200), move || {
        state_for_arm.borrow_mut().dismiss_armed = true;
    });

    let state_for_tick = state.clone();
    let window_weak = window.downgrade();
    glib::timeout_add_local(Duration::from_secs(1), move || {
        let Some(window) = window_weak.upgrade() else {
            return ControlFlow::Break;
        };
        if !window.is_visible() {
            return ControlFlow::Break;
        }

        let mut state = state_for_tick.borrow_mut();
        if state.live {
            state.reference_utc = Utc::now();
            update_row_widgets(&mut state);
        }
        ControlFlow::Continue
    });

    window.present();
    main_loop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_timeline_items, first_location_segment, format_timeline_zone_text,
        map_coordinates_to_lng_lat, read_card_row_width, read_card_title, read_entry_count,
        screen_mode_for_read_entry_count, search_result_subtitle, sort_read_entries_by_time,
        summary_search_result, timeline_entries, timeline_side_hours,
        timeline_tick_relative_minutes, visible_read_entries, PopupScreen, READ_CARD_COLUMNS,
        READ_CARD_LIMIT, READ_CARD_SPACING, READ_CARD_WIDTH,
    };
    use crate::config::{TimezoneEntry, TimezoneSearchResult};
    use crate::time::zoned_datetime;
    use chrono::{TimeZone, Utc};

    fn entry(timezone: &str) -> TimezoneEntry {
        TimezoneEntry {
            timezone: timezone.to_string(),
            label: String::new(),
            locked: false,
        }
    }

    #[test]
    fn visible_read_entries_skips_local_and_caps_to_limit() {
        let entries = vec![
            entry("America/Cancun"),
            entry("Europe/Paris"),
            entry("Asia/Tokyo"),
            entry("Europe/Lisbon"),
            entry("America/Los_Angeles"),
            entry("America/New_York"),
            entry("Asia/Kolkata"),
            entry("Australia/Sydney"),
            entry("Europe/Berlin"),
            entry("Europe/London"),
            entry("Asia/Singapore"),
        ];

        let visible = visible_read_entries(&entries, "America/Cancun");

        assert_eq!(visible.len(), READ_CARD_LIMIT);
        assert!(visible
            .iter()
            .all(|entry| entry.timezone != "America/Cancun"));
        assert_eq!(
            visible.first().map(|entry| entry.timezone.as_str()),
            Some("Europe/Paris")
        );
        assert_eq!(
            visible.last().map(|entry| entry.timezone.as_str()),
            Some("Europe/London")
        );
    }

    #[test]
    fn visible_read_entries_does_not_fall_back_to_local_only_entry() {
        let entries = vec![entry("America/Cancun")];

        let visible = visible_read_entries(&entries, "America/Cancun");

        assert!(visible.is_empty());
    }

    #[test]
    fn read_entry_count_excludes_local_when_other_cards_exist() {
        let entries = vec![
            entry("America/Cancun"),
            entry("Europe/Paris"),
            entry("Asia/Tokyo"),
        ];

        assert_eq!(read_entry_count(&entries, "America/Cancun"), 2);
    }

    #[test]
    fn read_entry_count_ignores_local_when_it_is_the_only_entry() {
        let entries = vec![entry("America/Cancun")];

        assert_eq!(read_entry_count(&entries, "America/Cancun"), 0);
    }

    #[test]
    fn screen_mode_for_read_entry_count_uses_add_when_empty() {
        assert_eq!(
            screen_mode_for_read_entry_count(PopupScreen::Read, 0),
            PopupScreen::Add
        );
        assert_eq!(
            screen_mode_for_read_entry_count(PopupScreen::Edit, 0),
            PopupScreen::Add
        );
    }

    #[test]
    fn screen_mode_for_read_entry_count_preserves_requested_mode_with_locations() {
        assert_eq!(
            screen_mode_for_read_entry_count(PopupScreen::Read, 1),
            PopupScreen::Read
        );
        assert_eq!(
            screen_mode_for_read_entry_count(PopupScreen::Edit, 1),
            PopupScreen::Edit
        );
    }

    #[test]
    fn read_card_row_width_centers_incomplete_rows() {
        assert_eq!(read_card_row_width(1), READ_CARD_WIDTH);
        assert_eq!(
            read_card_row_width(2),
            READ_CARD_WIDTH * 2 + READ_CARD_SPACING
        );
        assert_eq!(
            read_card_row_width(READ_CARD_COLUMNS as usize),
            READ_CARD_WIDTH * READ_CARD_COLUMNS + READ_CARD_SPACING * (READ_CARD_COLUMNS - 1)
        );
        assert_eq!(
            read_card_row_width((READ_CARD_COLUMNS + 1) as usize),
            read_card_row_width(READ_CARD_COLUMNS as usize)
        );
    }

    #[test]
    fn sort_read_entries_orders_cards_by_local_time() {
        let mut entries = vec![
            entry("Europe/Paris"),
            entry("America/Los_Angeles"),
            entry("Asia/Kolkata"),
            entry("America/Chicago"),
        ];

        sort_read_entries_by_time(
            &mut entries,
            Utc.with_ymd_and_hms(2026, 4, 18, 5, 5, 0).unwrap(),
            "America/Cancun",
        );

        let ordered = entries
            .iter()
            .map(|entry| entry.timezone.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            vec![
                "America/Los_Angeles",
                "America/Chicago",
                "Europe/Paris",
                "Asia/Kolkata",
            ]
        );
    }

    #[test]
    fn timeline_entries_always_include_local_anchor() {
        let entries = vec![entry("America/Chicago"), entry("Europe/Paris")];

        let timeline = timeline_entries(
            &entries,
            "America/Cancun",
            Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap(),
        );

        assert!(timeline
            .iter()
            .any(|entry| entry.timezone == "America/Cancun"));
    }

    #[test]
    fn timeline_groups_same_slot_abbreviations_with_anchor_first() {
        let entries = vec![entry("America/Chicago")];

        let items = build_timeline_items(
            &entries,
            "America/Cancun",
            Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap(),
            "24h",
        );

        let center_item = items
            .iter()
            .find(|item| item.relative_minutes == 0)
            .expect("center timeline item should exist");
        assert_eq!(center_item.zone_text, "EST / CDT");
        assert_eq!(center_item.entry_count, 2);
    }

    #[test]
    fn timeline_zone_text_compacts_three_or_more_entries() {
        let labels = vec!["EST".to_string(), "CDT".to_string(), "COT".to_string()];

        assert_eq!(format_timeline_zone_text(&labels, 3), "EST / CDT +1");
        assert_eq!(format_timeline_zone_text(&labels[..1], 3), "EST +2");
    }

    #[test]
    fn timeline_ticks_follow_whole_hour_boundaries() {
        let reference_utc = Utc.with_ymd_and_hms(2026, 4, 18, 20, 29, 0).unwrap();
        let anchor = zoned_datetime(reference_utc, "America/Cancun");

        let ticks = timeline_tick_relative_minutes(&anchor, 12);

        assert!(ticks
            .windows(2)
            .all(|pair| (pair[1] - pair[0] - 60.0).abs() < 0.001));
        assert!(ticks.iter().any(|tick| (*tick + 29.0).abs() < 0.001));
        assert!(ticks.iter().any(|tick| (*tick - 31.0).abs() < 0.001));
    }

    #[test]
    fn timeline_side_hours_keeps_an_extra_hour_beyond_farthest_offset() {
        let items = build_timeline_items(
            &[entry("Asia/Kolkata")],
            "America/Cancun",
            Utc.with_ymd_and_hms(2026, 4, 18, 5, 5, 0).unwrap(),
            "24h",
        );

        let side_hours = timeline_side_hours(&items);

        assert_eq!(side_hours, 12);
    }

    #[test]
    fn read_card_title_prefers_the_first_location_segment() {
        let entry = TimezoneEntry {
            timezone: "America/Vancouver".to_string(),
            label: "Vancouver Island, British Columbia, Canada".to_string(),
            locked: false,
        };

        assert_eq!(read_card_title(&entry), "Vancouver Island");
    }

    #[test]
    fn read_card_title_falls_back_to_the_friendly_timezone_name() {
        let entry = entry("America/Argentina/Buenos_Aires");

        assert_eq!(read_card_title(&entry), "Buenos Aires");
    }

    #[test]
    fn first_location_segment_uses_the_city_from_a_place_label() {
        assert_eq!(
            first_location_segment("Barcelona, Catalonia, Spain"),
            "Barcelona"
        );
    }

    #[test]
    fn summary_search_result_uses_label_context_for_metadata() {
        let result = summary_search_result(
            "Europe/Madrid",
            "Barcelona, Catalonia, Spain",
            Some(TimezoneSearchResult {
                timezone: "Europe/Madrid".to_string(),
                title: "Madrid".to_string(),
                subtitle: "Europe/Madrid  ·  CET / CEST".to_string(),
            }),
        );

        let subtitle = search_result_subtitle(
            &result,
            &Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap(),
        );

        assert_eq!(subtitle, "Europe/Madrid  ·  CEST  ·  Catalonia, Spain");
    }

    #[test]
    fn search_result_subtitle_inserts_current_abbreviation_after_timezone() {
        let result = TimezoneSearchResult {
            timezone: "Europe/Paris".to_string(),
            title: "Barc, Normandy, France".to_string(),
            subtitle: "Europe/Paris  ·  Normandy, France".to_string(),
        };

        let subtitle = search_result_subtitle(
            &result,
            &Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap(),
        );

        assert_eq!(subtitle, "Europe/Paris  ·  CEST  ·  Normandy, France");
    }

    #[test]
    fn search_result_subtitle_replaces_broad_abbreviation_list() {
        let result = TimezoneSearchResult {
            timezone: "Europe/Paris".to_string(),
            title: "Paris".to_string(),
            subtitle: "Europe/Paris  ·  CET / CEST".to_string(),
        };

        let subtitle = search_result_subtitle(
            &result,
            &Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap(),
        );

        assert_eq!(subtitle, "Europe/Paris  ·  CEST");
    }

    #[test]
    fn map_coordinates_cover_the_expected_world_extent() {
        let center = map_coordinates_to_lng_lat(900.0, 450.0, 450.0, 225.0);
        assert_eq!(center, Some((0.0, 0.0)));

        let top_left = map_coordinates_to_lng_lat(900.0, 450.0, 0.0, 0.0).unwrap();
        assert!(top_left.0 < -179.9);
        assert!(top_left.1 > 89.9);

        let bottom_right = map_coordinates_to_lng_lat(900.0, 450.0, 900.0, 450.0).unwrap();
        assert!(bottom_right.0 > 179.9);
        assert!(bottom_right.1 < -89.9);
    }
}
