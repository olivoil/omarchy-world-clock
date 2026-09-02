import QtQuick

// Panel key dispatcher with direct-input support: printable keys can be routed
// to the read-mode time editor or add-mode search before either field owns
// focus. This priority also keeps h/j/k/l/x intact as query characters.
Item {
  id: root

  property bool blocked: false
  property bool directTextInput: false

  signal moveRequested(int dx, int dy)
  signal activateRequested()
  signal closeRequested()
  signal deleteRequested()
  signal tabRequested(int direction)
  signal textKey(string text)

  function textForEvent(event) {
    var value = String(event.text || "")
    // Some keyboard/numpad paths expose a numeric Qt key without printable
    // text. Preserve those digits for direct time entry as well.
    if (!value && event.key >= Qt.Key_0 && event.key <= Qt.Key_9)
      value = String(event.key - Qt.Key_0)
    return value
  }

  focus: true
  Keys.priority: Keys.BeforeItem
  Keys.onPressed: function(event) {
    if (blocked) return

    var typedText = root.textForEvent(event)

    if (event.key === Qt.Key_Escape) {
      closeRequested(); event.accepted = true; return
    }
    if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
      tabRequested((event.modifiers & Qt.ShiftModifier)
        || event.key === Qt.Key_Backtab ? -1 : 1)
      event.accepted = true
      return
    }

    var commandModifiers = Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier
    if (directTextInput && typedText.length === 1
        && typedText.trim() !== ""
        && !(event.modifiers & commandModifiers)) {
      textKey(typedText)
      event.accepted = true
      return
    }

    if (event.key === Qt.Key_Down || typedText === "j") {
      moveRequested(0, 1); event.accepted = true; return
    }
    if (event.key === Qt.Key_Up || typedText === "k") {
      moveRequested(0, -1); event.accepted = true; return
    }
    if (event.key === Qt.Key_Right || typedText === "l") {
      moveRequested(1, 0); event.accepted = true; return
    }
    if (event.key === Qt.Key_Left || typedText === "h") {
      moveRequested(-1, 0); event.accepted = true; return
    }
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      activateRequested(); event.accepted = true; return
    }
    if (event.key === Qt.Key_Space) {
      activateRequested(); event.accepted = true; return
    }
    if (typedText === "x" || typedText === "X") {
      deleteRequested(); event.accepted = true; return
    }
    if (typedText.length === 1)
      textKey(typedText)
  }
}
