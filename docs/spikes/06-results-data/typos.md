# Spike 6: seeded misspellings

| Lang | Class | n | Flagged (own dictionary) | Flagged if either dictionary may accept | Right word top 1 | Right word in top 3 |
|---|---|---|---|---|---|---|
| da | cmp | 15 | 15 | 15 | 12 | 15 |
| da | dbl | 15 | 13 | 13 | 7 | 11 |
| da | drop | 12 | 9 | 9 | 5 | 6 |
| da | extra | 6 | 6 | 6 | 1 | 3 |
| da | fuge | 10 | 10 | 10 | 6 | 9 |
| da | inflect | 4 | 4 | 4 | 3 | 4 |
| da | key | 10 | 9 | 9 | 5 | 8 |
| da | realword | 10 | 0 | 0 | 0 | 0 |
| da | split | 8 | 0 | 0 | 0 | 0 |
| da | swap | 10 | 10 | 10 | 8 | 9 |
| da | æøå | 13 | 12 | 10 | 5 | 6 |
| en | common | 53 | 53 | 50 | 47 | 53 |
| en | realword | 5 | 0 | 0 | 0 | 0 |

Totals (detectable classes = all but realword and split):

- da, realword + split: 18 typos, flagged 0 (0%), with either dictionary 0 (0%); of the 0 flagged single words, right word top 1 in 0 (0%), top 3 in 0 (0%)
- da, detectable: 95 typos, flagged 88 (93%), with either dictionary 86 (91%); of the 88 flagged single words, right word top 1 in 52 (59%), top 3 in 71 (81%)
- en, realword + split: 5 typos, flagged 0 (0%), with either dictionary 0 (0%); of the 0 flagged single words, right word top 1 in 0 (0%), top 3 in 0 (0%)
- en, detectable: 53 typos, flagged 53 (100%), with either dictionary 50 (94%); of the 53 flagged single words, right word top 1 in 47 (89%), top 3 in 53 (100%)

Suggestion time per flagged da word (88 words): median 238 ms, p95 1581 ms, max 3845 ms.

Suggestion time per flagged en word (53 words): median 13 ms, p95 24 ms, max 27 ms.

Right words the dictionary rejects: []

en_GB (comparison only) flags 53 of the 53 English common typos; right word in top 3 for 51.
