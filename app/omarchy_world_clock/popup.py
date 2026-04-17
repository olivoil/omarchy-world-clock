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
)
from .core import (
    all_timezones,
    format_offset,
    parse_manual_reference,
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
  padding: 18px;
  box-shadow: 0 18px 36px {rgba("#000000", 0.30)};
}}

.panel-title {{
  color: {palette.foreground};
  font-weight: 700;
  font-size: 18px;
}}

.panel-subtitle,
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

button.suggested-action {{
  background: {rgba(palette.accent, 0.15)};
  border-color: {rgba(palette.accent, 0.45)};
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

button.icon-button.active {{
  background: {rgba(palette.accent, 0.10)};
  border-color: {rgba(palette.accent, 0.30)};
}}

button.remove-button {{
  min-width: 34px;
  min-height: 34px;
  padding: 0;
  font-size: 13px;
}}

button.move-button {{
  min-width: 28px;
  min-height: 16px;
  padding: 0;
  font-size: 11px;
}}

button.move-button:disabled,
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
        removable: bool,
        manual_sort: bool,
        can_move_up: bool,
        can_move_down: bool,
    ) -> None:
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=16)
        self.window = window
        self.entry = entry
        self.timezone_name = entry.timezone
        self.removable = removable
        self.suppress_changes = False
        self.dirty = False
        self.current_zoned = zoned_datetime(window.reference_utc, self.timezone_name)

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

        self.time_entry = Gtk.Entry()
        self.time_entry.set_alignment(1.0)
        self.time_entry.set_width_chars(10)
        self.time_entry.set_max_length(19)
        self.time_entry.set_placeholder_text("HH:MM")
        self.time_entry.get_style_context().add_class("time-entry")
        self.time_entry.connect("focus-in-event", self.on_focus_in)
        self.time_entry.connect("focus-out-event", self.on_focus_out)
        self.time_entry.connect("changed", self.on_changed)
        self.time_entry.connect("activate", self.on_activate)
        controls.pack_start(self.time_entry, False, False, 0)

        self.move_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.move_box.set_no_show_all(True)
        self.move_up_button = Gtk.Button(label="˄")
        self.move_up_button.get_style_context().add_class("move-button")
        self.move_up_button.set_sensitive(manual_sort and can_move_up)
        self.move_up_button.set_tooltip_text("Move timezone up")
        self.move_up_button.connect("clicked", self.on_move_up)
        self.move_box.pack_start(self.move_up_button, False, False, 0)

        self.move_down_button = Gtk.Button(label="˅")
        self.move_down_button.get_style_context().add_class("move-button")
        self.move_down_button.set_sensitive(manual_sort and can_move_down)
        self.move_down_button.set_tooltip_text("Move timezone down")
        self.move_down_button.connect("clicked", self.on_move_down)
        self.move_box.pack_start(self.move_down_button, False, False, 0)
        controls.pack_start(self.move_box, False, False, 0)

        self.remove_button = Gtk.Button(label="x")
        self.remove_button.set_no_show_all(True)
        self.remove_button.get_style_context().add_class("remove-button")
        self.remove_button.set_sensitive(removable)
        if removable:
            self.remove_button.set_tooltip_text("Remove timezone")
            self.remove_button.connect("clicked", self.on_remove)
        else:
            self.remove_button.set_tooltip_text("Local timezone")
        controls.pack_start(self.remove_button, False, False, 0)

        self.pack_start(controls, False, False, 0)
        self.set_edit_mode(window.edit_mode)
        self.refresh(window.reference_utc)

    def set_edit_mode(self, enabled: bool) -> None:
        if enabled:
            self.move_box.show()
            self.move_up_button.show()
            self.move_down_button.show()
            self.remove_button.show()
        else:
            self.move_box.hide()
            self.remove_button.hide()

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
        self.time_entry.set_text(self.current_zoned.strftime("%H:%M"))
        self.suppress_changes = False
        self.dirty = False

    def set_error(self, enabled: bool) -> None:
        context = self.time_entry.get_style_context()
        if enabled:
            context.add_class("error")
        else:
            context.remove_class("error")

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

    def on_move_up(self, *_args: object) -> None:
        self.window.move_timezone(self.timezone_name, -1)

    def on_move_down(self, *_args: object) -> None:
        self.window.move_timezone(self.timezone_name, 1)


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
        self.local_search_results: list[TimezoneSearchResult] = []
        self.remote_search_results: list[TimezoneSearchResult] = []
        self.search_results: list[TimezoneSearchResult] = []
        self.search_generation = 0
        self.remote_search_source: int | None = None
        self.status_clear_source: int | None = None
        self.pending_apply_source: int | None = None
        self.pending_apply_row: ClockRow | None = None
        self.editing_row: ClockRow | None = None
        self.dismiss_armed = False
        self.edit_mode = False

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
        GtkLayerShell.set_margin(self, GtkLayerShell.Edge.TOP, 32)

    def build_ui(self) -> None:
        root = Gtk.EventBox()
        root.set_visible_window(False)
        root.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        root.connect("button-press-event", self.on_root_button_press)
        self.add(root)

        layout = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        layout.set_vexpand(True)
        layout.set_hexpand(True)
        root.add(layout)

        top_band = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        top_band.set_halign(Gtk.Align.CENTER)
        top_band.set_valign(Gtk.Align.START)
        top_band.set_margin_top(8)
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
        header.pack_start(titles, True, True, 0)

        title = Gtk.Label(xalign=0)
        title.set_text("World Clock")
        title.get_style_context().add_class("panel-title")
        titles.pack_start(title, False, False, 0)

        subtitle = Gtk.Label(xalign=0)
        subtitle.set_text("Type HH:MM in any clock to convert every row.")
        subtitle.get_style_context().add_class("panel-subtitle")
        titles.pack_start(subtitle, False, False, 0)

        header_actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        header.pack_start(header_actions, False, False, 0)

        self.now_button = Gtk.Button(label="Now")
        self.now_button.get_style_context().add_class("suggested-action")
        self.now_button.connect("clicked", self.on_now_clicked)
        header_actions.pack_start(self.now_button, False, False, 0)

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

        self.rows_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.rows_box.set_margin_top(8)
        panel.pack_start(self.rows_box, False, False, 0)

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
        local_entry = TimezoneEntry(timezone=self.local_timezone)
        extras: list[TimezoneEntry] = []
        seen = {self.local_timezone}
        for entry in self.config.timezones:
            if entry.timezone in seen:
                continue
            seen.add(entry.timezone)
            extras.append(entry)

        if self.config.sort_mode == "alpha":
            extras.sort(key=lambda entry: (entry.display_label().casefold(), entry.timezone.casefold()))
        elif self.config.sort_mode == "time":
            extras.sort(
                key=lambda entry: (
                    zoned_datetime(self.reference_utc, entry.timezone).replace(tzinfo=None),
                    entry.display_label().casefold(),
                )
            )
        return [local_entry, *extras]

    def rebuild_rows(self) -> None:
        for child in list(self.rows_box.get_children()):
            self.rows_box.remove(child)

        self.rows = []
        entries = self.selected_entries()
        extra_entries = entries[1:]
        manual_sort = self.config.sort_mode == "manual"
        for index, entry in enumerate(entries):
            extra_index = next(
                (position for position, extra in enumerate(extra_entries) if extra.timezone == entry.timezone),
                None,
            )
            row = ClockRow(
                self,
                entry,
                removable=entry.timezone != self.local_timezone,
                manual_sort=manual_sort,
                can_move_up=extra_index is not None and extra_index > 0,
                can_move_down=extra_index is not None and extra_index < len(extra_entries) - 1,
            )
            self.rows.append(row)
            self.rows_box.pack_start(row, False, False, 0)
            if index < len(entries) - 1:
                separator = Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL)
                self.rows_box.pack_start(separator, False, False, 0)

        self.refresh_rows()
        self.rows_box.show_all()
        self.update_edit_mode()

    def refresh_rows(self) -> None:
        for row in self.rows:
            row.refresh(self.reference_utc)
        self.update_mode_button()

    def should_rebuild_time_sorted_rows(self) -> bool:
        return self.config.sort_mode == "time" and self.editing_row is None

    def update_mode_button(self) -> None:
        if self.live:
            self.now_button.set_label("Now")
            self.now_button.set_tooltip_text("Clocks are live.")
        else:
            self.now_button.set_label("Reset")
            self.now_button.set_tooltip_text("Return to the current time.")

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
            self.reference_utc = parse_manual_reference(
                row.time_entry.get_text(),
                row.timezone_name,
                self.reference_utc,
            )
        except ValueError as exc:
            if show_errors:
                row.set_error(True)
                self.show_status(str(exc), error=True)
            return False

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
        if timezone_name == self.local_timezone:
            self.show_status(f"{timezone_name} is already your local clock.", error=True)
            return
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
        self.edit_mode = not self.edit_mode
        self.update_edit_mode()

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

    def arm_dismissal(self) -> bool:
        self.dismiss_armed = True
        return False

    def on_root_button_press(self, _widget: Gtk.Widget, event: Gdk.EventButton) -> bool:
        if not self.dismiss_armed:
            return False

        allocation = self.panel.get_allocation()
        inside_x = allocation.x <= event.x <= allocation.x + allocation.width
        inside_y = allocation.y <= event.y <= allocation.y + allocation.height
        if inside_x and inside_y:
            return False

        self.close()
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
