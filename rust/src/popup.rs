use crate::config::{
    detect_local_timezone, effective_time_format, ordered_timezones, AppConfig, ConfigManager,
    TimezoneEntry,
};
use crate::layout::{
    load_window_border_size, load_window_gap, popup_top_margin, POPUP_TOP_CONTENT_MARGIN,
};
use crate::theme::{build_css, load_palette};
use crate::time::{format_display_time, row_metadata, zoned_datetime};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use gtk::gdk;
use gtk::glib::{self, ControlFlow, MainLoop, Propagation};
use gtk::prelude::*;
use gtk::{Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone)]
struct RowWidgets {
    entry: TimezoneEntry,
    root: gtk::Box,
    title: gtk::Label,
    context: gtk::Label,
    meta: gtk::Label,
    time: gtk::Label,
}

struct PopupState {
    config: AppConfig,
    local_timezone: String,
    time_format: String,
    reference_utc: DateTime<Utc>,
    rows_box: gtk::Box,
    rows: Vec<RowWidgets>,
    dismiss_armed: bool,
}

struct PidGuard {
    path: PathBuf,
}

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

fn build_row(entry: &TimezoneEntry) -> RowWidgets {
    let row = gtk::Box::new(Orientation::Horizontal, 16);
    row.add_css_class("clock-row");

    let info = gtk::Box::new(Orientation::Vertical, 2);
    info.set_hexpand(true);

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

    let time_chip = gtk::Box::new(Orientation::Horizontal, 0);
    time_chip.add_css_class("time-chip");
    time_chip.set_valign(Align::Center);

    let time = gtk::Label::new(None);
    time.add_css_class("time-label");
    time_chip.append(&time);

    row.append(&time_chip);

    RowWidgets {
        entry: entry.clone(),
        root: row,
        title,
        context,
        meta,
        time,
    }
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
        row.time
            .set_text(&format_display_time(&zoned, &state.time_format));
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
        return;
    }

    for (index, entry) in entries.iter().enumerate() {
        let widgets = build_row(entry);
        state.rows_box.append(&widgets.root);
        state.rows.push(widgets);

        if index + 1 < entries.len() {
            state
                .rows_box
                .append(&gtk::Separator::new(Orientation::Horizontal));
        }
    }

    update_row_widgets(state);
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

fn build_window(
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
    top_band.set_valign(Align::Start);
    top_band.set_margin_top(POPUP_TOP_CONTENT_MARGIN);
    top_band.set_margin_bottom(12);
    overlay.add_overlay(&top_band);

    let panel = gtk::Box::new(Orientation::Vertical, 14);
    panel.add_css_class("world-clock-panel");
    panel.set_width_request(620);
    panel.set_halign(Align::Center);
    top_band.append(&panel);

    let header = gtk::Box::new(Orientation::Horizontal, 12);
    panel.append(&header);

    let title = gtk::Label::new(Some("World Clock"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("panel-title");
    header.append(&title);

    let rows_box = gtk::Box::new(Orientation::Vertical, 10);
    rows_box.set_margin_top(14);
    panel.append(&rows_box);

    window.set_child(Some(&overlay));

    let state = Rc::new(RefCell::new(PopupState {
        config,
        local_timezone,
        time_format: String::new(),
        reference_utc: Utc::now(),
        rows_box,
        rows: Vec::new(),
        dismiss_armed: false,
    }));
    {
        let mut state_mut = state.borrow_mut();
        state_mut.time_format = effective_time_format(&state_mut.config.time_format);
        render_rows(&mut state_mut);
    }

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
    let state = build_window(config, local_timezone, &window);
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
        if state_for_focus.borrow().dismiss_armed && !window.is_active() {
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
        state.reference_utc = Utc::now();
        update_row_widgets(&mut state);
        ControlFlow::Continue
    });

    window.present();
    main_loop.run();
    Ok(())
}
