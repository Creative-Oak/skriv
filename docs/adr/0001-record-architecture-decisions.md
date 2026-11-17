# ADR 0001: Record architecture decisions

- Status: accepted
- Date: 2026-11-17

## Context

skriv is the first piece of an open-source Microsoft 365 replacement. The
architecture comes from a research report ("Building an Open-Source,
ODF-First Word Processor: Technology Survey and Recommended Architecture").
That report is good input, but several of its claims are marked unverified,
and its decision points can only be settled by spikes. We need a place where
each decision, its evidence, and what is still open stay visible over time.

## Decision

We keep Architecture Decision Records in `docs/adr/`, numbered, one decision
per file. Each ADR states:

- **status** (proposed, accepted, superseded by NNNN);
- **context**, **decision** and **consequences**;
- **evidence**: what we verified ourselves, as opposed to what we took from the report;
- **open decisions**: what this ADR deliberately leaves open, and which spike settles it.

Spike briefs and results live in `docs/spikes/`. When a spike settles an open
decision, the ADR is updated or superseded, and it links to the result.

## Consequences

- Decisions are reviewable by people who weren't in the room, including
  procurement reviewers in the public sector.
- Report claims that turn out wrong are corrected in the ADR that relied on
  them. They are not silently dropped.
