// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// EditContext (https://w3c.github.io/edit-context/): Chromium only, not in
// TypeScript's DOM library yet.

interface TextFormat {
  rangeStart: number
  rangeEnd: number
  underlineStyle: string
  underlineThickness: string
}

interface TextUpdateEvent extends Event {
  readonly updateRangeStart: number
  readonly updateRangeEnd: number
  readonly text: string
  readonly selectionStart: number
  readonly selectionEnd: number
}

interface TextFormatUpdateEvent extends Event {
  getTextFormats(): TextFormat[]
}

interface CharacterBoundsUpdateEvent extends Event {
  readonly rangeStart: number
  readonly rangeEnd: number
}

declare class EditContext extends EventTarget {
  constructor(init?: { text?: string; selectionStart?: number; selectionEnd?: number })
  readonly text: string
  readonly selectionStart: number
  readonly selectionEnd: number
  updateText(rangeStart: number, rangeEnd: number, text: string): void
  updateSelection(start: number, end: number): void
  updateControlBounds(bounds: DOMRect): void
  updateSelectionBounds(bounds: DOMRect): void
  updateCharacterBounds(rangeStart: number, bounds: DOMRect[]): void
}

interface HTMLElement {
  editContext: EditContext | null
}
