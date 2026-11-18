### loro-flow — letter, growing (2 pages at the start)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50779 inserts (931985 chars), 24906 deletes (153801 chars), 12054 formats, 4042 splits, 4236 paragraph properties, 3983 cell edits |
| Time to replay the year | 260.5 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 304.0 KiB (content.xml + styles.xml 1643.9 KiB) |
| CRDT stored, history included | 3240.1 KiB |
| **Stored ÷ ODT at the end** | **10.7×** (÷ uncompressed XML: 1.97×) |
| Load (median of 5) | 6.4 ms |
| Read the whole document after load | 199.7 ms |
| Resident memory after load | 89.3 MiB (process baseline 6.4 MiB) |
| | snapshot 3317878 bytes, import+state 8.1 ms |
| | updates only 2879075 bytes, import+state 462.4 ms (57.3× the snapshot) |
| | shallow snapshot (no history) 1523069 bytes, import+state 6.2 ms |
| | state only 1523068 bytes |

### loro-flow — letter, steady size (2 pages)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50714 inserts (929983 chars), 24670 deletes (509319 chars), 11848 formats, 4035 splits, 4732 paragraph properties, 4001 cell edits |
| Time to replay the year | 179.1 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 188.5 KiB (content.xml + styles.xml 1078.3 KiB) |
| CRDT stored, history included | 3124.7 KiB |
| **Stored ÷ ODT at the end** | **16.6×** (÷ uncompressed XML: 2.90×) |
| Load (median of 5) | 5.0 ms |
| Read the whole document after load | 121.3 ms |
| Resident memory after load | 79.5 MiB (process baseline 6.3 MiB) |
| | snapshot 3199702 bytes, import+state 6.4 ms |
| | updates only 3301176 bytes, import+state 382.5 ms (59.7× the snapshot) |
| | shallow snapshot (no history) 990337 bytes, import+state 4.2 ms |
| | state only 990336 bytes |

### loro-flow — 100 pages, steady size

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50795 inserts (931583 chars), 24678 deletes (513439 chars), 11831 formats, 4011 splits, 4672 paragraph properties, 4013 cell edits |
| Time to replay the year | 58.5 s |
| ODT at the start | 32.1 KiB |
| ODT at the end | 249.8 KiB (content.xml + styles.xml 1372.1 KiB) |
| CRDT stored, history included | 4575.6 KiB |
| **Stored ÷ ODT at the end** | **18.3×** (÷ uncompressed XML: 3.33×) |
| Load (median of 5) | 11.1 ms |
| Read the whole document after load | 147.2 ms |
| Resident memory after load | 94.6 MiB (process baseline 7.6 MiB) |
| | snapshot 4685442 bytes, import+state 47.2 ms |
| | updates only 4307552 bytes, import+state 446.1 ms (9.5× the snapshot) |
| | shallow snapshot (no history) 1956612 bytes, import+state 32.1 ms |
| | state only 1956611 bytes |

