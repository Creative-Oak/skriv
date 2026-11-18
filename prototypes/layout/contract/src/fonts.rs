// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The fonts both engines use, embedded so native and wasm builds see the
//! same bytes. Liberation 2.1.5 (SIL OFL 1.1), metric-compatible with
//! Times New Roman and Arial. Run `fetch-fonts.sh` before building.

pub struct Face {
    pub family: &'static str,
    pub bold: bool,
    pub italic: bool,
    pub data: &'static [u8],
}

macro_rules! face {
    ($family:literal, $bold:literal, $italic:literal, $file:literal) => {
        Face {
            family: $family,
            bold: $bold,
            italic: $italic,
            data: include_bytes!(concat!("../../fonts/", $file)),
        }
    };
}

pub const FACES: &[Face] = &[
    face!(
        "Liberation Serif",
        false,
        false,
        "LiberationSerif-Regular.ttf"
    ),
    face!("Liberation Serif", true, false, "LiberationSerif-Bold.ttf"),
    face!(
        "Liberation Serif",
        false,
        true,
        "LiberationSerif-Italic.ttf"
    ),
    face!(
        "Liberation Serif",
        true,
        true,
        "LiberationSerif-BoldItalic.ttf"
    ),
    face!(
        "Liberation Sans",
        false,
        false,
        "LiberationSans-Regular.ttf"
    ),
    face!("Liberation Sans", true, false, "LiberationSans-Bold.ttf"),
    face!("Liberation Sans", false, true, "LiberationSans-Italic.ttf"),
    face!(
        "Liberation Sans",
        true,
        true,
        "LiberationSans-BoldItalic.ttf"
    ),
];

/// Map an ODF font name to one of our families. Anything sans-serif-looking
/// (Arial, Helvetica, Calibri, Liberation Sans) gets Liberation Sans; the
/// rest gets Liberation Serif.
pub fn family_for(odf_name: &str) -> &'static str {
    let n = odf_name.to_ascii_lowercase();
    if ["sans", "arial", "helvetica", "calibri", "verdana", "segoe"]
        .iter()
        .any(|k| n.contains(k))
    {
        "Liberation Sans"
    } else {
        "Liberation Serif"
    }
}
