use crate::config::{
    detect_local_timezone, effective_time_format, ordered_timezones, AppConfig, ConfigManager,
    TimezoneEntry,
};
use crate::layout::{
    load_window_border_size, load_window_gap, popup_top_margin, POPUP_TOP_CONTENT_MARGIN,
};
use crate::theme::{build_css, load_palette};
use crate::time::{
    format_display_time, parse_manual_reference_details, row_metadata, zoned_datetime,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use gtk::gdk;
use gtk::glib::{self, ControlFlow, MainLoop, Propagation};
use gtk::prelude::*;
use gtk::{Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;

#[derive(Clone)]
struct RowWidgets {
    entry: TimezoneEntry,
    root: gtk::Box,
    title: gtk::Label,
    context: gtk::Label,
    meta: gtk::Label,
    lock_button: gtk::Button,
    remove_button: gtk::Button,
    time_entry: gtk::Entry,
    dirty: Rc<Cell<bool>>,
    suppress_changes: Rc<Cell<bool>>,
}

struct PopupState {
    config_manager: ConfigManager,
    config: AppConfig,
    local_timezone: String,
    time_format: String,
    reference_utc: DateTime<Utc>,
    rows_box: gtk::Box,
    rows: Vec<RowWidgets>,
    dismiss_armed: bool,
    live: bool,
    edit_mode: bool,
    editing_timezone: Option<String>,
    syncing_controls: bool,
    pending_apply_source: Option<glib::SourceId>,
    pending_apply_timezone: Option<String>,
    live_button: gtk::Button,
    edit_button: gtk::Button,
    sort_row: gtk::Box,
    sort_combo: gtk::DropDown,
    time_format_combo: gtk::DropDown,
    status_label: gtk::Label,
    self_handle: Weak<RefCell<PopupState>>,
}

struct PidGuard {
    path: PathBuf,
}

const SORT_OPTIONS: [(&str, &str); 3] = [("manual", "Manual"), ("alpha", "A-Z"), ("time", "Time")];
const TIME_FORMAT_OPTIONS: [(&str, &str); 3] =
    [("system", "System"), ("24h", "24h"), ("ampm", "AM/PM")];

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

fn update_row_lock_button(row: &RowWidgets) {
    if row.entry.locked {
        row.lock_button.add_css_class("active");
        row.lock_button
            .set_tooltip_text(Some("Unlock this timezone so it sorts with the rest."));
    } else {
        row.lock_button.remove_css_class("active");
        row.lock_button
            .set_tooltip_text(Some("Keep this timezone above unlocked rows."));
    }
}

fn update_edit_mode(state: &PopupState) {
    state.sort_row.set_visible(state.edit_mode);
    if state.edit_mode {
        state.edit_button.add_css_class("active");
        state.edit_button.set_tooltip_text(Some("Leave edit mode."));
    } else {
        state.edit_button.remove_css_class("active");
        state
            .edit_button
            .set_tooltip_text(Some("Manage timezones and popup settings."));
    }

    let can_remove = state.config.timezones.len() > 1;
    for row in &state.rows {
        row.lock_button.set_visible(state.edit_mode);
        row.remove_button.set_visible(state.edit_mode);
        row.remove_button.set_sensitive(can_remove);
        update_row_lock_button(row);
    }
}

fn sync_config_controls(state: &mut PopupState) {
    state.syncing_controls = true;
    state.sort_combo.set_selected(dropdown_selection_index(
        &SORT_OPTIONS,
        &state.config.sort_mode,
    ));
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
    state.config = config;
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

    let lock_button = gtk::Button::from_icon_name("view-pin-symbolic");
    lock_button.add_css_class("icon-button");
    lock_button.add_css_class("lock-button");
    lock_button.set_size_request(32, 32);
    lock_button.set_valign(Align::Center);
    lock_button.set_visible(false);
    controls.append(&lock_button);

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

    let timezone_name_for_lock = row.entry.timezone.clone();
    let state_for_lock = state_handle.clone();
    row.lock_button.connect_clicked(move |_| {
        let (config_manager, locked) = {
            let state = state_for_lock.borrow();
            let locked = state
                .config
                .timezones
                .iter()
                .find(|entry| entry.timezone == timezone_name_for_lock)
                .map(|entry| entry.locked)
                .unwrap_or(false);
            (state.config_manager.clone(), locked)
        };

        match config_manager.set_timezone_locked(&timezone_name_for_lock, !locked) {
            Ok(config) => {
                let mut state = state_for_lock.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            Err(error) => {
                let state = state_for_lock.borrow();
                set_status(&state, &error.to_string(), true);
            }
        }
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
}

fn format_title(entry: &TimezoneEntry, local_timezone: &str) -> String {
    let mut title = entry.display_label();
    if entry.timezone == local_timezone {
        title = format!("{title}  ·  Local");
    }
    title
}

fn update_row_widgets(state: &mut PopupState) {
    let ordered = ordered_timezones(
        &state.config.timezones,
        &state.config.sort_mode,
        state.reference_utc,
    );
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
        update_row_lock_button(row);

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
}

fn render_rows(state: &mut PopupState) {
    clear_box(&state.rows_box);
    state.rows.clear();

    let entries = ordered_timezones(
        &state.config.timezones,
        &state.config.sort_mode,
        state.reference_utc,
    );
    if entries.is_empty() {
        let empty = gtk::Box::new(Orientation::Vertical, 4);
        empty.set_halign(Align::Start);

        let title = gtk::Label::new(Some("No timezones yet"));
        title.set_xalign(0.0);
        title.add_css_class("empty-state-title");
        empty.append(&title);

        let copy = gtk::Label::new(Some(
            "Add timezones in the Python app, then reopen this preview.",
        ));
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
            state
                .rows_box
                .append(&gtk::Separator::new(Orientation::Horizontal));
        }
    }

    update_row_widgets(state);
    update_edit_mode(state);
}

fn configure_layer_shell(window: &gtk::Window) -> Option<(i32, i32)> {
    window.init_layer_shell();
    window.set_namespace(Some("omarchy-world-clock-rs"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
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
    panel.set_width_request(700);
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

    let sort_row = gtk::Box::new(Orientation::Horizontal, 12);
    sort_row.set_halign(Align::Start);
    sort_row.set_visible(false);
    panel.append(&sort_row);

    let sort_label = gtk::Label::new(Some("Sort"));
    sort_label.add_css_class("hint-label");
    sort_row.append(&sort_label);

    let sort_combo = build_dropdown(&SORT_OPTIONS, &config.sort_mode);
    sort_row.append(&sort_combo);

    let format_label = gtk::Label::new(Some("Format"));
    format_label.add_css_class("hint-label");
    format_label.set_margin_start(16);
    sort_row.append(&format_label);

    let time_format_combo = build_dropdown(&TIME_FORMAT_OPTIONS, &config.time_format);
    sort_row.append(&time_format_combo);

    let rows_box = gtk::Box::new(Orientation::Vertical, 10);
    rows_box.set_margin_top(14);
    panel.append(&rows_box);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("status-label");
    status_label.set_visible(false);
    panel.append(&status_label);

    window.set_child(Some(&overlay));

    let state = Rc::new(RefCell::new(PopupState {
        config_manager,
        config,
        local_timezone,
        time_format: String::new(),
        reference_utc: Utc::now(),
        rows_box,
        rows: Vec::new(),
        dismiss_armed: false,
        live: true,
        edit_mode: false,
        editing_timezone: None,
        syncing_controls: false,
        pending_apply_source: None,
        pending_apply_timezone: None,
        live_button: live_button.clone(),
        edit_button: edit_button.clone(),
        sort_row: sort_row.clone(),
        sort_combo: sort_combo.clone(),
        time_format_combo: time_format_combo.clone(),
        status_label,
        self_handle: Weak::new(),
    }));
    state.borrow_mut().self_handle = Rc::downgrade(&state);
    {
        let mut state_mut = state.borrow_mut();
        state_mut.time_format = effective_time_format(&state_mut.config.time_format);
        render_rows(&mut state_mut);
        update_live_button(&state_mut);
    }

    let state_for_edit = state.clone();
    edit_button.connect_clicked(move |_| {
        let mut state = state_for_edit.borrow_mut();
        state.edit_mode = !state.edit_mode;
        clear_status(&state);
        update_edit_mode(&state);
    });

    let state_for_now = state.clone();
    live_button.connect_clicked(move |_| {
        reset_live_now(&state_for_now);
    });

    let state_for_sort = state.clone();
    sort_combo.connect_selected_notify(move |combo| {
        let config_manager = {
            let state = state_for_sort.borrow();
            if state.syncing_controls {
                return;
            }
            state.config_manager.clone()
        };
        let sort_mode = dropdown_selection_value(&SORT_OPTIONS, combo.selected(), "manual");
        match config_manager.set_sort_mode(&sort_mode) {
            Ok(config) => {
                let mut state = state_for_sort.borrow_mut();
                refresh_config_state(&mut state, config);
            }
            Err(error) => {
                let state = state_for_sort.borrow();
                set_status(&state, &error.to_string(), true);
            }
        }
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

    let state_for_click = state.clone();
    let window_for_click = window.clone();
    let dismiss_click = gtk::GestureClick::new();
    dismiss_click.set_button(0);
    dismiss_click.connect_pressed(move |_, _, _, _| {
        if state_for_click.borrow().dismiss_armed {
            window_for_click.close();
        }
    });
    dismiss_area.add_controller(dismiss_click);

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
    let window_for_escape = window.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            window_for_escape.close();
            return Propagation::Stop;
        }
        Propagation::Proceed
    });
    window.add_controller(key_controller);

    let state_for_focus = state.clone();
    window.connect_is_active_notify(move |window| {
        let state = state_for_focus.borrow();
        if state.dismiss_armed && !state.edit_mode && !window.is_active() {
            window.close();
        }
    });

    let main_loop = MainLoop::new(None, false);
    let main_loop_for_close = main_loop.clone();
    window.connect_close_request(move |_| {
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
