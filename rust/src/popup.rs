use crate::config::{
    all_timezones, detect_local_timezone, effective_time_format, AppConfig, ConfigManager,
    RemotePlaceSearch, TimezoneEntry, TimezoneResolver, TimezoneSearchResult,
};
use crate::layout::{
    load_window_border_size, load_window_gap, popup_top_margin, POPUP_TOP_CONTENT_MARGIN,
};
use crate::theme::{build_css, load_palette};
use crate::time::{
    format_display_time, format_offset, friendly_timezone_name, parse_manual_reference_details,
    row_metadata, zoned_datetime,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Offset, Utc};
use gtk::gdk;
use gtk::glib::{self, ControlFlow, MainLoop, Propagation};
use gtk::prelude::*;
use gtk::{Align, Orientation, SelectionMode};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
struct RowWidgets {
    entry: TimezoneEntry,
    root: gtk::Box,
    drag_handle: gtk::Box,
    title: gtk::Label,
    context: gtk::Label,
    meta: gtk::Label,
    remove_button: gtk::Button,
    time_entry: gtk::Entry,
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
    edit_mode: bool,
    add_panel_visible: bool,
    editing_timezone: Option<String>,
    syncing_controls: bool,
    pending_apply_source: Option<glib::SourceId>,
    pending_apply_timezone: Option<String>,
    content_stack: gtk::Stack,
    panel_title: gtk::Label,
    format_row: gtk::Box,
    live_button: gtk::Button,
    edit_button: gtk::Button,
    time_format_combo: gtk::DropDown,
    read_summary_time: gtk::Label,
    read_summary_location: gtk::Label,
    timeline_area: gtk::DrawingArea,
    timeline_labels: gtk::Fixed,
    cards_flow: gtk::FlowBox,
    footer_separator: gtk::Separator,
    add_stack: gtk::Stack,
    add_toggle_button: gtk::Button,
    add_panel: gtk::Box,
    add_entry: gtk::Entry,
    search_results_scroller: gtk::ScrolledWindow,
    search_results_box: gtk::Box,
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

const TIME_FORMAT_OPTIONS: [(&str, &str); 3] =
    [("system", "System"), ("24h", "24h"), ("ampm", "AM/PM")];
const READ_TIMELINE_WIDTH: i32 = 700;
const READ_CARD_COLUMNS: i32 = 3;
const READ_CARD_LIMIT: usize = 9;
const READ_CARD_SPACING: i32 = 18;
const READ_CARD_WIDTH: i32 =
    (READ_TIMELINE_WIDTH - (READ_CARD_SPACING * (READ_CARD_COLUMNS - 1))) / READ_CARD_COLUMNS;

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

fn dropdown_selection_index(options: &[(&str, &str)], value: &str) -> u32 {
    options.iter().position(|(id, _)| *id == value).unwrap_or(0) as u32
}

fn dropdown_selection_value(options: &[(&str, &str)], index: u32, fallback: &str) -> String {
    options
        .get(index as usize)
        .map(|(id, _)| (*id).to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn update_dropdown_list_item(list_item: &gtk::ListItem) {
    let Some(container) = list_item
        .child()
        .and_then(|child| child.downcast::<gtk::Box>().ok())
    else {
        return;
    };
    let Some(label) = container
        .first_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    else {
        return;
    };
    let Some(checkmark) = container
        .last_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    else {
        return;
    };

    if let Some(item) = list_item
        .item()
        .and_then(|item| item.downcast::<gtk::StringObject>().ok())
    {
        label.set_text(item.string().as_str());
    } else {
        label.set_text("");
    }

    checkmark.set_visible(list_item.is_selected());
}

fn build_dropdown(options: &[(&str, &str)], active_value: &str) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(|(_, label)| *label).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.add_css_class("popup-select");
    dropdown.set_selected(dropdown_selection_index(options, active_value));

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };

        let row = gtk::Box::new(Orientation::Horizontal, 12);
        row.set_hexpand(true);
        row.set_halign(Align::Fill);
        row.add_css_class("popup-select-row");

        let label = gtk::Label::new(None);
        label.set_hexpand(true);
        label.set_xalign(0.0);
        label.add_css_class("popup-select-item-label");
        row.append(&label);

        let checkmark = gtk::Label::new(Some("✓"));
        checkmark.set_halign(Align::End);
        checkmark.set_xalign(1.0);
        checkmark.add_css_class("popup-select-item-check");
        row.append(&checkmark);

        list_item.set_child(Some(&row));

        let list_item = list_item.clone();
        list_item.connect_selected_notify(|item| {
            update_dropdown_list_item(item);
        });
    });
    factory.connect_bind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        update_dropdown_list_item(list_item);
    });
    dropdown.set_list_factory(Some(&factory));

    dropdown
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

fn sanitize_popup_config(mut config: AppConfig) -> AppConfig {
    config.sort_mode = "manual".to_string();
    for entry in &mut config.timezones {
        entry.locked = false;
    }
    config
}

fn selected_entries(state: &PopupState) -> Vec<TimezoneEntry> {
    state.config.timezones.clone()
}

fn visible_read_entries(entries: &[TimezoneEntry], local_timezone: &str) -> Vec<TimezoneEntry> {
    let mut visible = entries
        .iter()
        .filter(|entry| entry.timezone != local_timezone)
        .take(READ_CARD_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    if visible.is_empty() {
        visible = entries.iter().take(READ_CARD_LIMIT).cloned().collect();
    }
    visible
}

fn read_entries(state: &PopupState) -> Vec<TimezoneEntry> {
    visible_read_entries(&state.config.timezones, &state.local_timezone)
}

fn row_can_reorder(state: &PopupState, _entry: &TimezoneEntry) -> bool {
    state.config.timezones.len() > 1
}

fn set_row_error(row: &RowWidgets, enabled: bool) {
    if enabled {
        row.time_entry.add_css_class("error");
    } else {
        row.time_entry.remove_css_class("error");
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

fn timeline_extent_minutes(
    anchor: &DateTime<chrono_tz::Tz>,
    entries: &[TimezoneEntry],
    reference_utc: DateTime<Utc>,
) -> i64 {
    entries
        .iter()
        .map(|entry| {
            let zoned = zoned_datetime(reference_utc, &entry.timezone);
            timeline_relative_minutes(anchor, &zoned).abs()
        })
        .max()
        .unwrap_or(60)
        .max(60)
}

fn render_read_view(state: &mut PopupState) {
    let anchor = zoned_datetime(state.reference_utc, &state.local_timezone);
    state
        .read_summary_time
        .set_text(&format_display_time(&anchor, &state.time_format));
    state.read_summary_location.set_text(&format!(
        "{}  ·  {}",
        anchor_label(state),
        format_offset(anchor.offset().fix().local_minus_utc())
    ));

    while let Some(child) = state.timeline_labels.first_child() {
        state.timeline_labels.remove(&child);
    }

    let entries = read_entries(state);
    let extent_minutes = timeline_extent_minutes(&anchor, &entries, state.reference_utc) as f64;
    let mut timeline_items = entries
        .iter()
        .map(|entry| {
            let zoned = zoned_datetime(state.reference_utc, &entry.timezone);
            let relative_minutes = timeline_relative_minutes(&anchor, &zoned) as f64;
            (
                relative_minutes,
                format_display_time(&zoned, &state.time_format),
                zoned.format("%Z").to_string(),
            )
        })
        .collect::<Vec<_>>();
    timeline_items.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let timeline_width = f64::from(READ_TIMELINE_WIDTH);
    let label_width = 92.0;
    let padding = 28.0;
    let usable_width = (timeline_width - padding * 2.0).max(1.0);
    for (relative_minutes, time_text, abbreviation) in &timeline_items {
        let x = padding
            + (((*relative_minutes + extent_minutes) / (extent_minutes * 2.0)) * usable_width);
        let item = gtk::Box::new(Orientation::Vertical, 6);
        item.set_size_request(label_width as i32, -1);

        let time_label = gtk::Label::new(Some(time_text));
        time_label.set_xalign(0.5);
        time_label.add_css_class("timeline-time");
        item.append(&time_label);

        let abbreviation_label = gtk::Label::new(Some(abbreviation));
        abbreviation_label.set_xalign(0.5);
        abbreviation_label.add_css_class("timeline-zone");
        item.append(&abbreviation_label);

        state.timeline_labels.put(
            &item,
            (x - label_width / 2.0).clamp(0.0, timeline_width - label_width),
            0.0,
        );
    }
    state.timeline_area.queue_draw();

    while let Some(child) = state.cards_flow.first_child() {
        state.cards_flow.remove(&child);
    }
    for entry in entries {
        let zoned = zoned_datetime(state.reference_utc, &entry.timezone);

        let card = gtk::Box::new(Orientation::Vertical, 16);
        card.add_css_class("timezone-card");
        card.set_size_request(READ_CARD_WIDTH, -1);

        let title = gtk::Label::new(Some(&entry.display_label()));
        title.set_xalign(0.0);
        title.add_css_class("timezone-card-title");
        card.append(&title);

        let time_label = gtk::Label::new(Some(&format_display_time(&zoned, &state.time_format)));
        time_label.set_xalign(0.0);
        time_label.add_css_class("timezone-card-time");
        card.append(&time_label);

        let footer = gtk::Box::new(Orientation::Horizontal, 16);
        footer.set_halign(Align::Fill);

        let timezone_label = gtk::Label::new(Some(&entry.timezone));
        timezone_label.set_xalign(0.0);
        timezone_label.set_hexpand(true);
        timezone_label.add_css_class("timezone-card-meta");
        footer.append(&timezone_label);

        let delta_label = gtk::Label::new(Some(&relative_time_label(&anchor, &zoned)));
        delta_label.set_xalign(1.0);
        delta_label.set_halign(Align::End);
        delta_label.add_css_class("timezone-card-meta");
        footer.append(&delta_label);

        card.append(&footer);
        state.cards_flow.append(&card);
    }
}

fn update_edit_mode(state: &PopupState) {
    state
        .content_stack
        .set_visible_child_name(if state.edit_mode { "edit" } else { "read" });
    state.panel_title.set_visible(state.edit_mode);
    state.format_row.set_visible(state.edit_mode);
    if state.edit_mode {
        state.edit_button.add_css_class("active");
        state.edit_button.set_tooltip_text(Some("Leave edit mode."));
        state.footer_separator.set_visible(true);
        state.add_stack.set_visible(true);
        if state.add_panel_visible {
            state.add_stack.set_visible_child_name("panel");
            state.add_panel.set_visible(true);
            if state.add_entry.text().trim().is_empty() {
                state.search_results_scroller.set_visible(false);
            }
        } else {
            state.add_stack.set_visible_child_name("toggle");
            state.add_toggle_button.set_visible(true);
        }
    } else {
        state.edit_button.remove_css_class("active");
        state
            .edit_button
            .set_tooltip_text(Some("Manage timezones and popup settings."));
        state.footer_separator.set_visible(false);
        state.add_stack.set_visible(false);
    }

    let can_remove = state.config.timezones.len() > 1;
    for row in &state.rows {
        row.drag_handle
            .set_visible(state.edit_mode && row_can_reorder(state, &row.entry));
        row.remove_button.set_visible(state.edit_mode);
        row.remove_button.set_sensitive(can_remove);
    }
    update_row_separators(state);
}

fn update_row_separators(state: &PopupState) {
    let show_separators = !state.edit_mode;
    for separator in &state.row_separators {
        separator.set_visible(show_separators);
    }
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

fn set_add_panel_visible(state_handle: &Rc<RefCell<PopupState>>, visible: bool) {
    debug_popup_event(&format!("set_add_panel_visible visible={visible}"));
    let (focus_entry, entry_to_clear) = {
        let mut state = state_handle.borrow_mut();
        state.add_panel_visible = visible;
        if visible {
            state.add_stack.set_visible_child_name("panel");
            state.add_panel.set_visible(true);
            if state.add_entry.text().trim().is_empty() {
                state.search_results_scroller.set_visible(false);
            }
            (Some(state.add_entry.clone()), None)
        } else {
            clear_search_results(&mut state);
            state.add_stack.set_visible_child_name("toggle");
            (None, Some(state.add_entry.clone()))
        }
    };

    if let Some(entry) = entry_to_clear {
        entry.set_text("");
    }

    if let Some(entry) = focus_entry {
        glib::idle_add_local_once(move || {
            let _ = entry.grab_focus();
        });
    }
}

fn focus_add_toggle_button(state_handle: &Rc<RefCell<PopupState>>) {
    let button = state_handle.borrow().add_toggle_button.clone();
    glib::idle_add_local_once(move || {
        let _ = button.grab_focus();
    });
}

fn sync_config_controls(state: &mut PopupState) {
    state.syncing_controls = true;
    state
        .time_format_combo
        .set_selected(dropdown_selection_index(
            &TIME_FORMAT_OPTIONS,
            &state.config.time_format,
        ));
    state.syncing_controls = false;
}

fn refresh_config_state(state: &mut PopupState, config: AppConfig) {
    cancel_pending_apply(state);
    state.config = sanitize_popup_config(config);
    state.time_format = effective_time_format(&state.config.time_format);
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
    sync_config_controls(state);
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
        remove_button,
        time_entry,
        dirty: Rc::new(Cell::new(false)),
        suppress_changes: Rc::new(Cell::new(false)),
    }
}

fn bind_row_events(state_handle: &Rc<RefCell<PopupState>>, row: &RowWidgets) {
    let timezone_name = row.entry.timezone.clone();
    let dirty = row.dirty.clone();
    let suppress_changes = row.suppress_changes.clone();
    let state_for_change = state_handle.clone();
    let timezone_name_for_change = timezone_name.clone();
    row.time_entry.connect_changed(move |time_entry| {
        if suppress_changes.get() {
            return;
        }

        dirty.set(true);
        time_entry.remove_css_class("error");
        if let Ok(state) = state_for_change.try_borrow() {
            clear_status(&state);
        }
        schedule_live_apply(&state_for_change, &timezone_name_for_change);
    });

    let focus_controller = gtk::EventControllerFocus::new();
    let timezone_name_for_enter = timezone_name.clone();
    let dirty_for_enter = row.dirty.clone();
    let state_for_enter = state_handle.clone();
    let time_entry_for_enter = row.time_entry.clone();
    focus_controller.connect_enter(move |_| {
        dirty_for_enter.set(false);
        time_entry_for_enter.remove_css_class("error");
        time_entry_for_enter.select_region(0, -1);
        let mut state = state_for_enter.borrow_mut();
        state.editing_timezone = Some(timezone_name_for_enter.clone());
        clear_status(&state);
    });

    let timezone_name_for_leave = timezone_name.clone();
    let dirty_for_leave = row.dirty.clone();
    let state_for_leave = state_handle.clone();
    focus_controller.connect_leave(move |_| {
        {
            let mut state = state_for_leave.borrow_mut();
            if state.editing_timezone.as_deref() == Some(timezone_name_for_leave.as_str()) {
                state.editing_timezone = None;
            }
        }

        if dirty_for_leave.get() {
            let applied = flush_live_apply(&state_for_leave, &timezone_name_for_leave, false);
            if !applied {
                dirty_for_leave.set(false);
                let mut state = state_for_leave.borrow_mut();
                update_row_widgets(&mut state);
            }
        } else {
            let mut state = state_for_leave.borrow_mut();
            update_row_widgets(&mut state);
        }
    });
    row.time_entry.add_controller(focus_controller);

    let state_for_activate = state_handle.clone();
    row.time_entry.connect_activate(move |_| {
        let _ = flush_live_apply(&state_for_activate, &timezone_name, true);
    });

    let timezone_name_for_remove = row.entry.timezone.clone();
    let state_for_remove = state_handle.clone();
    row.remove_button.connect_clicked(move |_| {
        let config_manager = {
            let state = state_for_remove.borrow();
            if state.config.timezones.len() <= 1 {
                set_status(&state, "Keep at least one timezone in the popup.", true);
                return;
            }
            state.config_manager.clone()
        };

        match config_manager.remove_timezone(&timezone_name_for_remove) {
            Ok(config) => {
                let mut state = state_for_remove.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            Err(error) => {
                let state = state_for_remove.borrow();
                set_status(&state, &error.to_string(), true);
            }
        }
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

fn update_row_widgets(state: &mut PopupState) {
    let ordered = selected_entries(state);
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
}

fn render_rows(state: &mut PopupState) {
    clear_drop_slot(state);
    clear_box(&state.rows_box);
    state.rows.clear();
    state.row_separators.clear();

    let entries = selected_entries(state);
    if entries.is_empty() {
        let empty = gtk::Box::new(Orientation::Vertical, 4);
        empty.set_halign(Align::Start);

        let title = gtk::Label::new(Some("No timezones yet"));
        title.set_xalign(0.0);
        title.add_css_class("empty-state-title");
        empty.append(&title);

        let copy = gtk::Label::new(Some("Use edit mode to add or restore a timezone."));
        copy.set_xalign(0.0);
        copy.add_css_class("empty-state-copy");
        empty.append(&copy);

        state.rows_box.append(&empty);
        update_edit_mode(state);
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
    update_edit_mode(state);
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

        let meta = gtk::Label::new(Some(&result.subtitle));
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

        state.local_search_results = state.resolver.search(&query, 8);
        state.search_results =
            merge_search_results(&state.local_search_results, &state.remote_search_results, 8);

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
                .map(|mut search| search.search(&query, 8))
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
    }

    let config_manager = state_handle.borrow().config_manager.clone();
    match config_manager.add_timezone(timezone_name, label) {
        Ok(config) => {
            debug_popup_event(&format!(
                "add_timezone success timezone={timezone_name} label={display_name}"
            ));
            set_add_panel_visible(state_handle, false);
            let mut state = state_handle.borrow_mut();
            refresh_config_state(&mut state, config);
            set_status(&state, &format!("Added {display_name}."), false);
            drop(state);
            focus_add_toggle_button(state_handle);
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
    let entries = selected_entries(&state_handle.borrow());
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
    if !state.edit_mode || !row_can_reorder(&state, &state.rows[row_index].entry) {
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

fn can_drop_at_index(state: &PopupState, insert_index: usize) -> bool {
    let Some(source_timezone) = state.drag_source_timezone.as_deref() else {
        return false;
    };
    let entries = selected_entries(state);
    let Some(source_index) = entries
        .iter()
        .position(|entry| entry.timezone == source_timezone)
    else {
        return false;
    };
    let effective_index = if source_index < insert_index {
        insert_index.saturating_sub(1)
    } else {
        insert_index
    };
    effective_index != source_index
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
    window.set_namespace(Some("omarchy-world-clock-rs"));
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
    for row in &state.rows {
        row.dirty.set(false);
        set_row_error(row, false);
    }
    clear_status(&state);
    update_live_button(&state);
    update_row_widgets(&mut state);
}

fn apply_manual_entry(
    state_handle: &Rc<RefCell<PopupState>>,
    timezone_name: &str,
    show_errors: bool,
) -> bool {
    let (raw_value, dirty, row_index) = {
        let state = state_handle.borrow();
        let Some((index, row)) = state
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.entry.timezone == timezone_name)
        else {
            return false;
        };
        (row.time_entry.text().to_string(), row.dirty.get(), index)
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
                    if let Some(row) = state.rows.get(row_index) {
                        set_row_error(row, true);
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
        if let Some(active_row) = state.rows.get(row_index) {
            let rendered = format_display_time(
                &zoned_datetime(parsed_reference.reference_utc, timezone_name),
                &state.time_format,
            );
            active_row.suppress_changes.set(true);
            active_row.time_entry.set_text(&rendered);
            active_row.time_entry.set_position(-1);
            active_row.suppress_changes.set(false);
        }
    }

    for row in &state.rows {
        row.dirty.set(false);
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
    panel.set_width_request(760);
    panel.set_halign(Align::Center);
    top_band.append(&panel);

    let header = gtk::Box::new(Orientation::Horizontal, 12);
    panel.append(&header);

    let title = gtk::Label::new(Some("World Clock"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("panel-title");
    header.append(&title);

    let header_actions = gtk::Box::new(Orientation::Horizontal, 8);
    header.append(&header_actions);

    let live_button = gtk::Button::from_icon_name("view-refresh-symbolic");
    live_button.add_css_class("icon-button");
    live_button.set_valign(Align::Center);
    header_actions.append(&live_button);

    let edit_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    edit_button.add_css_class("icon-button");
    edit_button.set_valign(Align::Center);
    header_actions.append(&edit_button);

    let content_stack = gtk::Stack::new();
    content_stack.set_hhomogeneous(false);
    content_stack.set_vhomogeneous(false);
    panel.append(&content_stack);

    let read_root = gtk::Box::new(Orientation::Vertical, 22);
    read_root.add_css_class("read-mode");
    content_stack.add_named(&read_root, Some("read"));

    let read_summary = gtk::Box::new(Orientation::Vertical, 8);
    read_summary.set_halign(Align::Center);
    read_root.append(&read_summary);

    let read_summary_time = gtk::Label::new(None);
    read_summary_time.set_xalign(0.5);
    read_summary_time.add_css_class("read-summary-time");
    read_summary.append(&read_summary_time);

    let read_summary_location = gtk::Label::new(None);
    read_summary_location.set_xalign(0.5);
    read_summary_location.add_css_class("read-summary-location");
    read_summary.append(&read_summary_location);

    let timeline_overlay = gtk::Overlay::new();
    timeline_overlay.add_css_class("timeline-shell");
    timeline_overlay.set_halign(Align::Center);
    timeline_overlay.set_width_request(READ_TIMELINE_WIDTH);
    read_root.append(&timeline_overlay);

    let timeline_area = gtk::DrawingArea::new();
    timeline_area.set_content_width(READ_TIMELINE_WIDTH);
    timeline_area.set_width_request(READ_TIMELINE_WIDTH);
    timeline_area.set_content_height(92);
    timeline_overlay.set_child(Some(&timeline_area));

    let timeline_labels = gtk::Fixed::new();
    timeline_labels.set_can_target(false);
    timeline_labels.set_width_request(READ_TIMELINE_WIDTH);
    timeline_labels.set_height_request(92);
    timeline_overlay.add_overlay(&timeline_labels);
    timeline_overlay.set_measure_overlay(&timeline_labels, false);

    let cards_flow = gtk::FlowBox::new();
    cards_flow.set_selection_mode(SelectionMode::None);
    cards_flow.set_halign(Align::Center);
    cards_flow.set_width_request(READ_TIMELINE_WIDTH);
    cards_flow.set_max_children_per_line(READ_CARD_COLUMNS as u32);
    cards_flow.set_row_spacing(READ_CARD_SPACING as u32);
    cards_flow.set_column_spacing(READ_CARD_SPACING as u32);
    cards_flow.add_css_class("timezone-card-grid");
    read_root.append(&cards_flow);

    let edit_root = gtk::Box::new(Orientation::Vertical, 14);
    content_stack.add_named(&edit_root, Some("edit"));

    let format_row = gtk::Box::new(Orientation::Horizontal, 12);
    format_row.set_halign(Align::Start);
    format_row.set_visible(false);
    edit_root.append(&format_row);

    let format_label = gtk::Label::new(Some("Format"));
    format_label.add_css_class("hint-label");
    format_row.append(&format_label);

    let time_format_combo = build_dropdown(&TIME_FORMAT_OPTIONS, &config.time_format);
    format_row.append(&time_format_combo);

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

    let footer = gtk::Box::new(Orientation::Vertical, 10);
    edit_root.append(&footer);

    let footer_separator = gtk::Separator::new(Orientation::Horizontal);
    footer_separator.set_visible(false);
    footer.append(&footer_separator);

    let add_stack = gtk::Stack::new();
    add_stack.set_hhomogeneous(false);
    add_stack.set_vhomogeneous(false);
    add_stack.set_visible(false);
    footer.append(&add_stack);

    let add_toggle_button = gtk::Button::with_label("+ Add timezone");
    add_toggle_button.add_css_class("add-toggle");
    add_stack.add_named(&add_toggle_button, Some("toggle"));

    let add_panel = gtk::Box::new(Orientation::Vertical, 10);
    add_stack.add_named(&add_panel, Some("panel"));

    let add_box = gtk::Box::new(Orientation::Horizontal, 8);
    add_panel.append(&add_box);

    let add_entry = gtk::Entry::new();
    add_entry.set_hexpand(true);
    add_entry.set_placeholder_text(Some("Add timezone: Europe/Paris, Tokyo, or Asia/Kolkata"));
    add_entry.add_css_class("search-entry");
    add_box.append(&add_entry);

    let add_button = gtk::Button::with_label("Add");
    add_box.append(&add_button);

    let search_results_scroller = gtk::ScrolledWindow::new();
    search_results_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    search_results_scroller.set_overlay_scrolling(true);
    search_results_scroller.set_propagate_natural_height(true);
    search_results_scroller.set_max_content_height(210);
    search_results_scroller.set_visible(false);
    add_panel.append(&search_results_scroller);

    let search_results_box = gtk::Box::new(Orientation::Vertical, 6);
    search_results_scroller.set_child(Some(&search_results_box));

    let hint = gtk::Label::new(Some("Search by timezone, city, or abbreviation like IST."));
    hint.set_xalign(0.0);
    hint.add_css_class("hint-label");
    add_panel.append(&hint);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("status-label");
    status_label.set_visible(false);
    footer.append(&status_label);

    window.set_child(Some(&overlay));
    let (remote_search_sender, remote_search_receiver) = mpsc::channel::<RemoteSearchMessage>();

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
        edit_mode: false,
        add_panel_visible: false,
        editing_timezone: None,
        syncing_controls: false,
        pending_apply_source: None,
        pending_apply_timezone: None,
        content_stack: content_stack.clone(),
        panel_title: title.clone(),
        format_row: format_row.clone(),
        live_button: live_button.clone(),
        edit_button: edit_button.clone(),
        time_format_combo: time_format_combo.clone(),
        read_summary_time: read_summary_time.clone(),
        read_summary_location: read_summary_location.clone(),
        timeline_area: timeline_area.clone(),
        timeline_labels: timeline_labels.clone(),
        cards_flow: cards_flow.clone(),
        footer_separator: footer_separator.clone(),
        add_stack: add_stack.clone(),
        add_toggle_button: add_toggle_button.clone(),
        add_panel: add_panel.clone(),
        add_entry: add_entry.clone(),
        search_results_scroller: search_results_scroller.clone(),
        search_results_box: search_results_box.clone(),
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

    let state_for_timeline = state.clone();
    timeline_area.set_draw_func(move |_, context, width, height| {
        let state = state_for_timeline.borrow();
        let entries = read_entries(&state);
        let anchor = zoned_datetime(state.reference_utc, &state.local_timezone);
        let extent_minutes = timeline_extent_minutes(&anchor, &entries, state.reference_utc) as f64;
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

        let line_y = height as f64 * 0.54;
        let padding = 28.0;
        let usable_width = (width as f64 - padding * 2.0).max(1.0);
        let center_x = padding + usable_width / 2.0;

        context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.16);
        context.set_line_width(1.0);
        context.move_to(padding, line_y);
        context.line_to(width as f64 - padding, line_y);
        let _ = context.stroke();

        for tick in 0..=24 {
            let x = padding + (tick as f64 / 24.0) * usable_width;
            context.set_source_rgba(
                stroke.0,
                stroke.1,
                stroke.2,
                if tick < 24 { 0.12 } else { 0.0 },
            );
            context.move_to(x, line_y - 5.0);
            context.line_to(x, line_y + 5.0);
            let _ = context.stroke();
        }

        context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.22);
        context.move_to(center_x, line_y - 8.0);
        context.line_to(center_x, line_y + 8.0);
        let _ = context.stroke();

        for entry in entries {
            let zoned = zoned_datetime(state.reference_utc, &entry.timezone);
            let relative_minutes = timeline_relative_minutes(&anchor, &zoned) as f64;
            let x = padding
                + (((relative_minutes + extent_minutes) / (extent_minutes * 2.0)) * usable_width);
            context.set_source_rgba(stroke.0, stroke.1, stroke.2, 0.22);
            context.arc(x, line_y, 5.5, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        }
    });

    {
        let mut state_mut = state.borrow_mut();
        state_mut.time_format = effective_time_format(&state_mut.config.time_format);
        sync_config_controls(&mut state_mut);
        render_rows(&mut state_mut);
        update_live_button(&state_mut);
        update_edit_mode(&state_mut);
    }
    set_add_panel_visible(&state, false);

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
            state.search_results =
                merge_search_results(&state.local_search_results, &state.remote_search_results, 8);
            should_render = true;
        }

        if should_render {
            render_search_results(&state_for_remote_results);
        }

        ControlFlow::Continue
    });

    let state_for_edit = state.clone();
    edit_button.connect_clicked(move |_| {
        let leaving_edit_mode = {
            let mut state = state_for_edit.borrow_mut();
            state.edit_mode = !state.edit_mode;
            clear_status(&state);
            !state.edit_mode
        };
        if leaving_edit_mode {
            set_add_panel_visible(&state_for_edit, false);
        }
        let state = state_for_edit.borrow();
        update_edit_mode(&state);
    });

    let state_for_now = state.clone();
    live_button.connect_clicked(move |_| {
        reset_live_now(&state_for_now);
    });

    let state_for_time_format = state.clone();
    time_format_combo.connect_selected_notify(move |combo| {
        let config_manager = {
            let state = state_for_time_format.borrow();
            if state.syncing_controls {
                return;
            }
            state.config_manager.clone()
        };
        let time_format =
            dropdown_selection_value(&TIME_FORMAT_OPTIONS, combo.selected(), "system");
        match config_manager.set_time_format(&time_format) {
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

    let state_for_toggle_add = state.clone();
    add_toggle_button.connect_clicked(move |_| {
        {
            let state = state_for_toggle_add.borrow();
            debug_popup_event(&format!(
                "add_toggle_clicked edit_mode={} add_panel_visible={}",
                state.edit_mode, state.add_panel_visible
            ));
        }
        let visible = !state_for_toggle_add.borrow().add_panel_visible;
        set_add_panel_visible(&state_for_toggle_add, visible);
    });

    let state_for_add_change = state.clone();
    add_entry.connect_changed(move |_| {
        update_search_results(&state_for_add_change);
    });

    let state_for_add_activate = state.clone();
    add_entry.connect_activate(move |_| {
        submit_add_timezone(&state_for_add_activate);
    });

    let state_for_add_click = state.clone();
    add_button.connect_clicked(move |_| {
        submit_add_timezone(&state_for_add_click);
    });

    let state_for_click = state.clone();
    let window_for_click = window.clone();
    let overlay_for_click = overlay.clone();
    let panel_for_click = panel.clone().upcast::<gtk::Widget>();
    let dismiss_click = gtk::GestureClick::new();
    dismiss_click.set_button(0);
    dismiss_click.connect_pressed(move |_, _, x, y| {
        let state = state_for_click.borrow();
        debug_popup_event(&format!(
            "dismiss_click pressed x={x:.1} y={y:.1} dismiss_armed={} edit_mode={}",
            state.dismiss_armed, state.edit_mode
        ));
        if !state.dismiss_armed || state.edit_mode {
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
    let loaded_config = config_manager.load()?;
    let config = sanitize_popup_config(loaded_config.clone());
    if config != loaded_config {
        config_manager.save(&config)?;
    }
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
            request_window_close(&state_for_escape, &window_for_escape, "escape");
            return Propagation::Stop;
        }
        Propagation::Proceed
    });
    window.add_controller(key_controller);

    let state_for_focus = state.clone();
    window.connect_is_active_notify(move |window| {
        let state = state_for_focus.borrow();
        debug_popup_event(&format!(
            "is_active_notify active={} dismiss_armed={} edit_mode={}",
            window.is_active(),
            state.dismiss_armed,
            state.edit_mode
        ));
        if state.dismiss_armed && !state.edit_mode && !window.is_active() {
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
    use super::{visible_read_entries, READ_CARD_LIMIT};
    use crate::config::TimezoneEntry;

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
        assert!(visible.iter().all(|entry| entry.timezone != "America/Cancun"));
        assert_eq!(visible.first().map(|entry| entry.timezone.as_str()), Some("Europe/Paris"));
        assert_eq!(
            visible.last().map(|entry| entry.timezone.as_str()),
            Some("Europe/London")
        );
    }

    #[test]
    fn visible_read_entries_falls_back_to_local_when_it_is_the_only_entry() {
        let entries = vec![entry("America/Cancun")];

        let visible = visible_read_entries(&entries, "America/Cancun");

        assert_eq!(visible, entries);
    }
}
