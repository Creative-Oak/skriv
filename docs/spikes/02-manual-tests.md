# Spike 2: manual test sheet

The tests from the brief ([`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md),
"Spike 2") that need a person, real devices and assistive technology. Fill
in a row per run; ✅ pass, ❌ fail (say what happened), ➖ not run.

**Setup:** on the Mac, `cd prototypes/view/web && pnpm dev` (see
[`prototypes/view/README.md`](../../prototypes/view/README.md)). Other devices
on the same network open `http://<Mac IP>:5180/`. Each page's panel shows
the input path (EditContext, textarea, contenteditable). Before each latency
run press **Reset**; after it press **Download JSON** and put the file in
`docs/spikes/02-results-data/manual/` named
`<device>-<browser>-<view>-<test>.json`.

**Scope decided 2026-11-18:** phones, Windows (NVDA, JAWS, Narrator,
Windows IMEs), VoiceOver and latency on the reference hardware are
**deferred** for this prototype. The tests in scope were run by hand in
Firefox and Safari on the Mac and worked well, apart from two bugs, both
fixed the same day: spaces typed at the end of a paragraph didn't move the
caret until the next character, and in Safari, view B (textarea) ignored
Backspace at the start of a paragraph, so paragraphs couldn't be joined.

Views: **A** = `a.html` (ProseMirror), **B** = `b.html` (EditContext where
available), **B-ta** = `b.html?input=textarea` (forced textarea, to test
B's Firefox/Safari path in Chrome too).

## 1. Keyboard and input methods

| # | Input | What to type | Check |
|---|---|---|---|
| K1 | Danish keyboard | `Æbleøen på Ålborg` | No lost or doubled characters |
| K2 | Dead keys | `´ e` → `é`, `¨ u` → `ü`, `^ o` → `ô`, `` ` a `` → `à` | Caret right after each composed character |
| K3 | AltGr / Option combinations | `@ £ $ € { } [ ] \|` | All arrive, once |
| K4 | macOS press-and-hold | hold `e`, pick `é` | The menu appears near the caret; the choice replaces `e` |
| K5 | macOS Japanese (Kana) | `nihon` → にほん → 日本 | Underline while composing; commit replaces the whole composition; Escape cancels |
| K6 | macOS Pinyin | `zhongwen` → 中文 | As K5 |
| K7 | Windows Japanese IME, Pinyin, emoji panel (Win+.) | as K5/K6; an emoji | As K5 |
| K8 | Android Gboard (Danish) | type with autocorrect on; swipe a word; tap a suggestion; long-press for `æ ø å`; backspace through a "composed" word | No duplicated or vanished words; caret stays put |
| K9 | iOS keyboard (Danish) | as K8, plus QuickType suggestions | As K8 |
| K10 | Dictation | macOS Dictation; Windows Voice Typing (Win+H); Gboard voice | Text arrives once, at the caret |
| K11 | Across page breaks | Shift+arrows and mouse drag from page 3 to page 4; Backspace; type; Cmd/Ctrl+Z | Selection continuous; nothing else changes; undo restores |

| Test | Device / OS | Browser | A | B | B-ta | Notes |
|---|---|---|---|---|---|---|
| K1 | Mac | Chrome | | | | |
| K1 | Mac | Safari | | ➖ | | (B uses the textarea in Safari) |
| K1 | Mac | Firefox | | ➖ | | |
| K2 | | | | | | |
| K3 | | | | | | |
| K4 | | | | | | |
| K5 | | | | | | |
| K6 | | | | | | |
| K7 | | | | | | |
| K8 | | | | | | |
| K9 | | | | | | |
| K10 | | | | | | |
| K11 | | | | | | |

## 2. Screen readers

Checks for every row: heading navigation (H, 1–6 / rotor); reading
continuously from page 3 into page 4 *without* page chrome being read;
typing echo; selection announced (Shift+arrows); a figure's alt text (the
test letter has none yet: note "n/a").

| Reader | Browser | View | Headings | Continuous reading | Typing echo | Selection | Notes |
|---|---|---|---|---|---|---|---|
| VoiceOver (macOS) | Safari | A | | | | | |
| VoiceOver (macOS) | Safari | B-ta | | | | | |
| VoiceOver (macOS) | Chrome | A | | | | | |
| VoiceOver (macOS) | Chrome | B | | | | | Also try `b.html?role=textbox` |
| VoiceOver (iOS) | Safari | A | | | | | |
| VoiceOver (iOS) | Safari | B-ta | | | | | |
| NVDA | Firefox | A | | | | | |
| NVDA | Firefox | B-ta | | | | | |
| NVDA | Chrome | A | | | | | |
| NVDA | Chrome | B | | | | | |
| JAWS | Chrome / Edge | A | | | | | |
| JAWS | Chrome / Edge | B | | | | | |
| Narrator (bonus) | Edge | A, B | | | | | |

## 3. Latency on reference hardware

On each device: open the view, press **Reset**, then either type for about
a minute at three places (page 1, page 50, a table cell on page 80: the
buttons place the caret) or press the three **1000 ×** buttons (synthetic:
engine + DOM + paint, not the input path). Download the JSON after each.
Target: p95 keystroke-to-paint under 16 ms on the mid-range laptop.

| Device | Browser | View | Typing p95 (p.1 / p.50 / table) | Synthetic p95 (p.1 / p.50 / table) | File |
|---|---|---|---|---|---|
| Mid-range Windows laptop (i5, 8 GB) | Chrome | A | | | |
| | Chrome | B | | | |
| | Firefox | A | | | |
| | Firefox | B | | | |
| MacBook Air M1 | Safari | A | | | |
| | Safari | B | | | |
| | Chrome | A | | | |
| | Chrome | B | | | |
| Mid-range Android (Pixel 6a / Galaxy A5x) | Chrome | A | | | |
| | Chrome | B | | | |
