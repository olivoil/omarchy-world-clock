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

  focus: true
  Keys.priority: Keys.BeforeItem
  Keys.onPressed: function(event) {
    if (blocked) return

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
    if (directTextInput && event.text && event.text.length === 1
        && event.text.trim() !== ""
        && !(event.modifiers & commandModifiers)) {
      textKey(event.text)
      event.accepted = true
      return
    }

    if (event.key === Qt.Key_Down || event.text === "j") {
      moveRequested(0, 1); event.accepted = true; return
    }
    if (event.key === Qt.Key_Up || event.text === "k") {
      moveRequested(0, -1); event.accepted = true; return
    }
    if (event.key === Qt.Key_Right || event.text === "l") {
      moveRequested(1, 0); event.accepted = true; return
    }
    if (event.key === Qt.Key_Left || event.text === "h") {
      moveRequested(-1, 0); event.accepted = true; return
    }
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      activateRequested(); event.accepted = true; return
    }
    if (event.key === Qt.Key_Space) {
      activateRequested(); event.accepted = true; return
    }
    if (event.text === "x" || event.text === "X") {
      deleteRequested(); event.accepted = true; return
    }
    if (event.text && event.text.length === 1)
      textKey(event.text)
  }
}
