# Spike 2: Paginated editing latency, IME and screen readers

**Risk (report #2):** paginated editing feels laggy, or breaks IME and screen readers.

**Experiment.** Build view option A (ProseMirror view with engine-directed
lines and pages) and option B (an owned DOM view with EditContext, or a
hidden textarea), both on a 100-page document. Test with:

- a Danish keyboard, including dead keys and `æ ø å`;
- macOS and Windows input methods;
- Android Gboard;
- dictation;
- NVDA, JAWS and VoiceOver;
- selection across page breaks.

**Kill / continue (from the report).**
- p95 keystroke-to-paint under 16 ms on a mid-range laptop.
- No IME composition is lost.
- Headings and paragraphs can be navigated by screen reader.

**Decides:** the view strategy (ADR 0004).

**Status:** not started. Proposed plan: [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md).
