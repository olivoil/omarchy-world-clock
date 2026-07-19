use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub const MODULE_ID: &str = "world-clock";
const LEGACY_MODULE_IDS: [&str; 3] = [MODULE_ID, "custom/world-clock", "custom/world-clock-rs"];

fn entry_id(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(id) => Some(id),
        Value::Object(object) => object.get("id").and_then(Value::as_str),
        _ => None,
    }
}

fn is_world_clock_entry(entry: &Value) -> bool {
    entry_id(entry).is_some_and(|id| LEGACY_MODULE_IDS.contains(&id))
}

pub fn contains_module(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|root| root.pointer("/bar/layout").cloned())
        .and_then(|layout| layout.as_object().cloned())
        .is_some_and(|layout| {
            layout.values().any(|section| {
                section
                    .as_array()
                    .is_some_and(|entries| entries.iter().any(is_world_clock_entry))
            })
        })
}

fn render(root: &Value) -> Result<String> {
    let mut rendered =
        serde_json::to_string_pretty(root).context("failed to serialize Omarchy shell config")?;
    rendered.push('\n');
    Ok(rendered)
}

fn normalized_root(text: &str) -> Result<Value> {
    let root: Value = serde_json::from_str(text).context("failed to parse Omarchy shell config")?;
    if !root.is_object() {
        bail!("Omarchy shell config must contain a JSON object");
    }
    Ok(root)
}

fn layout_mut(root: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    let root = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Omarchy shell config must contain a JSON object"))?;
    let bar = root.entry("bar").or_insert_with(|| json!({}));
    let bar = bar
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Omarchy shell bar config must contain a JSON object"))?;
    let layout = bar.entry("layout").or_insert_with(|| json!({}));
    layout
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Omarchy shell bar layout must contain a JSON object"))
}

fn remove_existing_entries(layout: &mut serde_json::Map<String, Value>) {
    for section in ["left", "center", "right"] {
        if let Some(entries) = layout.get_mut(section).and_then(Value::as_array_mut) {
            entries.retain(|entry| !is_world_clock_entry(entry));
        }
    }
}

pub fn patch_config_text(text: &str, command_path: &str) -> Result<String> {
    let mut root = normalized_root(text)?;
    let layout = layout_mut(&mut root)?;
    remove_existing_entries(layout);

    let center = layout.entry("center").or_insert_with(|| json!([]));
    let center = center
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("Omarchy shell center layout must contain a JSON array"))?;
    let insert_at = center
        .iter()
        .position(|entry| entry_id(entry) == Some("omarchy.clock"))
        .map_or(0, |index| index + 1);

    center.insert(
        insert_at,
        json!({
            "id": MODULE_ID,
            "type": "command",
            "exec": format!("{command_path} module"),
            "interval": 2,
            "onClick": format!("{command_path} toggle"),
            "onRightClick": "omarchy-menu-timezone"
        }),
    );

    render(&root)
}

pub fn unpatch_config_text(text: &str) -> Result<String> {
    let mut root = normalized_root(text)?;
    remove_existing_entries(layout_mut(&mut root)?);
    render(&root)
}

#[cfg(test)]
mod tests {
    use super::{contains_module, patch_config_text, unpatch_config_text};
    use serde_json::Value;

    const SHELL_CONFIG: &str = r#"{
  "version": 1,
  "bar": {
    "centerAnchor": "omarchy.clock",
    "layout": {
      "left": [{ "id": "omarchy.menu" }],
      "center": [
        { "id": "omarchy.clock", "format": "HH:mm" },
        { "id": "omarchy.weather" },
        { "id": "omarchy.system-update" }
      ],
      "right": [{ "id": "omarchy.tray" }]
    }
  },
  "plugins": []
}"#;

    #[test]
    fn installs_command_module_after_clock() {
        let patched = patch_config_text(SHELL_CONFIG, "/usr/bin/omarchy-world-clock").unwrap();
        let root: Value = serde_json::from_str(&patched).unwrap();
        let center = root
            .pointer("/bar/layout/center")
            .unwrap()
            .as_array()
            .unwrap();

        assert_eq!(center[0]["id"], "omarchy.clock");
        assert_eq!(center[1]["id"], "world-clock");
        assert_eq!(center[1]["type"], "command");
        assert_eq!(center[1]["exec"], "/usr/bin/omarchy-world-clock module");
        assert_eq!(center[1]["onRightClick"], "omarchy-menu-timezone");
        assert_eq!(center[2]["id"], "omarchy.weather");
        assert!(contains_module(&patched));
    }

    #[test]
    fn reinstall_updates_command_without_duplicating_module() {
        let first = patch_config_text(SHELL_CONFIG, "/old/omarchy-world-clock").unwrap();
        let second = patch_config_text(&first, "/new/omarchy-world-clock").unwrap();
        let root: Value = serde_json::from_str(&second).unwrap();
        let center = root
            .pointer("/bar/layout/center")
            .unwrap()
            .as_array()
            .unwrap();
        let entries = center
            .iter()
            .filter(|entry| entry["id"] == "world-clock")
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["exec"], "/new/omarchy-world-clock module");
    }

    #[test]
    fn uninstall_removes_current_and_legacy_entries() {
        let config = r#"{
          "bar": {
            "layout": {
              "left": ["custom/world-clock-rs"],
              "center": [
                {"id":"omarchy.clock"},
                {"id":"world-clock","type":"command"},
                {"id":"omarchy.weather"}
              ],
              "right": [{"id":"custom/world-clock"}]
            }
          }
        }"#;

        let unpatched = unpatch_config_text(config).unwrap();
        assert!(!contains_module(&unpatched));
        assert!(!unpatched.contains("custom/world-clock"));
        assert!(unpatched.contains("omarchy.clock"));
        assert!(unpatched.contains("omarchy.weather"));
    }
}
