### loro — letter, growing (2 pages at the start)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50779 inserts (931985 chars), 24906 deletes (153801 chars), 12054 formats, 4042 splits, 4236 paragraph properties, 3983 cell edits |
| Time to replay the year | 39.2 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 306.7 KiB (content.xml + styles.xml 1623.1 KiB) |
| CRDT stored, history included | 5294.9 KiB |
| **Stored ÷ ODT at the end** | **17.3×** (÷ uncompressed XML: 3.26×) |
| Load (median of 5) | 12.9 ms |
| Read the whole document after load | 165.5 ms |
| Resident memory after load | 85.5 MiB (process baseline 8.4 MiB) |
| | snapshot 5421945 bytes, import+state 111.3 ms |
| | updates only 4549982 bytes, import+state 386.6 ms (3.5× the snapshot) |
| | shallow snapshot (no history) 2239004 bytes, import+state 94.5 ms |
| | state only 2239003 bytes |

### automerge — letter, growing (2 pages at the start)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50779 inserts (931985 chars), 24906 deletes (153801 chars), 12054 formats, 4042 splits, 4236 paragraph properties, 3983 cell edits |
| Time to replay the year | 561.7 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 304.0 KiB (content.xml + styles.xml 1643.9 KiB) |
| CRDT stored, history included | 2258.6 KiB |
| **Stored ÷ ODT at the end** | **7.4×** (÷ uncompressed XML: 1.37×) |
| Load (median of 5) | 1275.1 ms |
| Read the whole document after load | 184.6 ms |
| Resident memory after load | 222.5 MiB (process baseline 5.5 MiB) |
| | 5001 changes; saved without compression 5267870 bytes |

### loro — letter, steady size (2 pages)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50714 inserts (929983 chars), 24670 deletes (509319 chars), 11848 formats, 4035 splits, 4732 paragraph properties, 4001 cell edits |
| Time to replay the year | 31.7 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 188.6 KiB (content.xml + styles.xml 1026.1 KiB) |
| CRDT stored, history included | 4753.7 KiB |
| **Stored ÷ ODT at the end** | **25.2×** (÷ uncompressed XML: 4.63×) |
| Load (median of 5) | 9.6 ms |
| Read the whole document after load | 143.9 ms |
| Resident memory after load | 67.8 MiB (process baseline 7.8 MiB) |
| | snapshot 4867816 bytes, import+state 98.5 ms |
| | updates only 4442969 bytes, import+state 448.5 ms (4.6× the snapshot) |
| | shallow snapshot (no history) 1664798 bytes, import+state 94.8 ms |
| | state only 1664797 bytes |

### automerge — letter, steady size (2 pages)

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50714 inserts (929983 chars), 24670 deletes (509319 chars), 11848 formats, 4035 splits, 4732 paragraph properties, 4001 cell edits |
| Time to replay the year | 548.9 s |
| ODT at the start | 28.2 KiB |
| ODT at the end | 188.5 KiB (content.xml + styles.xml 1078.3 KiB) |
| CRDT stored, history included | 2269.8 KiB |
| **Stored ÷ ODT at the end** | **12.0×** (÷ uncompressed XML: 2.10×) |
| Load (median of 5) | 1524.0 ms |
| Read the whole document after load | 156.0 ms |
| Resident memory after load | 293.3 MiB (process baseline 5.4 MiB) |
| | 5001 changes; saved without compression 5265154 bytes |

### loro — 100 pages, steady size

| Measure | Value |
|---|---|
| Sessions × operations | 250 × 400 = 100000 |
| Edits | 50795 inserts (931583 chars), 24678 deletes (513439 chars), 11831 formats, 4011 splits, 4672 paragraph properties, 4013 cell edits |
| Time to replay the year | 48.6 s |
| ODT at the start | 32.1 KiB |
| ODT at the end | 249.6 KiB (content.xml + styles.xml 1362.7 KiB) |
| CRDT stored, history included | 5669.5 KiB |
| **Stored ÷ ODT at the end** | **22.7×** (÷ uncompressed XML: 4.16×) |
| Load (median of 5) | 12.3 ms |
| Read the whole document after load | 206.8 ms |
| Resident memory after load | 77.5 MiB (process baseline 8.7 MiB) |
| | snapshot 5805561 bytes, import+state 153.0 ms |
| | updates only 5098803 bytes, import+state 433.2 ms (2.8× the snapshot) |
| | shallow snapshot (no history) 2400273 bytes, import+state 137.0 ms |
| | state only 2400272 bytes |

