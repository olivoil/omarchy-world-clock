from __future__ import annotations

import atexit
import os
import signal
import threading
import tomllib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

import gi

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
gi.require_version("GtkLayerShell", "0.1")

from gi.repository import Gdk, GLib, Gtk, GtkLayerShell, Pango

from .configuration import (
    ConfigManager,
    RemotePlaceSearch,
    TimezoneEntry,
    TimezoneResolver,
    TimezoneSearchResult,
    detect_local_timezone,
    ordered_timezones,
    effective_time_format,
)
from .layout import (
    POPUP_TOP_CONTENT_MARGIN,
    load_window_border_size,
    load_window_gap,
    popup_top_margin,
)
from .core import (
    all_timezones,
    format_display_time,
    format_offset,
    parse_manual_reference_details,
    zoned_datetime,
)


SEARCH_RESULT_LIMIT = 8
def rgba(hex_value: str, alpha: float) -> str:
    value = hex_value.lstrip("#")
    red = int(value[0:2], 16)
    green = int(value[2:4], 16)
    blue = int(value[4:6], 16)
    return f"rgba({red}, {green}, {blue}, {alpha:.3f})"


@dataclass
class Palette:
    accent: str = "#faa968"
    foreground: str = "#f6dcac"
    background: str = "#05182e"


def load_palette() -> Palette:
    palette = Palette()
    path = Path.home() / ".config" / "omarchy" / "current" / "theme" / "colors.toml"
    if not path.exists():
        return palette

    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return palette

    return Palette(
        accent=data.get("accent", palette.accent),
        foreground=data.get("foreground", palette.foreground),
        background=data.get("background", palette.background),
    )


def build_css(palette: Palette) -> str:
    return f"""
window {{
  background: transparent;
}}

#world-clock-panel {{
  background: {rgba(palette.background, 0.94)};
  border: 1px solid {rgba(palette.accent, 0.42)};
  border-radius: 18px;
  padding: 18px 18px 12px 18px;
  box-shadow: 0 18px 36px {rgba("#000000", 0.30)};
}}

.panel-title {{
  color: {palette.foreground};
  font-weight: 700;
  font-size: 18px;
}}

.clock-context,
.clock-meta,
.hint-label {{
  color: {rgba(palette.foreground, 0.72)};
  font-size: 12px;
}}

.status-label {{
  color: {palette.accent};
  font-size: 12px;
}}

.status-label.error {{
  color: #ff8b8b;
}}

.clock-title {{
  color: {palette.foreground};
  font-weight: 700;
  font-size: 14px;
}}

.clock-row {{
  border-radius: 14px;
}}

.clock-row.dragging {{
  opacity: 0.18;
}}

.clock-row.drag-preview {{
  background: {rgba(palette.background, 0.94)};
  border: 1px solid {rgba(palette.accent, 0.34)};
  border-radius: 14px;
  padding: 10px 12px;
  box-shadow: 0 14px 28px {rgba("#000000", 0.24)};
}}

.drag-preview-time {{
  color: {palette.foreground};
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: 0.1em;
}}

.drop-slot-line,
.drag-insert-marker {{
  min-height: 2px;
  border-radius: 999px;
  background: {rgba(palette.accent, 0.18)};
}}

.drag-insert-marker {{
  min-height: 4px;
  background: {rgba(palette.accent, 0.78)};
}}

.drag-handle-label {{
  color: {rgba(palette.foreground, 0.44)};
  font-size: 20px;
  font-weight: 700;
}}

.time-entry {{
  color: {palette.foreground};
  caret-color: {palette.accent};
  background: {rgba("#000000", 0.10)};
  border: 1px solid {rgba(palette.foreground, 0.12)};
  border-radius: 12px;
  padding: 12px 14px;
  font-family: "JetBrainsMono Nerd Font Mono", "JetBrains Mono", monospace;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: 0.16em;
}}

.time-entry:focus {{
  border-color: {rgba(palette.accent, 0.75)};
  box-shadow: 0 0 0 3px {rgba(palette.accent, 0.14)};
}}

.time-entry.error {{
  border-color: {rgba("#ff8b8b", 0.92)};
}}

.search-entry {{
  font-size: 15px;
}}

button {{
  color: {palette.foreground};
  background: {rgba(palette.background, 0.72)};
  border: 1px solid {rgba(palette.foreground, 0.10)};
  border-radius: 10px;
  padding: 8px 12px;
}}

button:hover {{
  background: {rgba(palette.background, 0.86)};
}}

button:focus {{
  border-color: {rgba(palette.accent, 0.75)};
}}

button.icon-button {{
  background: transparent;
  border-color: {rgba(palette.foreground, 0.06)};
  border-radius: 999px;
  min-width: 34px;
  min-height: 34px;
  padding: 6px;
}}

button.icon-button:hover {{
  background: {rgba(palette.foreground, 0.06)};
  border-color: {rgba(palette.foreground, 0.16)};
}}

button.icon-button:disabled {{
  opacity: 0.28;
}}

button.icon-button.active {{
  background: {rgba(palette.accent, 0.10)};
  border-color: {rgba(palette.accent, 0.30)};
}}

button.lock-button {{
  min-width: 30px;
  min-height: 30px;
  padding: 4px;
  opacity: 0.28;
}}

button.lock-button.active {{
  opacity: 1.0;
}}

button.remove-button {{
  min-width: 34px;
  min-height: 34px;
  padding: 0;
  font-size: 13px;
}}

button.remove-button:disabled {{
  opacity: 0.28;
}}

button.search-result-button {{
  padding: 10px 12px;
  border-radius: 12px;
}}

button.search-result-button:hover {{
  background: {rgba(palette.accent, 0.12)};
  border-color: {rgba(palette.accent, 0.35)};
}}

button.add-toggle {{
  padding: 9px 14px;
}}

.search-result-title {{
  color: {palette.foreground};
  font-size: 14px;
  font-weight: 700;
}}

.search-result-meta {{
  color: {rgba(palette.foreground, 0.72)};
  font-size: 12px;
}}

.empty-state-title {{
  color: {palette.foreground};
  font-size: 15px;
  font-weight: 700;
}}

.empty-state-copy {{
  color: {rgba(palette.foreground, 0.72)};
  font-size: 13px;
}}

separator {{
  color: {rgba(palette.foreground, 0.09)};
}}
"""


def apply_css() -> None:
    palette = load_palette()
    provider = Gtk.CssProvider()
    provider.load_from_data(build_css(palette).encode("utf-8"))
    screen = Gdk.Screen.get_default()
    if screen is not None:
        Gtk.StyleContext.add_provider_for_screen(
            screen,
            provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )


class ClockRow(Gtk.Box):
    def __init__(
        self,
        window: "WorldClockWindow",
        entry: TimezoneEntry,
        manual_sort: bool,
    ) -> None:
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=16)
        self.window = window
        self.entry = entry
        self.timezone_name = entry.timezone
        self.manual_sort = manual_sort
        self.suppress_changes = False
        self.dirty = False
        self.current_zoned = zoned_datetime(window.reference_utc, self.timezone_name)
        self.get_style_context().add_class("clock-row")

        self.drag_handle = Gtk.EventBox()
        self.drag_handle.set_no_show_all(True)
        self.drag_handle.set_visible_window(False)
        self.drag_handle.set_above_child(True)
        self.drag_handle.set_valign(Gtk.Align.CENTER)
        self.drag_handle.set_margin_end(4)
        self.drag_handle.set_tooltip_text("Drag to reorder")
        self.drag_handle.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.BUTTON1_MOTION_MASK
        )
        self.drag_handle.connect("button-press-event", self.on_drag_button_press)
        self.drag_armed = False
        self.drag_active = False

        handle_label = Gtk.Label()
        handle_label.set_text("≡")
        handle_label.get_style_context().add_class("drag-handle-label")
        self.drag_handle.add(handle_label)
        self.pack_start(self.drag_handle, False, False, 0)

        info_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        info_box.set_hexpand(True)

        self.title_label = Gtk.Label(xalign=0)
        self.title_label.get_style_context().add_class("clock-title")
        info_box.pack_start(self.title_label, False, False, 0)

        self.context_label = Gtk.Label(xalign=0)
        self.context_label.get_style_context().add_class("clock-context")
        self.context_label.set_ellipsize(Pango.EllipsizeMode.END)
        info_box.pack_start(self.context_label, False, False, 0)

        self.meta_label = Gtk.Label(xalign=0)
        self.meta_label.get_style_context().add_class("clock-meta")
        info_box.pack_start(self.meta_label, False, False, 0)

        self.pack_start(info_box, True, True, 0)

        controls = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        controls.set_halign(Gtk.Align.END)
        controls.set_valign(Gtk.Align.CENTER)

        self.time_entry = Gtk.Entry()
        self.time_entry.set_alignment(1.0)
        self.time_entry.set_width_chars(8)
        self.time_entry.set_max_length(19)
        self.time_entry.set_placeholder_text(self.window.time_entry_placeholder())
        self.time_entry.get_style_context().add_class("time-entry")
        self.time_entry.connect("focus-in-event", self.on_focus_in)
        self.time_entry.connect("focus-out-event", self.on_focus_out)
        self.time_entry.connect("changed", self.on_changed)
        self.time_entry.connect("activate", self.on_activate)

        self.lock_button = Gtk.Button()
        self.lock_button.get_style_context().add_class("icon-button")
        self.lock_button.get_style_context().add_class("lock-button")
        self.lock_button.set_no_show_all(True)
        self.lock_button.set_valign(Gtk.Align.CENTER)
        self.lock_button.set_always_show_image(True)
        self.lock_button.set_tooltip_text("Keep this timezone above unlocked rows.")
        self.lock_button.connect("clicked", self.on_toggle_lock)
        self.lock_image = Gtk.Image()
        self.lock_button.set_image(self.lock_image)
        controls.pack_start(self.lock_button, False, False, 0)
        controls.pack_start(self.time_entry, False, False, 0)

        self.remove_button = Gtk.Button(label="x")
        self.remove_button.set_no_show_all(True)
        self.remove_button.get_style_context().add_class("remove-button")
        self.remove_button.set_valign(Gtk.Align.CENTER)
        self.remove_button.set_sensitive(True)
        self.remove_button.set_tooltip_text("Remove timezone")
        self.remove_button.connect("clicked", self.on_remove)
        controls.pack_start(self.remove_button, False, False, 0)

        self.pack_start(controls, False, False, 0)
        self.update_lock_button()
        self.set_edit_mode(window.edit_mode)
        self.refresh(window.reference_utc)

    def can_reorder(self) -> bool:
        return self.manual_sort and not self.entry.locked

    def set_edit_mode(self, enabled: bool) -> None:
        drag_enabled = enabled and self.can_reorder()
        if drag_enabled:
            self.drag_handle.show()
            child = self.drag_handle.get_child()
            if child is not None:
                child.show()
        else:
            if self.drag_active or self.drag_armed:
                self.drag_armed = False
                self.drag_active = False
                self.get_style_context().remove_class("dragging")
                self.window.cancel_pointer_drag()
            self.drag_handle.hide()
            self.window.clear_drop_slot()

        if enabled:
            self.lock_button.show()
            self.remove_button.show()
        else:
            self.lock_button.hide()
            self.remove_button.hide()

    def build_drag_preview(self) -> Gtk.Widget:
        preview = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
        preview_context = preview.get_style_context()
        preview_context.add_class("clock-row")
        preview_context.add_class("drag-preview")

        handle_label = Gtk.Label()
        handle_label.set_text("≡")
        handle_label.get_style_context().add_class("drag-handle-label")
        preview.pack_start(handle_label, False, False, 0)

        info_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        info_box.set_hexpand(True)

        title = Gtk.Label(xalign=0)
        title.set_text(self.title_label.get_text())
        title.get_style_context().add_class("clock-title")
        info_box.pack_start(title, False, False, 0)

        context = Gtk.Label(xalign=0)
        context.set_text(self.timezone_name)
        context.get_style_context().add_class("clock-context")
        info_box.pack_start(context, False, False, 0)

        preview.pack_start(info_box, True, True, 0)

        time_label = Gtk.Label(xalign=1)
        time_label.set_text(self.window.display_time(self.current_zoned))
        time_label.get_style_context().add_class("drag-preview-time")
        preview.pack_start(time_label, False, False, 0)

        width = self.get_allocated_width()
        if width > 0:
            preview.set_size_request(width, -1)
        preview.show_all()
        return preview

    def refresh(self, reference_utc: datetime) -> None:
        self.current_zoned = zoned_datetime(reference_utc, self.timezone_name)
        title = self.entry.display_label()
        if self.timezone_name == self.window.local_timezone:
            title = f"{title}  ·  Local"
        self.title_label.set_text(title)
        self.context_label.set_text(self.timezone_name)

        offset = format_offset(self.current_zoned.utcoffset())
        abbrev = self.current_zoned.tzname() or ""
        self.meta_label.set_text(
            f"{self.current_zoned.strftime('%a %d %b')}  ·  {abbrev}  ·  {offset}"
        )

        if self.window.editing_row is self or self.time_entry.is_focus():
            return

        self.set_error(False)
        self.suppress_changes = True
        self.time_entry.set_text(self.window.display_time(self.current_zoned))
        self.suppress_changes = False
        self.dirty = False

    def set_error(self, enabled: bool) -> None:
        context = self.time_entry.get_style_context()
        if enabled:
            context.add_class("error")
        else:
            context.remove_class("error")

    def update_lock_button(self) -> None:
        context = self.lock_button.get_style_context()
        icon_name = "view-pin-symbolic"
        if self.entry.locked:
            context.add_class("active")
            self.lock_button.set_tooltip_text("Unlock this timezone so it sorts with the rest.")
        else:
            context.remove_class("active")
            self.lock_button.set_tooltip_text("Keep this timezone above unlocked rows.")
        self.lock_image.set_from_icon_name(icon_name, Gtk.IconSize.MENU)

    def on_focus_in(self, *_args: object) -> bool:
        self.window.editing_row = self
        self.dirty = False
        self.set_error(False)
        self.time_entry.select_region(0, -1)
        return False

    def on_focus_out(self, *_args: object) -> bool:
        if self.window.editing_row is self:
            self.window.editing_row = None
        if self.dirty:
            applied = self.window.flush_live_apply(self, show_errors=False)
            if not applied:
                self.set_error(False)
                self.refresh(self.window.reference_utc)
        else:
            self.refresh(self.window.reference_utc)
        return False

    def on_changed(self, *_args: object) -> None:
        if not self.suppress_changes and self.time_entry.is_focus():
            self.dirty = True
            self.set_error(False)
            self.window.schedule_live_apply(self)

    def on_activate(self, *_args: object) -> None:
        self.window.flush_live_apply(self, show_errors=True)

    def on_remove(self, *_args: object) -> None:
        self.window.remove_timezone(self.timezone_name)

    def on_drag_button_press(
        self,
        _widget: Gtk.Widget,
        event: Gdk.EventButton,
    ) -> bool:
        if event.button != Gdk.BUTTON_PRIMARY:
            return False
        return self.window.arm_pointer_drag(self, event.x_root, event.y_root)

    def on_toggle_lock(self, *_args: object) -> None:
        self.window.toggle_timezone_lock(self.timezone_name)


class WorldClockWindow(Gtk.Window):
    def __init__(self, pid_path: Path, config_path: Path | None = None) -> None:
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.pid_path = pid_path
        self.config_manager = ConfigManager(config_path)
        self.config = self.config_manager.load()
        self.resolver = TimezoneResolver(all_timezones())
        self.place_search = RemotePlaceSearch(self.resolver.zones)
        self.local_timezone = detect_local_timezone()
        self.reference_utc = datetime.now(timezone.utc)
        self.live = True
        self.rows: list[ClockRow] = []
        self.row_separators: list[Gtk.Separator] = []
        self.local_search_results: list[TimezoneSearchResult] = []
        self.remote_search_results: list[TimezoneSearchResult] = []
        self.search_results: list[TimezoneSearchResult] = []
        self.search_generation = 0
        self.remote_search_source: int | None = None
        self.status_clear_source: int | None = None
        self.pending_apply_source: int | None = None
        self.pending_apply_row: ClockRow | None = None
        self.editing_row: ClockRow | None = None
        self.drag_pending_row: ClockRow | None = None
        self.drag_source_row: ClockRow | None = None
        self.active_drop_index: int | None = None
        self.drag_start_root_x = 0.0
        self.drag_start_root_y = 0.0
        self.drag_start_rows_box_y = 0.0
        self.drag_start_row_top_y = 0.0
        self.drag_row_overlay_x = 0.0
        self.dismiss_armed = False
        self.edit_mode = False
        self.root: Gtk.EventBox | None = None
        self.drag_ghost: Gtk.Widget | None = None

        self.set_title("Omarchy World Clock")
        self.set_resizable(False)
        self.set_decorated(False)
        self.set_keep_above(True)
        self.set_accept_focus(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_type_hint(Gdk.WindowTypeHint.DROPDOWN_MENU)
        self.connect("destroy", self.on_destroy)
        self.connect("delete-event", self.on_delete)
        self.connect("key-press-event", self.on_key_press)

        screen = self.get_screen()
        if screen is not None and screen.is_composited():
            visual = screen.get_rgba_visual()
            if visual is not None:
                self.set_visual(visual)

        self.configure_layer_shell()
        self.build_ui()
        self.rebuild_rows()
        self.show_all()
        self.present()
        GLib.timeout_add(200, self.arm_dismissal)

        GLib.timeout_add_seconds(1, self.on_tick)

    def configure_layer_shell(self) -> None:
        GtkLayerShell.init_for_window(self)
        GtkLayerShell.set_namespace(self, "omarchy-world-clock")
        GtkLayerShell.set_layer(self, GtkLayerShell.Layer.OVERLAY)
        GtkLayerShell.set_keyboard_mode(self, GtkLayerShell.KeyboardMode.ON_DEMAND)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.TOP, True)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.BOTTOM, True)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.LEFT, True)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.RIGHT, True)
        GtkLayerShell.set_margin(
            self,
            GtkLayerShell.Edge.TOP,
            popup_top_margin(
                load_window_gap(),
                load_window_border_size(),
            ),
        )

    def build_ui(self) -> None:
        root = Gtk.EventBox()
        root.set_visible_window(False)
        root.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.POINTER_MOTION_MASK
            | Gdk.EventMask.BUTTON1_MOTION_MASK
        )
        root.connect("button-press-event", self.on_root_button_press)
        root.connect("motion-notify-event", self.on_root_motion_notify)
        root.connect("button-release-event", self.on_root_button_release)
        self.root = root
        self.add(root)

        layout = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        layout.set_vexpand(True)
        layout.set_hexpand(True)
        root.add(layout)

        top_band = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        top_band.set_halign(Gtk.Align.CENTER)
        top_band.set_valign(Gtk.Align.START)
        top_band.set_margin_top(POPUP_TOP_CONTENT_MARGIN)
        top_band.set_margin_bottom(12)
        layout.pack_start(top_band, False, False, 0)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        panel.set_name("world-clock-panel")
        panel.set_size_request(620, -1)
        panel.set_halign(Gtk.Align.CENTER)
        self.panel = panel
        top_band.pack_start(panel, False, False, 0)

        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        panel.pack_start(header, False, False, 0)

        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
        titles.set_valign(Gtk.Align.CENTER)
        header.pack_start(titles, True, True, 0)

        title = Gtk.Label(xalign=0)
        title.set_text("World Clock")
        title.get_style_context().add_class("panel-title")
        titles.pack_start(title, False, False, 0)

        header_actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        header.pack_start(header_actions, False, False, 0)

        self.live_button = Gtk.Button()
        self.live_button.get_style_context().add_class("icon-button")
        self.live_button.set_image(
            Gtk.Image.new_from_icon_name("view-refresh-symbolic", Gtk.IconSize.MENU)
        )
        self.live_button.connect("clicked", self.on_now_clicked)
        header_actions.pack_start(self.live_button, False, False, 0)

        self.edit_button = Gtk.Button()
        self.edit_button.get_style_context().add_class("icon-button")
        self.edit_button.set_image(
            Gtk.Image.new_from_icon_name("emblem-system-symbolic", Gtk.IconSize.MENU)
        )
        self.edit_button.connect("clicked", self.on_toggle_edit_mode)
        header_actions.pack_start(self.edit_button, False, False, 0)

        sort_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        sort_row.set_halign(Gtk.Align.START)
        sort_row.set_no_show_all(True)
        self.sort_row = sort_row
        panel.pack_start(sort_row, False, False, 0)

        sort_label = Gtk.Label(xalign=0)
        sort_label.set_text("Sort")
        sort_label.get_style_context().add_class("hint-label")
        sort_row.pack_start(sort_label, False, False, 0)

        self.sort_combo = Gtk.ComboBoxText()
        self.sort_combo.append("manual", "Manual")
        self.sort_combo.append("alpha", "A-Z")
        self.sort_combo.append("time", "Time")
        self.sort_combo.set_active_id(self.config.sort_mode)
        self.sort_combo.connect("changed", self.on_sort_mode_changed)
        sort_row.pack_start(self.sort_combo, False, False, 0)

        format_label = Gtk.Label(xalign=0)
        format_label.set_text("Format")
        format_label.get_style_context().add_class("hint-label")
        sort_row.pack_start(format_label, False, False, 12)

        self.time_format_combo = Gtk.ComboBoxText()
        self.time_format_combo.append("system", "System")
        self.time_format_combo.append("24h", "24h")
        self.time_format_combo.append("ampm", "AM/PM")
        self.time_format_combo.set_active_id(self.config.time_format)
        self.time_format_combo.connect("changed", self.on_time_format_changed)
        sort_row.pack_start(self.time_format_combo, False, False, 0)

        rows_overlay = Gtk.Overlay()
        rows_overlay.set_margin_top(14)
        self.rows_overlay = rows_overlay
        panel.pack_start(rows_overlay, False, False, 0)

        self.rows_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        rows_overlay.add(self.rows_box)

        self.drag_layer = Gtk.Fixed()
        self.drag_layer.set_hexpand(True)
        self.drag_layer.set_vexpand(True)
        rows_overlay.add_overlay(self.drag_layer)
        rows_overlay.set_overlay_pass_through(self.drag_layer, True)

        insertion_marker = Gtk.Box()
        insertion_marker.set_no_show_all(True)
        insertion_marker.set_size_request(-1, 4)
        insertion_marker.set_hexpand(True)
        insertion_marker.set_halign(Gtk.Align.FILL)
        insertion_marker.set_margin_top(2)
        insertion_marker.set_margin_bottom(2)
        insertion_marker.get_style_context().add_class("drag-insert-marker")
        self.insertion_marker = insertion_marker

        footer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        panel.pack_start(footer, False, False, 0)

        separator = Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL)
        separator.set_no_show_all(True)
        self.footer_separator = separator
        footer.pack_start(separator, False, False, 0)

        self.add_stack = Gtk.Stack()
        self.add_stack.set_homogeneous(False)
        self.add_stack.set_no_show_all(True)
        footer.pack_start(self.add_stack, False, False, 0)

        self.add_toggle_button = Gtk.Button(label="+ Add timezone")
        self.add_toggle_button.get_style_context().add_class("add-toggle")
        self.add_toggle_button.connect("clicked", self.on_toggle_add_panel)
        self.add_stack.add_named(self.add_toggle_button, "toggle")

        self.add_panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.add_stack.add_named(self.add_panel, "panel")

        add_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.add_panel.pack_start(add_box, False, False, 0)

        self.add_entry = Gtk.Entry()
        self.add_entry.set_hexpand(True)
        self.add_entry.set_placeholder_text("Add timezone: Europe/Paris, Tokyo, or Bangalore")
        self.add_entry.connect("activate", self.on_add_timezone)
        self.add_entry.connect("changed", self.on_search_query_changed)
        self.add_entry.get_style_context().add_class("search-entry")
        add_box.pack_start(self.add_entry, True, True, 0)

        add_button = Gtk.Button(label="Add")
        add_button.connect("clicked", self.on_add_timezone)
        add_box.pack_start(add_button, False, False, 0)

        self.search_results_scroller = Gtk.ScrolledWindow()
        self.search_results_scroller.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.search_results_scroller.set_overlay_scrolling(True)
        self.search_results_scroller.set_propagate_natural_height(True)
        self.search_results_scroller.set_max_content_height(210)
        self.search_results_scroller.set_no_show_all(True)
        self.add_panel.pack_start(self.search_results_scroller, False, False, 0)

        self.search_results_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.search_results_scroller.add(self.search_results_box)

        hint = Gtk.Label(xalign=0)
        hint.set_text("Search by place, timezone, or abbreviation like IST.")
        hint.get_style_context().add_class("hint-label")
        self.add_panel.pack_start(hint, False, False, 0)

        self.status_label = Gtk.Label(xalign=0)
        self.status_label.get_style_context().add_class("status-label")
        self.status_label.set_no_show_all(True)
        footer.pack_start(self.status_label, False, False, 0)
        self.set_add_panel_visible(False)
        self.update_edit_mode()

    def selected_entries(self) -> list[TimezoneEntry]:
        return ordered_timezones(self.config.timezones, self.config.sort_mode, self.reference_utc)

    def rebuild_rows(self) -> None:
        self.clear_drop_slot()
        for child in list(self.rows_box.get_children()):
            self.rows_box.remove(child)

        self.rows = []
        self.row_separators = []
        entries = self.selected_entries()
        if not entries:
            empty_state = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            empty_state.set_halign(Gtk.Align.START)

            empty_title = Gtk.Label(xalign=0)
            empty_title.set_text("No timezones yet")
            empty_title.get_style_context().add_class("empty-state-title")
            empty_state.pack_start(empty_title, False, False, 0)

            empty_copy = Gtk.Label(xalign=0)
            empty_copy.set_text("Use edit mode to add or restore a timezone.")
            empty_copy.get_style_context().add_class("empty-state-copy")
            empty_state.pack_start(empty_copy, False, False, 0)

            self.rows_box.pack_start(empty_state, False, False, 0)
            self.rows_box.show_all()
            self.update_edit_mode()
            return

        manual_sort = self.config.sort_mode == "manual"
        for index, entry in enumerate(entries):
            row = ClockRow(
                self,
                entry,
                manual_sort=manual_sort,
            )
            self.rows.append(row)
            self.rows_box.pack_start(row, False, False, 0)
            if index < len(entries) - 1:
                separator = Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL)
                self.row_separators.append(separator)
                self.rows_box.pack_start(separator, False, False, 0)

        self.refresh_rows()
        self.rows_box.show_all()
        self.update_row_separators()
        self.update_edit_mode()

    def refresh_rows(self) -> None:
        for row in self.rows:
            row.refresh(self.reference_utc)
        self.update_mode_button()

    def begin_drag(self, row: ClockRow) -> None:
        self.drag_source_row = row
        self.clear_drop_slot()
        if self.drag_ghost is not None:
            self.drag_layer.remove(self.drag_ghost)
            self.drag_ghost.destroy()
            self.drag_ghost = None

        ghost = row.build_drag_preview()
        self.drag_layer.put(
            ghost,
            int(round(self.drag_row_overlay_x)),
            int(round(self.drag_start_row_top_y)),
        )
        ghost.show_all()
        self.drag_ghost = ghost

    def arm_pointer_drag(
        self,
        row: ClockRow,
        start_root_x: float,
        start_root_y: float,
    ) -> bool:
        if not self.edit_mode or not row.can_reorder():
            return False
        translated = row.translate_coordinates(
            self.rows_box,
            0,
            row.get_allocated_height() // 2,
        )
        if translated is None:
            return False
        overlay_origin = row.translate_coordinates(self.rows_overlay, 0, 0)
        if overlay_origin is None:
            return False

        self.cancel_pointer_drag()
        self.drag_pending_row = row
        self.drag_start_root_x = float(start_root_x)
        self.drag_start_root_y = float(start_root_y)
        self.drag_start_rows_box_y = float(translated[1])
        self.drag_start_row_top_y = float(overlay_origin[1])
        self.drag_row_overlay_x = float(overlay_origin[0])
        row.drag_armed = True
        row.drag_active = False
        if self.root is not None:
            self.root.grab_add()
        return True

    def update_pointer_drag(self, root_x: float, root_y: float) -> bool:
        row = self.drag_pending_row
        if row is None or not row.drag_armed:
            return False

        offset_x = root_x - self.drag_start_root_x
        offset_y = root_y - self.drag_start_root_y
        if not row.drag_active:
            if max(abs(offset_x), abs(offset_y)) < 8:
                return True
            row.drag_active = True
            row.get_style_context().add_class("dragging")
            self.begin_drag(row)

        self.set_drag_ghost_position(root_y)
        rows_y = int(round(self.drag_start_rows_box_y + offset_y))
        self.update_drag_position(rows_y)
        return True

    def finish_pointer_drag(self, root_y: float, commit: bool) -> bool:
        row = self.drag_pending_row
        if row is None:
            return False

        if row.drag_active:
            rows_y = int(round(self.drag_start_rows_box_y + (root_y - self.drag_start_root_y)))
            self.set_drag_ghost_position(root_y)
            self.update_drag_position(rows_y)

        row.drag_armed = False
        row.drag_active = False
        row.get_style_context().remove_class("dragging")

        insert_index = self.active_drop_index
        timezone_name = self.drag_source_row.timezone_name if self.drag_source_row is not None else None

        self.end_drag()

        committed = False
        if commit and timezone_name is not None and insert_index is not None:
            committed = self.reorder_timezone_to_index(timezone_name, insert_index)

        self.drag_pending_row = None
        self.drag_start_root_x = 0.0
        self.drag_start_root_y = 0.0
        self.drag_start_rows_box_y = 0.0
        self.drag_start_row_top_y = 0.0
        self.drag_row_overlay_x = 0.0
        if self.root is not None and self.root.has_grab():
            self.root.grab_remove()
        return committed

    def cancel_pointer_drag(self) -> None:
        self.finish_pointer_drag(self.drag_start_root_y, commit=False)

    def update_drag_position(self, rows_box_y: int) -> None:
        if self.drag_source_row is None or not self.drag_source_row.can_reorder():
            self.clear_drop_slot()
            return

        unlocked_rows = [
            (index, row)
            for index, row in enumerate(self.rows)
            if not row.entry.locked and row is not self.drag_source_row
        ]
        if not unlocked_rows:
            self.clear_drop_slot()
            return

        insert_index = unlocked_rows[-1][0] + 1
        for row_index, row in unlocked_rows:
            translated = row.translate_coordinates(
                self.rows_box,
                0,
                row.get_allocated_height() // 2,
            )
            if translated is None:
                continue
            _row_x, midpoint = translated
            if rows_box_y < midpoint:
                insert_index = row_index
                break

        if not self.can_drop_at_index(insert_index):
            self.clear_drop_slot()
            return
        self.show_drop_marker(insert_index)

    def end_drag(self) -> None:
        self.clear_drop_slot()
        if self.drag_ghost is not None:
            self.drag_layer.remove(self.drag_ghost)
            self.drag_ghost.destroy()
            self.drag_ghost = None
        self.drag_source_row = None

    def can_drop_at_index(self, insert_index: int) -> bool:
        if self.drag_source_row is None or not self.drag_source_row.can_reorder():
            return False
        entries = self.selected_entries()
        source_index = next(
            (
                index
                for index, entry in enumerate(entries)
                if entry.timezone == self.drag_source_row.timezone_name
            ),
            None,
        )
        if source_index is None:
            return False
        effective_index = insert_index - 1 if source_index < insert_index else insert_index
        return effective_index != source_index

    def clear_drop_slot(self) -> None:
        if self.active_drop_index is None and self.insertion_marker.get_parent() is None:
            return
        if self.insertion_marker.get_parent() is self.rows_box:
            self.rows_box.remove(self.insertion_marker)
        self.insertion_marker.hide()
        self.active_drop_index = None

    def show_drop_marker(self, insert_index: int) -> None:
        if self.active_drop_index == insert_index and self.insertion_marker.get_parent() is self.rows_box:
            return

        children = [child for child in self.rows_box.get_children() if child is not self.insertion_marker]
        if insert_index < len(self.rows):
            marker_position = children.index(self.rows[insert_index])
        else:
            marker_position = len(children)

        if self.insertion_marker.get_parent() is self.rows_box:
            self.rows_box.remove(self.insertion_marker)
        self.rows_box.pack_start(self.insertion_marker, True, True, 0)
        self.rows_box.reorder_child(self.insertion_marker, marker_position)
        self.insertion_marker.show()
        self.rows_box.queue_resize()
        self.active_drop_index = insert_index

    def set_drag_ghost_position(self, root_y: float) -> None:
        if self.drag_ghost is None:
            return
        ghost_y = int(round(self.drag_start_row_top_y + (root_y - self.drag_start_root_y)))
        self.drag_layer.move(
            self.drag_ghost,
            int(round(self.drag_row_overlay_x)),
            ghost_y,
        )
        self.drag_ghost.queue_draw()

    def update_row_separators(self) -> None:
        show_separators = not (self.edit_mode and self.config.sort_mode == "manual")
        for separator in self.row_separators:
            separator.set_visible(show_separators)

    def current_time_format(self) -> str:
        return effective_time_format(self.config.time_format)

    def display_time(self, value: datetime) -> str:
        return format_display_time(value, self.current_time_format())

    def time_entry_placeholder(self) -> str:
        if self.current_time_format() == "ampm":
            return "h:mm AM"
        return "HH:MM"

    def should_rebuild_time_sorted_rows(self) -> bool:
        return (
            self.config.sort_mode == "time"
            and self.editing_row is None
            and not self.edit_mode
        )

    def update_mode_button(self) -> None:
        context = self.live_button.get_style_context()
        if self.live:
            self.live_button.set_sensitive(False)
            self.live_button.set_tooltip_text("Clocks are live.")
            context.remove_class("active")
        else:
            self.live_button.set_sensitive(True)
            self.live_button.set_tooltip_text("Return to the current time.")
            context.add_class("active")

    def update_edit_mode(self) -> None:
        if self.edit_mode:
            self.edit_button.set_tooltip_text("Hide timezone management controls.")
            self.edit_button.get_style_context().add_class("active")
            self.sort_row.show()
            for child in self.sort_row.get_children():
                child.show()
            self.footer_separator.show()
            self.add_stack.show()
            visible_child = self.add_stack.get_visible_child_name()
            if visible_child == "panel":
                self.add_panel.show_all()
                if not self.add_entry.get_text().strip():
                    self.search_results_scroller.hide()
            else:
                self.add_toggle_button.show()
        else:
            self.edit_button.set_tooltip_text("Show sort, add, and reorder controls.")
            self.edit_button.get_style_context().remove_class("active")
            self.set_add_panel_visible(False)
            self.sort_row.hide()
            self.footer_separator.hide()
            self.add_stack.hide()

        for row in self.rows:
            row.set_edit_mode(self.edit_mode)
        self.update_row_separators()

    def focus_add_entry(self) -> bool:
        self.add_entry.grab_focus()
        return False

    def set_add_panel_visible(self, visible: bool) -> None:
        if visible:
            self.add_panel.show_all()
            if not self.add_entry.get_text().strip():
                self.search_results_scroller.hide()
            self.add_stack.set_visible_child_name("panel")
            GLib.idle_add(self.focus_add_entry)
        else:
            self.add_entry.set_text("")
            self.clear_search_results()
            self.add_stack.set_visible_child_name("toggle")

    def clear_search_results(self) -> None:
        for child in list(self.search_results_box.get_children()):
            self.search_results_box.remove(child)
        self.search_results = []
        self.search_results_scroller.hide()

    def render_search_results(self) -> None:
        self.clear_search_results()
        seen_timezones: set[str] = set()
        self.search_results = []
        for match in [*self.local_search_results, *self.remote_search_results]:
            if match.timezone in seen_timezones:
                continue
            seen_timezones.add(match.timezone)
            self.search_results.append(match)
            if len(self.search_results) >= SEARCH_RESULT_LIMIT:
                break

        if not self.search_results:
            return

        for match in self.search_results:
            button = Gtk.Button()
            button.set_halign(Gtk.Align.FILL)
            button.set_hexpand(True)
            button.get_style_context().add_class("search-result-button")
            button.connect("clicked", self.on_search_result_clicked, match)

            content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            content.set_halign(Gtk.Align.START)

            title = Gtk.Label(xalign=0)
            title.set_text(match.title)
            title.get_style_context().add_class("search-result-title")
            content.pack_start(title, False, False, 0)

            meta = Gtk.Label(xalign=0)
            meta.set_text(match.subtitle)
            meta.get_style_context().add_class("search-result-meta")
            content.pack_start(meta, False, False, 0)

            button.add(content)
            self.search_results_box.pack_start(button, False, False, 0)

        self.search_results_box.show_all()
        self.search_results_scroller.show()

    def cancel_remote_search(self) -> None:
        if self.remote_search_source is not None:
            GLib.source_remove(self.remote_search_source)
            self.remote_search_source = None

    def begin_remote_search(self, generation: int, query: str) -> bool:
        self.remote_search_source = None
        worker = threading.Thread(
            target=self.run_remote_search,
            args=(generation, query),
            daemon=True,
        )
        worker.start()
        return False

    def run_remote_search(self, generation: int, query: str) -> None:
        results = self.place_search.search(query, limit=SEARCH_RESULT_LIMIT)
        GLib.idle_add(self.apply_remote_search_results, generation, query, results)

    def apply_remote_search_results(
        self,
        generation: int,
        query: str,
        results: list[TimezoneSearchResult],
    ) -> bool:
        if generation != self.search_generation:
            return False
        if self.add_entry.get_text().strip() != query:
            return False
        self.remote_search_results = results
        self.render_search_results()
        return False

    def update_search_results(self) -> None:
        query = self.add_entry.get_text().strip()
        self.cancel_remote_search()
        self.search_generation += 1
        self.remote_search_results = []

        if not query:
            self.local_search_results = []
            self.render_search_results()
            return

        self.local_search_results = self.resolver.search(query, limit=SEARCH_RESULT_LIMIT)
        self.render_search_results()

        if self.local_search_results:
            return

        if len(TimezoneResolver._normalize(query)) < 3:
            return

        self.remote_search_source = GLib.timeout_add(
            250,
            self.begin_remote_search,
            self.search_generation,
            query,
        )

    def show_status(self, message: str, error: bool = False) -> None:
        self.status_label.set_text(message)
        self.status_label.show()
        context = self.status_label.get_style_context()
        if error:
            context.add_class("error")
        else:
            context.remove_class("error")

        if self.status_clear_source is not None:
            GLib.source_remove(self.status_clear_source)
            self.status_clear_source = None

        if not error:
            self.status_clear_source = GLib.timeout_add_seconds(4, self.clear_status)

    def clear_status(self) -> bool:
        self.status_label.set_text("")
        self.status_label.hide()
        self.status_label.get_style_context().remove_class("error")
        self.status_clear_source = None
        return False

    def schedule_live_apply(self, row: ClockRow) -> None:
        if self.pending_apply_source is not None:
            GLib.source_remove(self.pending_apply_source)
        self.pending_apply_row = row
        self.pending_apply_source = GLib.timeout_add(120, self.run_pending_apply)

    def run_pending_apply(self) -> bool:
        row = self.pending_apply_row
        self.pending_apply_source = None
        self.pending_apply_row = None
        if row is not None:
            self.apply_manual_entry(row, show_errors=False)
        return False

    def flush_live_apply(self, row: ClockRow, show_errors: bool) -> bool:
        if self.pending_apply_source is not None and self.pending_apply_row is row:
            GLib.source_remove(self.pending_apply_source)
            self.pending_apply_source = None
            self.pending_apply_row = None
        return self.apply_manual_entry(row, show_errors=show_errors)

    def apply_manual_entry(self, row: ClockRow, show_errors: bool) -> bool:
        if not row.dirty:
            return False

        try:
            parsed_reference = parse_manual_reference_details(
                row.time_entry.get_text(),
                row.timezone_name,
                self.reference_utc,
            )
        except ValueError as exc:
            if show_errors:
                row.set_error(True)
                self.show_status(str(exc), error=True)
            return False

        self.reference_utc = parsed_reference.reference_utc
        if show_errors:
            row.suppress_changes = True
            row.time_entry.set_text(
                self.display_time(zoned_datetime(parsed_reference.reference_utc, row.timezone_name))
            )
            row.time_entry.set_position(-1)
            row.suppress_changes = False

        self.live = False
        for clock_row in self.rows:
            clock_row.dirty = clock_row is row and row.time_entry.is_focus() and not show_errors
            clock_row.set_error(False)
        # Defer reordering until the user leaves the active entry so the widget
        # instance holding focus is not destroyed mid-edit.
        if self.should_rebuild_time_sorted_rows():
            self.rebuild_rows()
        else:
            self.refresh_rows()
        return True

    def remove_timezone(self, timezone_name: str) -> None:
        removed_label = next(
            (entry.display_label() for entry in self.config.timezones if entry.timezone == timezone_name),
            timezone_name,
        )
        self.config = self.config_manager.remove_timezone(timezone_name)
        self.show_status(f"Removed {removed_label}.")
        self.rebuild_rows()

    def add_timezone(self, timezone_name: str, label: str = "") -> None:
        if timezone_name in {entry.timezone for entry in self.config.timezones}:
            self.show_status(f"{label or timezone_name} is already in the list.", error=True)
            return

        self.config = self.config_manager.add_timezone(timezone_name, label=label)
        self.set_add_panel_visible(False)
        self.show_status(f"Added {label or timezone_name}.")
        self.rebuild_rows()

    def move_timezone(self, timezone_name: str, offset: int) -> None:
        if self.config.sort_mode != "manual":
            return
        self.config = self.config_manager.move_timezone(timezone_name, offset)
        self.rebuild_rows()

    def reorder_timezone(
        self,
        timezone_name: str,
        target_timezone_name: str,
        place_after: bool = False,
    ) -> bool:
        if self.config.sort_mode != "manual":
            return False
        source_entry = next(
            (entry for entry in self.config.timezones if entry.timezone == timezone_name),
            None,
        )
        target_entry = next(
            (entry for entry in self.config.timezones if entry.timezone == target_timezone_name),
            None,
        )
        if source_entry is None or target_entry is None:
            return False
        if source_entry.locked or target_entry.locked:
            return False
        updated = self.config_manager.reorder_timezone(
            timezone_name,
            target_timezone_name,
            place_after=place_after,
        )
        if updated.timezones == self.config.timezones:
            return False
        self.config = updated
        self.rebuild_rows()
        return True

    def reorder_timezone_to_index(self, timezone_name: str, insert_index: int) -> bool:
        entries = self.selected_entries()
        if not entries:
            return False
        if insert_index < 0 or insert_index > len(entries):
            return False
        if insert_index == len(entries):
            target_timezone_name = entries[-1].timezone
            place_after = True
        else:
            target_timezone_name = entries[insert_index].timezone
            place_after = False
        return self.reorder_timezone(
            timezone_name,
            target_timezone_name,
            place_after=place_after,
        )


    def on_tick(self) -> bool:
        if self.live:
            self.reference_utc = datetime.now(timezone.utc)
            if self.should_rebuild_time_sorted_rows():
                self.rebuild_rows()
            else:
                self.refresh_rows()
        return True

    def on_now_clicked(self, *_args: object) -> None:
        self.live = True
        self.reference_utc = datetime.now(timezone.utc)
        if self.pending_apply_source is not None:
            GLib.source_remove(self.pending_apply_source)
            self.pending_apply_source = None
            self.pending_apply_row = None
        for row in self.rows:
            row.dirty = False
            row.set_error(False)
        if self.config.sort_mode == "time":
            self.rebuild_rows()
        else:
            self.refresh_rows()

    def on_toggle_edit_mode(self, *_args: object) -> None:
        self.cancel_pointer_drag()
        self.edit_mode = not self.edit_mode
        self.update_edit_mode()
        if not self.edit_mode and self.config.sort_mode == "time":
            self.rebuild_rows()

    def on_toggle_add_panel(self, *_args: object) -> None:
        current = self.add_stack.get_visible_child_name()
        self.set_add_panel_visible(current != "panel")

    def on_search_query_changed(self, *_args: object) -> None:
        self.update_search_results()

    def on_search_result_clicked(
        self,
        _button: Gtk.Button,
        match: TimezoneSearchResult,
    ) -> None:
        self.add_timezone(match.timezone, label=match.title)

    def on_add_timezone(self, *_args: object) -> None:
        raw_value = self.add_entry.get_text().strip()
        timezone_name = self.resolver.resolve(raw_value)
        if not timezone_name:
            visible_match = self.single_visible_search_match(raw_value)
            if visible_match is not None:
                self.add_timezone(visible_match.timezone, label=visible_match.title)
                return
            if self.search_results:
                self.show_status("Pick one of the matching timezones below.", error=True)
            else:
                self.show_status(
                    "Enter a valid timezone, place, or abbreviation like IST.",
                    error=True,
                )
            return
        self.add_timezone(timezone_name, label=self.label_for_input(raw_value, timezone_name))

    def label_for_input(self, raw_value: str, timezone_name: str) -> str:
        value = raw_value.strip()
        if not value:
            return ""
        if value.casefold() == timezone_name.casefold():
            return value
        if value.replace("_", " ").casefold() == timezone_name.replace("_", " ").casefold():
            return value
        matches = self.resolver.search(value, limit=1)
        if matches and matches[0].timezone == timezone_name:
            return matches[0].title
        return value

    def single_visible_search_match(self, raw_value: str) -> TimezoneSearchResult | None:
        if len(self.search_results) != 1:
            return None
        normalized_value = TimezoneResolver._normalize(raw_value)
        if not normalized_value:
            return None
        match = self.search_results[0]
        if TimezoneResolver._normalize(match.title).startswith(normalized_value):
            return match
        return None

    def on_sort_mode_changed(self, combo: Gtk.ComboBoxText) -> None:
        sort_mode = combo.get_active_id() or "manual"
        self.config = self.config_manager.set_sort_mode(sort_mode)
        self.rebuild_rows()

    def toggle_timezone_lock(self, timezone_name: str) -> None:
        entry = next(
            (candidate for candidate in self.config.timezones if candidate.timezone == timezone_name),
            None,
        )
        if entry is None:
            return
        self.config = self.config_manager.set_timezone_locked(timezone_name, not entry.locked)
        self.rebuild_rows()

    def on_time_format_changed(self, combo: Gtk.ComboBoxText) -> None:
        time_format = combo.get_active_id() or "system"
        self.config = self.config_manager.set_time_format(time_format)
        self.rebuild_rows()

    def arm_dismissal(self) -> bool:
        self.dismiss_armed = True
        return False

    def event_targets_panel(self, event: Gdk.EventButton) -> bool:
        event_widget = Gtk.get_event_widget(event)
        current = event_widget
        while current is not None:
            if current is self.panel:
                return True
            current = current.get_parent()

        if self.root is None:
            return False

        translated = self.panel.translate_coordinates(self.root, 0, 0)
        if translated is None:
            return False

        panel_x, panel_y = translated
        allocation = self.panel.get_allocation()
        inside_x = panel_x <= event.x <= panel_x + allocation.width
        inside_y = panel_y <= event.y <= panel_y + allocation.height
        return inside_x and inside_y

    def on_root_button_press(self, _widget: Gtk.Widget, event: Gdk.EventButton) -> bool:
        if self.drag_pending_row is not None:
            return True
        if not self.dismiss_armed:
            return False

        if self.event_targets_panel(event):
            return False

        self.close()
        return True

    def on_root_motion_notify(self, _widget: Gtk.Widget, event: Gdk.EventMotion) -> bool:
        return self.update_pointer_drag(event.x_root, event.y_root)

    def on_root_button_release(self, _widget: Gtk.Widget, event: Gdk.EventButton) -> bool:
        if event.button != Gdk.BUTTON_PRIMARY or self.drag_pending_row is None:
            return False
        self.finish_pointer_drag(event.y_root, commit=True)
        return True

    def on_focus_out(self, *_args: object) -> bool:
        if self.dismiss_armed:
            self.close()
        return False

    def on_key_press(self, _widget: Gtk.Widget, event: Gdk.EventKey) -> bool:
        if event.keyval == Gdk.KEY_Escape:
            self.close()
            return True
        return False

    def on_delete(self, *_args: object) -> bool:
        Gtk.main_quit()
        return False

    def on_destroy(self, *_args: object) -> None:
        Gtk.main_quit()


def run_popup(pid_path: Path, config_path: Path | None = None) -> None:
    apply_css()
    pid_path.parent.mkdir(parents=True, exist_ok=True)
    pid_path.write_text(str(os.getpid()), encoding="utf-8")

    def cleanup() -> None:
        try:
            if pid_path.exists():
                pid_path.unlink()
        except FileNotFoundError:
            pass

    atexit.register(cleanup)

    def handle_signal(*_args: object) -> None:
        GLib.idle_add(Gtk.main_quit)

    signal.signal(signal.SIGTERM, handle_signal)
    signal.signal(signal.SIGINT, handle_signal)

    window = WorldClockWindow(pid_path=pid_path, config_path=config_path)
    Gtk.main()
    _ = window
