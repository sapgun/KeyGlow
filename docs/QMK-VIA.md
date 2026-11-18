# QMK / VIA keymap import and export (S9)

KeyGlow is a Windows keyboard control app, not a firmware tool. It can
read and write QMK/VIA keymap files so a profile can start from (or
share) a QMK layout — it cannot flash firmware, and it never tries to.

## What import does

Import replaces the profile's **base keymap** (disabled keys and
remap rules, taken from layer 0) and its **layers**. Macros are
untouched. The import is previewed in the dialog before anything is
applied; cancelling leaves the profile unchanged. The profile's
layout must match the keymap's layer length, otherwise the import is
refused.

## QMK keymap.json

Format: `{"keyboard", "keymap", "layout": "LAYOUT_all", "layers": [[...]]}`.
Each layer is one keycode per key position, in the layout's position
order (row-major, `(y, x)` sorted — the frontend computes this from
the layout registry; the backend never knows a layout's geometry).

- `XXXXXXX` is `KC_NO`: the position is disabled in that layer.
- `_______` is `KC_TRNS`: the position inherits the key below it.
  At layer 0 there is nothing below, so a `KC_TRNS` there means no
  rule for that position.
- `KC_MO(n)` / `KC_TG(n)` make the position the layer's switch key
  (hold / toggle). Rules:
  - A layer above 0 without a switch key is refused.
  - A switch key that only appears inside its own layer is refused
    (unreachable — nothing can ever activate it).
  - `MO(0)` / `TG(0)` are refused.
  - Two different switch keys on one layer are refused.
- A plain `KC_X` at a position whose key is `P` becomes a remap
  `P → X` when `X` differs from `P`; when they are the same, no
  rule is created.
- The QMK keycode table covers the basic `KC_*` set plus the
  Japanese keys (`KC_RO` → IntlRo, `KC_JYEN` → IntlYen,
  `KC_HENK` → Convert, `KC_MHEN` → NonConvert, `KC_KANA` →
  KanaMode, `KC_NUHS` → IntlBackslash, `KC_INT1`–`KC_INT5`
  aliases). `KC_NUHS` (0x32) is the same physical key as Backslash
  (scan 0x2B); `KC_NUBS` (0x64) is IntlBackslash (scan 0x56).

Refused, with a reason:

- `TO` / `DF` / `OSL` / `LT` / `LM` / `TT` / `MT` / mod-tap and
  other compound keycodes — KeyGlow has no layer-tap or
  one-shot model.
- QMK macros — they are C code, not JSON, so there is nothing to
  import. Define the same sequence as a KeyGlow macro instead.
- Unknown `KC_*` names, wrong layer length, a layout that does not
  match the profile, files over 64 KiB, or more than 9 layers.

## VIA JSON

Format: `{"layers": [["0x0004", ...]]}` — hex keycodes per position,
same position order as QMK. Only basic keys are supported (HID
usage `0x04`–`0xE7` → KeyCode, `0x0000` = `KC_NO`, `0x0001` =
`KC_TRNS`). 16-bit VIA layer keycodes (`MO` / `TG` and friends) are
refused: their values vary by QMK version and cannot be resolved
reliably here. Use a QMK keymap.json for layers instead.

## Export

- QMK: the base disabled keys become `KC_NO`, remaps become their
  target keycode, and a switch key position carries `KC_MO(n)` /
  `KC_TG(n)` on every layer except its own, where it is `KC_TRNS`.
  Positions with no rule are `KC_TRNS`.
- VIA: only for profiles without layers. A profile with layers is
  refused with a message pointing at QMK export instead.

## Out of scope

Flashing QMK firmware to a keyboard, reading a keymap back from a
keyboard, VIA's live remapping protocol, and any QMK feature the
tables above do not name. If a keymap needs one of those, edit it
in QMK Configurator / VIA first, then import the resulting JSON.
