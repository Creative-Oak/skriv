# Design 01: Guidance for people who think in Word

- Status: proposed
- Date: 2026-11-18
- Working name: *guidance* (Danish UI: *forklaringer*). Not a character.

## The problem

skriv asks Word users to work differently. The ideas aren't hard, but they
aren't Word's ideas, and nobody reads a manual:

| What people do now | What skriv asks for |
|---|---|
| Make a line bold and 16 pt so it looks like a heading | Use a heading style, so it's in the outline and the table of contents |
| Type "1.", "2.", "a)" by hand | Use a list, so numbers stay right when paragraphs move |
| Press Enter twice for space, or until a new page | Give the style spacing, or insert a page break |
| Indent with spaces and tabs | Use indentation, so it survives a font change |
| Save `rapport_v3_ENDELIG.docx` | Name a version; skriv keeps history |
| Type "Figur 3" and "se side 12" | Use a caption and a cross-reference, so they renumber |
| Send .docx by default | ODT is the native format; .docx is an export |

Every change on the right pays off later: when the document is restructured,
reformatted or reused. The habit on the left looks fine *today*. So help has to
show the payoff, not lecture about correctness.

## What we take from Clippy, and from what worked

Clippy failed because it **interrupted**, **guessed wrongly** and talked about
**the program instead of your document**. It also couldn't be told "never
again" in a way that stuck.

Things that worked:

- Word's own Reveal Formatting (Shift+F1) answers "why does this look like
  this?" exactly. Hardly anyone knows it exists.
- Spellcheck's underline is calm: it marks, it doesn't speak, and you act
  when you choose to.
- Command palettes let people search in their own words.

## Principles

These are the rules every guidance feature must follow. They are meant to be
testable.

1. **Explain the document, not the app.** Every message refers to something on
   the page.
2. **Never interrupt input.** Nothing appears or moves while the user is
   typing. Nothing takes focus, and nothing is modal.
3. **Mark first, speak when asked.** The default form is a quiet marker. Text
   appears when the caret rests in that place, or when the marker is clicked.
4. **Only speak when sure.** A detector ships only with measured precision
   ≥ 95% on our corpus (see *Measuring it*).
5. **One click fixes it, one undo reverts it.** A suggestion carries its fix
   as a single transaction.
6. **Show the payoff.** After a fix, say what it gained: "Now in the outline."
7. **"No" is remembered.** Dismissing one suggestion stops that suggestion.
   Dismissing a kind twice offers to switch that kind off. Settings list
   everything that was switched off.
8. **Everything is reachable on demand.** Nothing exists only as a popup.
   Someone who turns all nudges off loses no information.
9. **Danish first, plain, never scolding.** No "wrong", no "should". Copy
   says what skriv can do, not what the user did badly.
10. **Local only.** What was shown and dismissed lives on the user's machine.
    No telemetry decides what to show.

## The four surfaces

They run from passive to active. The cheapest and safest come first.

### 1. The inspector: "Why does this look like this?"

The core idea, and the one only an ODF-shaped model can do well.

- Opened with a shortcut, a context-menu item, or by clicking the style name
  in the toolbar.
- It shows the sentence first: **"Overskrift 2 + fed tilføjet i hånden"**
  ("Heading 2 + bold added by hand").
- Below that, every property that differs from the body text, with where it
  came from:

  ```
  Overskrift 2                          [Gå til typografi]
    skrift      Liberation Sans 14 pt   fra Overskrift
    afstand før 10 pt                   fra Overskrift 2
  Tilføjet i hånden                     [Nulstil til Overskrift 2]
    fed                                 dette afsnit
    farve       #C9211E                 de markerede ord
  ```

- It explains more than formatting. These are the questions people ask, and
  the model already knows the answers:
  - "Why is this on a new page?" → page break before, set in the style
    *Kapitel*; or a manual page break.
  - "Why is this number 4?" → the list continues from the list on page 2.
  - "Why does this say 12?" → it's a page-number field; it updates itself.
  - "Why can't I see this in the table of contents?" → it isn't a heading
    style; it only looks like one. (This is where the inspector hands over
    to a nudge.)
- Each "added by hand" layer has a reset. Each style has "update the style
  to match this", which is how people learn that styles are editable.

### 2. Word words in command search

People arrive with Word's vocabulary, in Danish. The search accepts it, takes
them to the skriv equivalent, and says in one line how it differs.

| They type | They get |
|---|---|
| Formatpensel / Format Painter | *Kopiér formatering.* "Bruger du det tit, så lav en typografi." |
| Spor ændringer / Track Changes | *Spor ændringer.* Same thing. |
| Gem som / Save As | *Gem en navngivet version*, plus *Eksportér*. "skriv husker alle versioner." |
| Typografier / Styles | *Typografier.* |
| Sektionsskift / Section break | *Sektion* or *Sideskift*, with a line on the difference |
| Krydshenvisning / Cross-reference | *Henvisning* |
| Indholdsfortegnelse | *Indholdsfortegnelse* |
| .docx / Word | *Eksportér som Word.* "Modtageren kan åbne det i Word." |

The map is plain data, one row per term, written and reviewed by people who
use Danish Word. Exact matches are enough to start with; no fuzzy matching
until we see what people search for.

### 3. Nudges when someone works the Word way

A detector looks at the model and proposes a suggestion. It appears as a
small marker in the margin. When the caret leaves that paragraph or rests for
about two seconds, one line of text appears next to the marker.

> Det her ligner en overskrift. **Gør til Overskrift 2** · Nej tak

After the fix:

> Står nu i dokumentoversigten og kan komme med i indholdsfortegnelsen.

#### Detector catalogue

| Habit | Signal in the model | Fix | False-positive risk | First? |
|---|---|---|---|---|
| Fake heading | A short paragraph (≤ 12 words, no final full stop) whose resolved size or weight is above the body text's. It is followed by body text. The same pattern repeats in the document | Apply the heading style whose look is closest; drop the direct properties that are now redundant | Medium: a bold lead-in line, a form label | **Yes** |
| Typed numbering | Consecutive paragraphs starting with `1.`, `2)`, `a)`, `i.` or `•`, `–`, `*` and a space or tab | Convert them to a list, keeping the numbering format | Low | **Yes** |
| Empty paragraphs for spacing | ≥ 2 empty paragraphs in a row, or one used repeatedly between the same kinds of block | Remove them, and add spacing before or after to the style | Medium: forms and signature blocks | Later |
| Enter until next page | A run of empty paragraphs that ends at a page bottom | Page break | Low, but it needs layout | Later (needs layout) |
| Spaces for indent | A paragraph starting with ≥ 3 spaces, or tabs used only at the start | Indentation | Low | Later |
| Repeated direct formatting | ≥ 4 paragraphs with the same named style plus identical direct properties | "Lav en typografi af det" | Low | Later |
| Typed caption and references | Text matching `Figur 3`, `Tabel 2` next to a frame or table; `se side 12` | Caption field; cross-reference | Medium | Later |
| Typed date or page | A date in the header or footer; "Side 3 af 9" | Field | Low | Later |
| Versions in file names | Opening or saving with `v2`, `endelig`, `final`, `rev` in the name | "Navngiv en version i stedet?" | Low | With versions |

"First?" marks the two detectors worth building first. They look like
the most common Word habits, though we haven't measured that yet (G0 will).
They are also the easiest to detect precisely, and their payoff (outline, TOC,
renumbering) is visible right away.

#### Attention budget

- At most **one nudge text visible** at a time. Markers can be many, but they
  fade with distance from the caret.
- **No nudges in the first few minutes** of a new user's first document. Let
  them write.
- **Per kind:** after two dismissals, offer "Slå denne slags forslag fra?"
  once.
- **Imported documents get a summary, not a rash.** A Word file with 40 fake
  headings shows one line in the document bar: "14 linjer ligner overskrifter.
  Gennemgå?" Reviewing them is a list with "fix all", "fix this" and "skip".
  This is where guidance is worth most to people migrating from Word.
- Nudges are off in read-only documents, during review, and in comment threads.

### 4. One-time cards for new concepts

Only for ideas that have no Word equivalent: **versions** (instead of files),
**ODT and sharing with Word users**, and later **collaboration**.

- A card appears the first time the user meets the concept, at the place they
  met it. Examples: the first "Gem som", or the first time they send a file
  to someone.
- It's three sentences at most, with one action and a "Forstået".
- After that, the card is gone for good, and a small "?" next to the feature
  opens it again.

> **Du behøver ikke gemme kopier.** skriv husker hver version af dokumentet,
> og du kan altid gå tilbage. Giv en version et navn, når den er vigtig.
> **Navngiv denne version** · Forstået

## Accessibility

- Markers are buttons in the reading order after their paragraph, with an
  accessible name ("Forslag: gør til Overskrift 2").
- Suggestion text is announced through a polite live region, only when it
  appears. It is never announced while typing.
- A shortcut moves to the next or previous suggestion, as spellcheck does.
- Markers aren't colour-only, and they meet contrast requirements in both
  themes.
- The inspector is an ordinary panel with headings, readable from top to
  bottom.

## Architecture

Guidance is mostly model work, and it follows ADR 0002 and ADR 0004: the view
never owns the document.

- **Detectors are pure functions in Rust**, over the model and its resolved
  styles. They live in a new crate, `crates/guidance`, which depends on
  `model` only, and they ship with the rest of the core as WASM.

  ```rust
  pub struct Suggestion {
      pub kind: SuggestionKind,      // FakeHeading, TypedNumbering, ...
      pub nodes: Vec<NodeId>,        // stable IDs, so dismissals survive edits
      pub confidence: f32,
      pub fix: Fix,                  // one transaction against the model
  }
  ```

- **Dismissals are keyed by `(kind, NodeId)`.** They are stored locally,
  outside the document, so they never travel in the ODT or reach other people.
- **Detectors run incrementally** on changed blocks, off the input path, after
  the user pauses.
- **The view only renders markers and text.** Copy strings live in a catalogue
  keyed by `SuggestionKind`, with Danish as the source language.

### What the model needs, and doesn't have yet

1. **Full provenance resolution.** `Styles::explain` today reports one
   automatic style's own properties and its parent's name. The inspector needs,
   for any position and any property, the resolved value and which layer it
   came from. The layers are:
   - document defaults;
   - each named style in the parent chain;
   - the paragraph's automatic style;
   - the span's automatic style;
   - a tracked format change.

   It also needs list styles, page styles and conditional styles.
2. **Resolved comparisons.** Detectors need questions like "is this bigger
   than body text?". That needs resolved, unit-normalised values: pt, cm, %,
   and relative sizes.
3. **Fixes that keep the ODT clean.** Applying a heading style must:
   - change the style reference;
   - drop the now-redundant direct properties;
   - keep everything else in the slots.

   It must not leave a trail of orphan automatic styles. The fidelity harness
   can check this.
4. **Field and list introspection** for the "why is this number 4?" family.
   The model stores targets and formats already; it needs read accessors that
   say it in words.

Items 1 and 2 are worth doing early, even before any UI. Layout (Spike 2b)
needs resolved properties anyway.

## Measuring it

- **Precision before shipping.** Run every detector over the fidelity corpus:
  the generated documents, the LibreOffice test files and the Word-exported
  public-sector documents. Hand-label a sample of the hits. A detector ships
  at ≥ 95% precision and stays switched off until then. Recall matters less;
  a missed nudge costs nothing.
- **Precision in CI.** The labelled hits become golden tests, so a detector
  change that adds false positives fails.
- **With people.** Short sessions with Danish public-sector staff, on their
  own documents: do they understand the inspector sentence? Do they accept the
  fix? Do they feel interrupted? That's qualitative, and it's the measure that
  counts.
- **No telemetry** (principle 10). If we ever want aggregate numbers, they
  would be opt-in, and that is a separate decision.

## Phasing

| Step | What | Depends on |
|---|---|---|
| G0 | Provenance resolution in `model`, with tests. Run the fake-heading and typed-numbering detectors over the corpus as a report, to measure precision | Nothing; can start now |
| G1 | The inspector | The first editor UI (after Spike 2) |
| G2 | Word words in command search | The command palette |
| G3 | Fake-heading and typed-numbering nudges, plus the import summary | G0 precision results; undo |
| G4 | Concept cards | Versions |
| G5 | The rest of the catalogue, one detector at a time, each through the precision gate | G3 |

## Open questions

- Should the import summary also run on documents opened from a colleague, or
  only on imports from .docx? A colleague's document may be "theirs", and
  suggesting fixes to it could feel presumptuous.
- How do the heading levels map? A fake heading at 16 pt in a document whose
  real headings are absent: is it Overskrift 1 or 2? One option: rank the fake
  headings by size, and offer the mapping once for the whole document.
- How does this interact with municipal templates that define their own
  styles (e.g. *KL Overskrift*)? Detectors should prefer the template's
  styles, which needs a notion of "the house styles" for a document.
- Naming in the UI: *forslag* (suggestions) for nudges, *forklaring*
  (explanation) for the inspector? This needs testing with users.
- Should guidance teach keyboard shortcuts? Probably only in command search,
  next to the result.
