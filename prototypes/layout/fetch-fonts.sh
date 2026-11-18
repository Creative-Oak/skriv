#!/bin/sh
# Copy the Liberation fonts (SIL OFL 1.1; metric-compatible with Times New
# Roman and Arial) next to the prototypes. They are embedded at build time,
# so native and wasm builds lay out with identical fonts.
# Debian/Ubuntu: apt-get install fonts-liberation
set -e
cd "$(dirname "$0")/fonts"
src=/usr/share/fonts/truetype/liberation
for f in LiberationSerif-Regular LiberationSerif-Bold LiberationSerif-Italic LiberationSerif-BoldItalic \
         LiberationSans-Regular LiberationSans-Bold LiberationSans-Italic LiberationSans-BoldItalic; do
  cp "$src/$f.ttf" .
done
sha256sum *.ttf
