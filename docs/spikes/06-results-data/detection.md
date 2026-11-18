# Spike 6: language per paragraph

23 documents, 676 paragraphs, 41.2 pages. Dev = documents 1, 3, 5, … in name order; test = 2, 4, 6, …. The policy is chosen on dev (fewest false switches, then accuracy) and reported on test and on all.

False switches are counted over proofable paragraphs (at least one word the checker checks); the last column counts them over every paragraph. Accuracy is over proofable paragraphs.

| Detector | Policy | µs/para | False switches (spurious+missed) | per 10 pages | test half | handwritten | Wikipedia | per 100 true switches | Accuracy | per 10 pages, all paragraphs | per 10 pages, paragraphs of 20+ letters |
|---|---|---|---|---|---|---|---|---|---|---|---|
| lingua-da-en | raw (no hysteresis) | 92 | 37 (27+10) | **9.0** | 8.5 | 24.3 | 2.1 | 26 | 96.2% | 10.0 | 2.2 |
| lingua-da-en | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 92 | 23 (5+18) | **5.6** | 10.5 | 16.5 | 0.7 | 16 | 97.3% | 8.0 | 2.2 |
| lingua-da-en | tuned on dev: min 20 letters, switch ≥ 0.95, short inherit, headings look ahead | 92 | 23 (4+19) | **5.6** | 11.5 | 16.5 | 0.7 | 16 | 97.6% | 8.0 | 2.2 |
| lingua-da-en | best on all (optimistic): min 10 letters, switch ≥ 0.8, short inherit, headings look ahead | 92 | 19 (9+10) | **4.6** | 4.5 | 13.3 | 0.7 | 13 | 98.1% | 7.0 | 1.7 |
| lingua-da-en+mask | raw (no hysteresis) | 60 | 33 (25+8) | **8.0** | 6.5 | 22.7 | 1.4 | 23 | 96.1% | 11.4 | 1.7 |
| lingua-da-en+mask | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 60 | 27 (8+19) | **6.6** | 11.5 | 21.2 | 0.0 | 19 | 96.7% | 9.0 | 2.2 |
| lingua-da-en+mask | tuned on dev: min 40 letters, switch ≥ 0, short inherit, headings look ahead | 60 | 31 (8+23) | **7.5** | 14.5 | 22.7 | 0.7 | 22 | 95.9% | 10.0 | 3.6 |
| lingua-da-en+mask | best on all (optimistic): min 10 letters, switch ≥ 0.7, short ≥ 0.7, headings look ahead | 60 | 15 (9+6) | **3.6** | 2.5 | 11.8 | 0.0 | 11 | 97.6% | 7.0 | 1.2 |
| lingua-da-en-nb-sv | raw (no hysteresis) | 123 | 45 (29+16) | **10.9** | 11.5 | 27.5 | 3.5 | 32 | 95.3% | 14.3 | 3.6 |
| lingua-da-en-nb-sv | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 123 | 46 (8+38) | **11.2** | 19.5 | 31.4 | 2.1 | 32 | 94.7% | 11.7 | 8.3 |
| lingua-da-en-nb-sv | tuned on dev: min 40 letters, switch ≥ 0, short inherit, headings look ahead | 123 | 33 (7+26) | **8.0** | 15.5 | 21.2 | 2.1 | 23 | 95.9% | 10.4 | 4.1 |
| lingua-da-en-nb-sv | best on all (optimistic): min 10 letters, switch ≥ 0.5, short inherit, headings look ahead | 123 | 27 (11+16) | **6.6** | 9.5 | 16.5 | 2.1 | 19 | 97.3% | 9.0 | 3.6 |
| lingua-da-en-nb-sv+mask | raw (no hysteresis) | 157 | 41 (28+13) | **10.0** | 8.5 | 25.9 | 2.8 | 29 | 95.1% | 13.3 | 3.2 |
| lingua-da-en-nb-sv+mask | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 157 | 53 (12+41) | **12.9** | 19.5 | 36.9 | 2.1 | 37 | 92.9% | 13.3 | 10.0 |
| lingua-da-en-nb-sv+mask | tuned on dev: min 40 letters, switch ≥ 0, short inherit, headings look ahead | 157 | 33 (9+24) | **8.0** | 14.5 | 22.7 | 1.4 | 23 | 95.8% | 10.4 | 4.1 |
| lingua-da-en-nb-sv+mask | best on all (optimistic): min 10 letters, switch ≥ 0.5, short inherit, headings look ahead | 157 | 21 (9+12) | **5.1** | 6.5 | 13.3 | 1.4 | 15 | 97.3% | 7.5 | 2.7 |
| lingua-da-en-low | raw (no hysteresis) | 83 | 89 (74+15) | **21.6** | 18.5 | 49.4 | 9.1 | 63 | 91.7% | 24.0 | 6.1 |
| lingua-da-en-low | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 83 | 35 (15+20) | **8.5** | 13.5 | 22.7 | 2.1 | 25 | 96.2% | 11.4 | 4.1 |
| lingua-da-en-low | tuned on dev: min 40 letters, switch ≥ 0.8, short inherit, headings look ahead | 83 | 37 (10+27) | **9.0** | 17.5 | 22.7 | 2.8 | 26 | 95.1% | 11.4 | 5.1 |
| lingua-da-en-low | best on all (optimistic): min 10 letters, switch ≥ 0.99, short inherit, headings look ahead | 83 | 33 (16+17) | **8.0** | 8.5 | 19.6 | 2.8 | 23 | 96.7% | 10.4 | 4.6 |
| whatlang-da-en | raw (no hysteresis) | 11 | 103 (83+20) | **25.0** | 20.5 | 51.8 | 13.0 | 73 | 90.7% | 30.8 | 4.6 |
| whatlang-da-en | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 11 | 27 (4+23) | **6.6** | 11.5 | 21.2 | 0.0 | 19 | 97.3% | 9.0 | 3.2 |
| whatlang-da-en | tuned on dev: min 20 letters, switch ≥ 0.5, short inherit, headings look ahead | 11 | 24 (3+21) | **5.8** | 11.0 | 18.8 | 0.0 | 17 | 97.6% | 8.3 | 2.4 |
| whatlang-da-en | best on all (optimistic): min 20 letters, switch ≥ 0.5, short inherit, headings look ahead | 11 | 24 (3+21) | **5.8** | 11.0 | 18.8 | 0.0 | 17 | 97.6% | 8.3 | 2.4 |
| whatlang-da-en+mask | raw (no hysteresis) | 11 | 101 (81+20) | **24.5** | 22.5 | 50.2 | 13.0 | 71 | 90.6% | 28.9 | 4.6 |
| whatlang-da-en+mask | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 11 | 29 (4+25) | **7.0** | 12.5 | 22.7 | 0.0 | 20 | 97.2% | 9.5 | 3.6 |
| whatlang-da-en+mask | tuned on dev: min 20 letters, switch ≥ 0.5, short inherit, headings look ahead | 11 | 26 (3+23) | **6.3** | 12.0 | 20.4 | 0.0 | 18 | 97.5% | 8.7 | 2.9 |
| whatlang-da-en+mask | best on all (optimistic): min 20 letters, switch ≥ 0.5, short inherit, headings look ahead | 11 | 26 (3+23) | **6.3** | 12.0 | 20.4 | 0.0 | 18 | 97.5% | 8.7 | 2.9 |
| whatlang-all | raw (no hysteresis) | 58 | 91 (41+50) | **22.1** | 24.5 | 32.9 | 17.2 | 64 | 90.9% | 24.5 | 5.6 |
| whatlang-all | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 58 | 89 (13+76) | **21.6** | 28.0 | 41.6 | 12.7 | 63 | 78.6% | 22.6 | 19.2 |
| whatlang-all | tuned on dev: min 40 letters, switch ≥ 0, short inherit, headings look ahead | 58 | 41 (7+34) | **10.0** | 18.5 | 23.5 | 3.9 | 29 | 94.3% | 12.4 | 6.1 |
| whatlang-all | best on all (optimistic): min 20 letters, switch ≥ 0, short inherit, headings look ahead | 58 | 39 (9+30) | **9.5** | 15.5 | 20.4 | 4.6 | 27 | 96.2% | 11.9 | 5.6 |
| dict-vote | raw (no hysteresis) | 384 | 28 (14+14) | **6.8** | 7.0 | 15.7 | 2.8 | 20 | 97.5% | 9.2 | 1.0 |
| dict-vote | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 384 | 87 (10+77) | **21.1** | 27.5 | 37.7 | 13.7 | 61 | 84.6% | 22.6 | 17.7 |
| dict-vote | tuned on dev: min 20 letters, switch ≥ 0, short inherit, headings look ahead | 384 | 22 (7+15) | **5.3** | 10.0 | 17.3 | 0.0 | 15 | 98.1% | 7.8 | 1.0 |
| dict-vote | best on all (optimistic): min 10 letters, switch ≥ 0, short inherit, headings look ahead | 384 | 18 (8+10) | **4.4** | 6.0 | 14.1 | 0.0 | 13 | 98.6% | 6.8 | 1.0 |
| lingua-da-en+mask & dict-vote | raw (no hysteresis) | 536 | 31 (12+19) | **7.5** | 8.5 | 14.9 | 4.2 | 22 | 97.2% | 10.0 | 1.2 |
| lingua-da-en+mask & dict-vote | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 536 | 87 (10+77) | **21.1** | 27.5 | 37.7 | 13.7 | 61 | 84.6% | 22.6 | 17.7 |
| lingua-da-en+mask & dict-vote | tuned on dev: min 20 letters, switch ≥ 0, short inherit, headings look ahead | 536 | 21 (5+16) | **5.1** | 10.5 | 16.5 | 0.0 | 15 | 98.0% | 7.5 | 1.2 |
| lingua-da-en+mask & dict-vote | best on all (optimistic): min 10 letters, switch ≥ 0, short inherit, headings look ahead | 536 | 17 (4+13) | **4.1** | 6.5 | 13.3 | 0.0 | 12 | 98.6% | 6.6 | 1.2 |
| whatlang-da-en & dict-vote | raw (no hysteresis) | 463 | 34 (12+22) | **8.3** | 10.0 | 14.1 | 5.6 | 24 | 97.3% | 10.7 | 1.5 |
| whatlang-da-en & dict-vote | fixed: min 20 letters, switch ≥ 0.9, short inherit, headings look ahead | 463 | 87 (10+77) | **21.1** | 27.5 | 37.7 | 13.7 | 61 | 84.6% | 22.6 | 17.7 |
| whatlang-da-en & dict-vote | tuned on dev: min 20 letters, switch ≥ 0, short inherit, headings look ahead | 463 | 22 (5+17) | **5.3** | 11.0 | 17.3 | 0.0 | 15 | 98.0% | 7.8 | 1.5 |
| whatlang-da-en & dict-vote | best on all (optimistic): min 10 letters, switch ≥ 0, short inherit, headings look ahead | 463 | 16 (2+14) | **3.9** | 7.0 | 12.6 | 0.0 | 11 | 98.7% | 6.3 | 1.5 |

## Accuracy by paragraph length (letters)

Detector alone / raw assignment / tuned policy.

| Detector | a: <20 letters (n=233) | b: 20-59 (n=155) | c: 60-149 (n=88) | d: 150-399 (n=157) | e: 400+ (n=43) |
|---|---|---|---|---|---|
| lingua-da-en | 79 / 84 / 93% | 96 / 96 / 97% | 100 / 100 / 100% | 99 / 99 / 99% | 100 / 100 / 100% |
| lingua-da-en+mask | 76 / 82 / 91% | 94 / 94 / 92% | 100 / 100 / 100% | 100 / 100 / 100% | 100 / 100 / 100% |
| lingua-da-en-nb-sv | 63 / 80 / 91% | 92 / 95 / 94% | 100 / 100 / 100% | 99 / 99 / 99% | 100 / 100 / 100% |
| lingua-da-en-nb-sv+mask | 61 / 81 / 91% | 87 / 93 / 92% | 100 / 100 / 100% | 99 / 99 / 99% | 100 / 100 / 100% |
| lingua-da-en-low | 67 / 75 / 91% | 92 / 92 / 91% | 100 / 100 / 100% | 99 / 99 / 99% | 98 / 98 / 98% |
| whatlang-da-en | 63 / 67 / 93% | 94 / 94 / 97% | 100 / 100 / 99% | 100 / 100 / 100% | 100 / 100 / 100% |
| whatlang-da-en+mask | 61 / 68 / 93% | 94 / 94 / 97% | 100 / 100 / 98% | 100 / 100 / 100% | 100 / 100 / 100% |
| whatlang-all | 18 / 77 / 91% | 70 / 95 / 90% | 94 / 98 / 98% | 95 / 98 / 98% | 95 / 95 / 95% |
| dict-vote | 70 / 89 / 93% | 95 / 99 / 99% | 100 / 100 / 100% | 100 / 100 / 100% | 100 / 100 / 100% |
| lingua-da-en+mask & dict-vote | 67 / 89 / 93% | 91 / 98 / 98% | 100 / 100 / 100% | 100 / 100 / 100% | 100 / 100 / 100% |
| whatlang-da-en & dict-vote | 55 / 92 / 93% | 89 / 98 / 98% | 100 / 100 / 100% | 100 / 100 / 100% | 100 / 100 / 100% |

## Accuracy by paragraph kind (tuned policy)

| Detector | addr (n=20) | code (n=4) | h (n=127) | li (n=100) | p (n=277) | quote (n=5) | ref (n=5) | td (n=126) | title (n=12) |
|---|---|---|---|---|---|---|---|---|---|
| lingua-da-en | 85% | 100% | 94% | 99% | 99% | 100% | 100% | 94% | 100% |
| lingua-da-en+mask | 85% | 100% | 92% | 91% | 99% | 100% | 100% | 94% | 100% |
| lingua-da-en-nb-sv | 85% | 100% | 92% | 93% | 98% | 100% | 100% | 94% | 100% |
| lingua-da-en-nb-sv+mask | 85% | 100% | 92% | 91% | 98% | 100% | 100% | 94% | 100% |
| lingua-da-en-low | 85% | 100% | 92% | 93% | 97% | 100% | 100% | 91% | 100% |
| whatlang-da-en | 85% | 75% | 93% | 100% | 99% | 100% | 100% | 94% | 100% |
| whatlang-da-en+mask | 85% | 75% | 93% | 100% | 99% | 100% | 80% | 94% | 100% |
| whatlang-all | 85% | 100% | 92% | 89% | 96% | 100% | 100% | 94% | 100% |
| dict-vote | 90% | 75% | 95% | 100% | 99% | 100% | 100% | 94% | 100% |
| lingua-da-en+mask & dict-vote | 90% | 100% | 94% | 100% | 99% | 100% | 100% | 94% | 100% |
| whatlang-da-en & dict-vote | 90% | 100% | 94% | 100% | 99% | 100% | 100% | 94% | 100% |
