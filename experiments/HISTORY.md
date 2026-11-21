# Experiment history

Earlier timings below were single local runs unless stated otherwise. They were not collected with the current harness and should be treated as approximate. Practice-scale data was generated with seed 2, 20 million events and 5 million clicks; final-scale data used seed 3, 60 million events and 15 million clicks. Local runs used eight CPU IDs and a 24 GiB address-space limit. Address-space limits are not identical to the scorer's memory limit, and local machines and datasets differ from the scorer's.

| Version or experiment | Dataset | Wall time | Peak memory | Correctness and decision |
| --- | --- | ---: | ---: | --- |
| Initial correctness pipeline (`0669ba5`) | Hidden practice, leaderboard | 100.03 s | Not reported | User-reported successful score; baseline 171.91 s. |
| Initial pipeline | Seed 2, 20M | 78.08 s | 10.9 GiB | Local baseline. |
| Initial pipeline, instrumented | Seed 2, 20M | 78.73 s | 10.9 GiB | One run overlapped briefly with smaller checks; stage timings indicative only. |
| Compact retained event rows (`0743015`) | Seed 2, 20M | 65.17 s | 5.1 GiB | 36 fixtures and public dev answer matched; 20M output matched prior version. Kept. |
| Parallel event validation (`5297b5b`) | Seed 2, 20M | 62.32 s | 5.5 GiB | Fixtures and dev answer matched; 20M output matched prior version. Kept; difference is small versus noise. |
| Parallel event-file reading (`d3bdc55`) | Seed 2, 20M | 53.00 s | 6.6 GiB | Fixtures, dev answer, and previous 20M output matched. Kept. |
| Parallel event-file reading (`d3bdc55`) | Seed 3, 60M | 180 s cutoff | 20.1 GiB before cutoff | Timed out during output; full result unavailable. |
| Parallel session aggregation (`b99f536`) | Seed 2, 20M | 40.42 s | 6.6 GiB | Fixtures, dev answer, and previous 20M output matched. Kept. |
| Parallel session aggregation (`b99f536`) | Seed 3, 60M | 164.08 s | 20.2 GiB | Completed; both tables' session and revenue totals agreed. No full-size reference answer. |
| Fixed-size CSV field arrays, on `b99f536` | Seed 2, 20M | 43.98 s | 6.6 GiB | Fixtures and dev answer matched; slower in one run, so discarded pending stronger evidence. |
