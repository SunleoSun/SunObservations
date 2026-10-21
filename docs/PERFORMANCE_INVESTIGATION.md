# SunObservations: accepted observation and async JSONL cost

## Executive conclusion

**Не продолжать общий hot-path refactor без измеренной нагрузки приложения.** При
10–100k observations/s один текущий JsonlSink использует лишь примерно 1–8% своей
измеренной service capacity. Резерв есть, но предыдущая гипотеза «writer в основном
повторно сериализует Value» неполна: для малого record **освобождение owned records
дороже JSON encoding**.

Лучший следующий локальный эксперимент при устойчивом backlog — **ограничить writer
batch**: контроль 8192 records вместо всего накопленного prefix дал 710 → 586 ns/record
(−17.5% времени, +21% capacity) в 9 повторениях. Production-speedup пока **не измерен**;
это рекомендация для следующего небольшого изменения, не внедрённая оптимизация.

Не переносить RFC3339 на writer: producer экономит около 63 ns, но writer получает
около 100 ns дополнительной работы. Не переходить глобально на raw payload: выигрыш
есть для numeric/array payloads, однако текстовый контрпример ухудшил producer в **7.2 раза**.
Небольшой timestamp cache — только условный второй кандидат для producer-latency workloads:
измеренный выигрыш около **28 ns**, а не сотни ns.

## Environment

- Измерения 2026-10-20, checkout `283b6448ec9008f175ff4540a3b4a7dc913845f5`.
- AMD Ryzen 9 5950X, 16 cores / 32 logical processors, около 64 GiB RAM.
- Windows 11 Pro 10.0.26200, x86_64-pc-windows-msvc, local C:, NTFS.
- rustc 1.96.0 (`ac68faa20`, LLVM 22.1.2), Cargo 1.96.0.
- Optimized default bench/release profile; System allocator; без task-specific RUSTFLAGS,
  native CPU flags, affinity, LTO и изменения production source. Машина не изолирована.
- Cargo artifacts: `C:\Temp\sun-observations-build`; JSONL — обычный temporary directory
  на C:. Retention отключён, default flush interval = 1000 ms.
- Обычная buffered запись с OS caching. `BufWriter::flush()` **не делает fsync /
  FlushFileBuffers**. Это service rate до OS file cache, не durable storage bandwidth.

Исходные measurement snapshots были локальными файлами в `performance/`; они не являются
tracked release artifacts и больше не присутствуют. Воспроизводимость обеспечивается
benchmark harness и командами ниже.

## Current baseline

Два актуальных harness дают следующий порядок величин. Все числа ниже — **median**, ns/call;
интервалы min–max, не confidence intervals. Указанные далее разницы обычно являются
разностями medians; отдельно отмечены paired medians из одинаковых sample IDs.

| Existing runtime harness, 7 × 1M calls | Median | min–max |
| --- | ---: | ---: |
| inactive | 7.11 | 7.09–7.15 |
| excluded | 7.13 | 7.11–8.33 |
| Never | 7.60 | 7.10–7.69 |
| EveryN(100) | 9.29 | 9.27–9.31 |
| accepted Event, no sink | 150.85 | 150.57–151.73 |

| Investigation public API, 7 × 500k calls | Producer | min–max | End-to-end |
| --- | ---: | ---: | ---: |
| rejected | 6.95 | 6.62–7.47 | — |
| accepted unit / no sink | 9.00 | 8.93–9.42 | — |
| accepted Event / no sink | 160.04 | 158.16–180.25 | — |
| discard sink, full record | 425.24 | 421.99–446.35 | — |
| MemorySink | 660.86 | 623.09–745.62 | — |
| async discard queue | 549.53 | 533.69–559.93 | — |
| JsonlSink, 1 producer | 669.10 | 664.92–695.96 | 670.27 |
| JsonlSink, 4 producers, aggregate | 253.91 | 236.41–281.12 | 830.11 |

Разница no-sink 151 vs 160 ns между executable/callsite/control layouts не является
регрессией или оптимизацией. Не сравниваем её как speedup: отдельные monomorphizations,
расположение кода и состояние allocator различаются. Оба результата подтверждают исходный
порядок ~150 ns. Для differential claims используются контроли внутри investigation.

Один producer даёт ~1.49M records/s; 4 producers enqueue ~3.94M/s, но заканчивают
persistence лишь ~1.20M/s. 253.91 ns — aggregate reciprocal throughput, **не latency
отдельного вызова одного из четырёх producers**.

Evidence: `performance/investigation_runtime_hot.csv`,
`performance/investigation.csv`.

## Methodology

### Artifacts and reproduction

Benchmark-only sources:

- [investigation.rs](../crates/sun-observations/benches/investigation.rs): public API,
  timestamp/envelope, Serde, queue, writer, ownership and bounded-batch controls.
- [investigation_allocations.rs](../crates/sun-observations/benches/investigation_allocations.rs):
  отдельный executable с counting allocator; **не используется для timing**.
- `performance/summary.csv`: medians, min/max, paired differences;
  `performance/summarize.py` использует только Python stdlib.

В Git Bash из repository root:

```sh
SUN_OBS_INV_REPEATS=7 cargo bench -p sun-observations --bench investigation
SUN_OBS_INV_REPEATS=9 SUN_OBS_INV_GROUP=confirm \
  cargo bench -p sun-observations --bench investigation
cargo bench -p sun-observations --bench investigation_allocations
SUN_OBS_BENCH_REPEATS=7 SUN_OBS_BENCH_GROUP=hot \
  cargo bench -p sun-observations --bench runtime
```

Каждый новый запуск автоматически сохраняет `metrics.csv` и `stderr.txt` в отдельной
`benchmarks`-подпапке рядом с активным Cargo profile directory; временные JSONL также
создаются внутри неё и удаляются после сценария. Редиректы в `performance/` больше не нужны.
Ссылки на `performance/` в этом историческом отчёте обозначают исходные snapshots;
удалённые файлы не восстанавливаются.

`SUN_OBS_INV_GROUP=micro|serde|writer|api` выбирает отдельную основную группу; без него
запускаются все четыре. `confirm` — отдельный follow-up, не включённый в основной запуск.

### Controls and boundaries

- Основные micro/API cases: 500k iterations/sample, 1024 warm iterations вне timer,
  7 повторений. Writer/service/queue ownership cases: 300k records. Batch 8192 cases
  с фиксированным числом batches используют 294912 records; batch 256 — 299776.
- Follow-up: 9 samples. Порядок caps 300000/256/8192 разворачивается на нечётных samples.
  Retained producer Serde controls: 100k small/rich и 10k text records/sample.
- Payload baseline: `Event { index: u64, value: f64 }`; fixed micro index 123456,
  payload 28 JSON bytes; full fixed envelope 192 bytes + LF = **193 bytes/record**.
  Macro persistence меняет index, process id и proc_ms; длина немного отличается.
- Rich payload: nested bool/integer, UTF-8/escaping string, 16 floats, 275 payload bytes;
  fixed envelope 439 bytes без LF. Text stress control: одно string field, **4235 JSON
  bytes** с quotes/backslash/newline/UTF-8; это counterexample, не workload приложения.
- Micro times включают destruction результата, если он не retained. Поэтому `to_value`
  ниже — materialization **и последующий drop**, не чистое время обхода Serialize.
  Retained controls отдельно измеряют создание без освобождения результатов в timer.
- Discard sink override `write_shared()` не копирует record. Это реальный engine path
  с envelope/Arc/fan-out, но без queue; record освобождается на producer.
- Service cases заранее создают **разные owned Arc<Record>**, затем быстро передают их
  настоящему JsonlSink. Timing enqueue не включает materialization; timing e2e включает
  concurrent processing, final flush barrier и освобождение records writer-ом. Worker/file
  уже прогреты. Это приближение saturated service rate, не isolated CPU profile.
- Prefilled controls воспроизводят snapshot → message Vec → serialize → batch write/drop
  на другом thread, без concurrent producers. Подготовка очереди, records, file open,
  thread spawn/join исключены. Timer включает queue drain, destruction, byte-buffer growth,
  encoding, write/flush. Периодические deadline/status/error branches не копируются;
  это модель для controlled comparisons, не подмена production service measurement.
- File validation после timer: count всех строк, parse first/last в `ObservationRecord`,
  проверка завершающего LF; настоящий sink также проверяется по records_written,
  compactions = 0. Equivalent-schema experiments проверяют **byte equality** typed,
  Value и raw layouts на данных workloads, включая escaping и optional-field omission.
- CSV `aux`: для API producer/service enqueue — records ещё **не отражённые в output
  stats**, для e2e — batches, для prefilled — число batches. Это **не queue.len()**:
  текущий writer batch тоже ещё не отражён в stats. Не выдаём эту величину за точную
  длину очереди или histogram persistence latency.
- Warm repeated batches дают allocator/cache reuse; retained/backlogged cases — большой
  working set. Эти времена нельзя безоговорочно складывать. Никаких вычитаний одного
  noisy sample, отбрасывания выбросов или утверждений о точных процентах CPU из wall time.
  Например, cached-byte giant-file write имеет один выброс 550 ns при median 84 ns;
  file service e2e — максимум 1055 ns при median 691 ns. Это сохранено в raw CSV.

## Accepted-path decomposition

Canonical behavior: [engine.rs](../crates/sun-observations/src/engine.rs),
`ObservationEngine::observe_if_active()`:

```text
cached_plan → admit → build → to_value(&payload) → message()
    → no sinks? return (Value drops)
    → record(timestamp, strings, proc_ms) → Arc → dispatch
```

### No sink

| Control | ns/call | Что измеряет |
| --- | ---: | --- |
| empty black_box | 0.21 | harness/compiler floor, не вычитается автоматически |
| payload construction only | 0.62 | два scalar fields; не произвольный application builder |
| payload + `to_value` + drop | 151.24 | 3 allocations и destruction Value |
| default `message()` with payload control | 0.50 | default None inline; практически нет callback work |
| non-inlined returning-message callback | 1.46 | контролируемый простой callback, без String allocation |
| payload + Value + returning-message callback | 152.18 | около +1 ns к serialization control |
| actual accepted Unit / no sink | 9.00 | admission/cache + allocation-free Null serialization |
| actual Event / no sink | 160.04 | все реальные admitted semantics |
| actual returning-message Event / no sink | 158.93 | отличие от Event ниже шума/кодогенерации |

**Почему ~150 vs ~7 ns:** admission control стоит около 7–9 ns; admitted Event
материализует BTreeMap-backed `Value` и освобождает его, даже без consumers.
~151 ns / ~160 ns = **примерно 94%** no-sink cost связано с payload/Value lifecycle;
~9 ns (~6%) — engine/control residual. Builder здесь <1 ns; default message фактически
бесплатен. Returning callback ~1–2 ns виден в micro, но не надёжно разрешается разницей
макро cases. Пользовательский callback/build может быть сколь угодно дороже.

В no-sink path **0 ns timestamp/envelope/Arc/queue**: они не вызываются. Устранение Value
теоретически приблизило бы этот workload к ~9 ns, но просто пропустить serialization
**нельзя**: ошибки Serialize и callback side effects являются текущим contract.
Это upper bound, не предлагаемая оптимизация.

Evidence rows: `micro.payload_*`, `micro.message_*`, `api.*no_sink` в main CSV.

## Timestamp/envelope cost

| Micro operation | Median ns/record | min–max |
| --- | ---: | ---: |
| `Utc::now()` | 37.01 | 36.62–38.04 |
| `Utc::now().timestamp_millis()` | 40.38 | 40.11–42.01 |
| `SystemTime::now()` → numeric ms | 27.93 | 27.72–28.17 |
| fixed UTC → RFC3339 millis + String | 66.06 | 64.00–76.78 |
| UTC acquisition + RFC3339 | 102.92 | 100.83–106.51 |
| monotonic elapsed → `proc_ms` | 28.69 | 28.33–28.96 |
| clone proc/scope/namespace strings + drop | 85.58 | 82.83–90.94 |
| full envelope with Null payload | 213.58 | 209.93–223.54 |
| same + Arc allocation/drop | 247.05 | 242.85–259.37 |
| full envelope with Event Value | 367.55 | 354.45–397.96 |
| same + Arc | 396.82 | 388.10–405.32 |
| empty-owned-fields record → Arc + drop | 34.74 | 33.76–37.07 |
| hot Arc clone/drop, existing record | 3.03 | 2.65–3.16 |

UTC + format + monotonic + 3 strings ≈217 ns vs actual Null envelope 214 ns.
**Record field assembly не имеет отдельной крупной стоимости**: struct moves/optional
fields входят в этот differential/noise. Arc addition ~30–35 ns: 33.5 ns на Null
control, 29.3 ns на full-record medians; independent empty-fields allocation — 34.7 ns.
Это allocation/free вместе, не только atomic refcount.

Приблизительная decomposition полного accepted/discard producer (425 ns):

| Component, alloc/free included | ns | Approx. share |
| --- | ---: | ---: |
| payload → Value lifecycle | 151 | 36% |
| wall clock + RFC3339 | 103 | 24% |
| monotonic proc_ms | 29 | 7% |
| envelope strings lifecycle | 86 | 20% |
| Arc allocation/free | ~30 | 7% |
| engine/dispatch/codegen residual | ~27 | 6% |

Это accounting estimate, не exact stack profile. Особенно **не применять эти доли
напрямую к async producer 669 ns**: там final destruction переносится на writer,
а allocations не получают такой же immediate same-thread recycling.

### Allocation evidence

`performance/investigation_allocations.csv`:

| Operation | alloc | realloc | free | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| small T → Value + drop | 3 | 0 | 3 | 642 |
| small T → raw Box + drop | 1 | 1 | 1 | 156 |
| chrono RFC3339 String | 1 | 0 | 1 | 38 |
| clone ts/proc/scope/namespace | 4 | 0 | 4 | 81 |
| full record clone | 7 | 0 | 7 | 723 |
| fixed envelope + Event Value + Arc | 8 | 0 | 8 | 931 |
| encode record into reused Vec | 0 | 0 | 0 | 0 |

Requested bytes — сумма allocation/reallocation requests, не process RSS. Fixed cloned
ts занимает 24 bytes; production chrono String резервирует 38, поэтому этот fixture
не является точным heap-size снимком production envelope. Два Value keys плюс BTreeMap
node объясняют **642 bytes allocations для 28 bytes payload**. Точное число allocations
зависит от формы payload; здесь это важнее пары scalar stores record construction.

## Async queue cost

| Targeted control | Median ns/record | Boundary |
| --- | ---: | --- |
| scalar unbounded send, paused consumer | 9.12 | no Record / Arc; bulk send lower control |
| scalar send, active consumer | 8.60 | consumer may remain backlogged; не worst-case wakeup |
| actual JsonlSink `write_shared`, preallocated unique records | 23.35 | move owned Arc, worker active |
| same control, follow-up | 21.43 | 9 samples |
| preallocated cold records + engine-style Arc clone/drop | 86.96 | touches cold refcount cache lines |
| synth full records retained in Vec, no queue | 633.32 | construction/allocations; no destruction in timer |
| same construction + paused queue send | 640.99 | full-record controlled enqueue comparison |

У последних двух строк paired median increment **24.56 ns** (range −7.83…36.47);
разница medians лишь 7.67 ns. То есть не разрешаем точный sub-20-ns эффект из noisy
allocation workload; прямые enqueue controls подтверждают порядок **десятков**, не
сотен ns. Cold Arc refcount control не превращается в «87 ns pure send»:
preallocated сотни MiB records не имеют горячих cache lines свежесозданного Arc.
На hot existing Arc clone/drop отдельно ~3 ns.

Реальная разница JSONL producer vs no sink — paired median **+508 ns**; vs discard —
**+244 ns**. Это **не Sender::send cost**. JsonlSink добавляет owned envelope, timestamp,
Arc/fan-out, error-status/lazy-worker checks и передачу ownership. В отличие от discard,
allocations живут дольше и освобождаются на consumer, меняются allocator/cache reuse и
working set. MemorySink без async writer уже стоит 661 ns: один лишь envelope/send
не объясняет разницу.

Синтетический active-discard producer дал 678 ns в follow-up, а public-API
async-discard — 550 ns в main. Это полезное предупреждение: runtime interleaving,
allocation reuse и кодогенерация меняют absolute producer time; вычитание этих разных
harness не даёт точной lock/queue decomposition. Контролируемая retain vs paused-queue
пара и прямой `write_shared` — более корректная оценка enqueue.

Source: [channel.rs](../crates/sun-observations/src/channel.rs) `dispatch()` делает один
Arc clone на sink; [jsonl.rs](../crates/sun-observations/src/sinks/jsonl.rs)
`write_shared()` передаёт Arc, **не deep-clones ObservationRecord**. Один JsonlSink —
один lazy writer thread, общий для всех producers; это поведение не менялось.

## Writer decomposition

### Actual writer service

| Actual JsonlSink, prebuilt records, 7 samples | ns/record | M records/s | min–max ns |
| --- | ---: | ---: | ---: |
| null output, same shared Arc repeatedly | 233.25 | 4.29 | 210.00–293.93 |
| null output, distinct owned records | 593.44 | 1.69 | 571.19–653.92 |
| cached file, distinct owned records | 690.62 | 1.45 | 655.95–1054.64 |
| file control repeated in 9-sample follow-up | 676.79 | 1.48 | 639.02–742.00 |

Shared-Arc case имеет ту же JSON schema, но **не тот же ownership lifecycle**:
нет уничтожения каждого payload/envelope. Нельзя рекламировать 4.29M/s как скорость
обычной persistence. Этот контроль выявляет стоимость destruction/cache footprint.

На конце быстрого prebuilt enqueue ~299792 из 300000 records ещё не отражены в output
stats; materialization отсутствует в producer timer. Это действительно насыщает writer,
а не измеряет producer-limited feed. Реальный 4-producer macro workload медленнее:
**830 ns / 1.20M/s**, min–max 808–970 ns. Одновременная materialization и большие
ownership working sets значимы; capacity нельзя считать одной hardware-константой.

### CPU, ownership and buffered I/O controls

| Controlled writer operation | ns/record |
| --- | ---: |
| hot full record → reused JSON byte Vec + LF | 169.47 |
| encode batches 1 / 256 / 8192, reused capacity | 180.56 / 179.03 / 180.00 |
| append cached complete JSON line to Vec | 6.09–7.10 |
| prefilled shared Arc queue drain/drop, no encoding | 18.11 |
| prefilled unique records drain/drop, no encoding | 343.52 |
| same unique records + encoding, no output | 590.39 |
| same + BufWriter<File> write + final flush | 705.84 |
| copy cached bytes to file, batch 1 / 256 / 8192 | 62.13 / 75.94 / 40.97 |
| one 300000-record / ~58 MB cached-byte write | 84.46 |
| empty BufWriter<File>::flush | 3.31 ns/flush |

Воспроизведённый big-prefix writer budget около 706 ns:

- ~18 ns queue/drain/refcount control;
- ещё ~325 ns unique-record destruction / pointer walks / deallocation / working set;
- ещё ~247 ns encoding + cold-record/cache/buffer-growth work; hot encoding отдельно ~169 ns;
- ещё ~115 ns file/buffer-write service в этой паре.

Эти differential estimates **не точный additive CPU profile**. Но independent actual
sink null → file отличается лишь ~97 ns, а unique vs shared — ~360 ns. Вывод устойчив:
writer здесь преимущественно **CPU/heap/memory-lifecycle limited**, не queue limited
и не ограничен исключительно файловой системой. JSON encoder — важная, но не большая
часть всего пути. Storage остаётся измеримой частью (~14–16% этих контролей) и на другом
устройстве, под другой I/O нагрузкой, с fsync или большими records может стать главным.
Prebuilt file service соответствует ~280 MB/s JSONL bytes, тогда как cached-byte controls
дают ~2.3 GB/s для giant write и ~4.7 GB/s для повторяемого batch 8192. Это application/OS-cache
rates, не физическая скорость устройства: текущий writer не генерирует bytes достаточно быстро.

Ориентировочные отдельные ceilings относительно 691 ns actual prebuilt service:

| Полностью устранённая стоимость, всё остальное неизменно | Saving | Maximum capacity ratio |
| --- | ---: | ---: |
| unique-record lifecycle сверх shared control | ~325 ns | ~1.9× |
| hot full-envelope encoding | ~169 ns | ~1.32× |
| file-service difference vs null output | ~97 ns | ~1.16× |
| queue/drain shared control | ~18 ns | ~1.03× |

Это counterfactual bounds, не независимо достижимые speedups: они перекрываются,
меняются вместе с locality, а ownership нельзя просто удалить/не освобождать.
Особенно 1.9× не является обещанием allocator/pooling optimization. Измеренный raw
control и bounded batches ниже — более реалистичные конкретные операции.

Destruction-only controls без queue/encoding:

| Owned object | Same-thread drop | Other-thread drop, ns/object |
| --- | ---: | ---: |
| Value payload | 155.26 | 142.67 |
| four envelope strings | 89.63 | 88.38 |
| Arc + Null envelope | 151.37 | 155.00 |
| Arc + full record | 342.29 | 353.92 |

Таким образом, evidence **не подтверждает особый огромный cross-thread-free penalty**:
большие owned sets дороги и на allocating thread. Не приписываем это дефекту Windows heap
или одному atomic decrement. Immediate allocate/drop micro и bulk retained drop имеют
принципиально разную locality и allocator reuse.

### Flush and file-service interpretation

При batch 256 дополнительный flush в каждом batch дал 78.58 vs 75.94 ns/record;
при 8192 — 39.09 vs 40.97 ns. Эффект около шума: File::flush не обеспечивает durable
sync, а большие batches уже обходят внутреннюю 64-KiB BufWriter buffer. Default periodic
flush и один terminal barrier не объясняют ~0.7–0.8 us/record.

Async `Sink::flush()` также создаёт reply channel, ждёт FIFO barrier и может ждать
накопившийся backlog. **3.31 ns не его latency**; это только пустой BufWriter flush.
Отдельный idle thread-wakeup/round-trip профиль не измерялся. Его нельзя вывести из
producer→e2e разницы: там смешаны queue tail, незаписанный batch и flush acknowledgement.
Не менять interval ради этих цифр; synchronous per-observation barriers были бы другой
нагрузкой и здесь не тестировались.

### Batch accumulation hypothesis

Текущий [jsonl_writer.rs](../crates/sun-observations/src/sinks/jsonl_writer.rs) `run()`
снимает весь `receiver.len()` после первого receive, складывает prefix в messages,
затем сериализует его целиком в один growing byte Vec. Это уже batching, не write syscall
на каждый record, но при backlog размер batch практически не ограничен.

| Prefilled unique records → file | Main, 7 samples | Reversed-order confirmation, 9 samples (min–max) |
| --- | ---: | ---: |
| all 300000 records in prefix | 705.84 | 710.45 (673.84–754.31) |
| cap 256 records | 625.45 | 647.49 (615.10–737.81) |
| cap 8192 records | 569.62 | 585.81 (561.23–618.16) |

8192 saves paired median **123.26 ns**, range 63.49–172.41, positive in every confirmation
sample. Median-to-median −17.5% time, 1.41 → 1.71M/s (+21%). 256 is less effective:
more small file writes. No-output main comparison 590.39 → 526.73 ns also improves:
это не только filesystem write size; ограничение temporary working set/reallocation
и cache behavior имеет значение. Точная доля каждого не установлена.

Для small schema содержимое byte batch порядка 1.5 MiB вместо ~55 MiB плюс message Vec;
Vec capacity может округляться выше (примерно до 2 и 64 MiB). Это не bound общей unbounded queue. 8192 не объявляется универсальной константой для
произвольного payload. Контроль исключает concurrent allocation, поэтому **не заявляем
+21% у production workload** без последующего targeted implementation/validation.

## Value / double-serialization experiment

Одни и те же поля, числа, escaping, field order и omitted fields проверены byte equality.
Generic borrowed envelope устраняет различия владения строками в encoding micro;
полный owned raw writer control ниже отдельно возвращает destruction.

| Serialization operation, ns/record | Small (28-B payload) | Rich (275-B payload) |
| --- | ---: | ---: |
| T → Value (+ drop) | 151.68 | 773.20 |
| prebuilt Value → payload JSON, reuse Vec | 47.80 | 477.42 |
| typed T → payload JSON, reuse Vec | 37.94 | 420.58 |
| T → allocated raw JSON Box (+ drop) | 107.44 | 598.74 |
| Value envelope → JSON | 184.61 | 608.04 |
| raw envelope → JSON, embed raw bytes | 133.06 | 135.24 |
| typed envelope → JSON, one pass | 170.09 | 564.94 |
| measured T → Value → full JSON | 349.94 | 1418.42 |
| measured T → raw → full JSON | 248.18 | 751.42 |

Нельзя складывать отдельные median rows как exact composite: compiler, allocations,
lifetimes и buffer state отличаются. Поэтому two-stage/full и raw/full **измерены
как цельные цепочки**, а не вычислены суммой компонентов.

- Raw composite small: −101.76 ns (~29%); rich: −667 ns (~47%). Это serialization
  subsystem, **не speedup всего observe/persistence**.
- Typed one-pass full envelope теоретически исключает примерно 180 ns small / 853 ns rich
  относительно two-stage composite (около 51% / 60%). Это upper control без enqueue,
  deferred-object lifetime, dynamic dispatch, boxes, clocks и новых public contracts.
- Простой Value → JSON не в разы дороже typed → JSON: payload-only 48 vs 38 ns small,
  477 vs 421 ns rich. Главная проблема intermediate — **materialization, allocations
  и lifetime/destruction**, а не магически медленный обход Value на втором проходе.
- Удаление всего hot full-envelope encoder ~169 ns из actual 691 ns service — optimistic
  ceiling около 1.32× capacity, без изменения остальных операций. Удаление только
  payload encoding ~48 ns — около 1.07×. Эти ceilings не описывают Raw experiment,
  который меняет ещё и число allocations/working set.

### Variant A: serialize raw on producer

Producer-only controls удерживают результаты, поэтому не включают frees в timer:

| Producer materialization | T → Value | T → allocated raw | Effect |
| --- | ---: | ---: | --- |
| small | 214.84 | 110.14 | raw saves ~105 ns |
| rich | 967.47 | 643.73 | raw saves ~324 ns |
| escaped text, 4235 JSON bytes | 475.30 | 3427.55 | raw **adds ~2952 ns; 7.2× slower** |

Текстовый контрпример также проигрывает суммарно: payload-only T → Value → JSON
2727.50 ns vs T → raw → embedded JSON 3224.56 ns (**+18%**). Результаты equivalent-schema,
9 samples; не противопоставляем разные структуры или final JSON layouts.

Value может дешёво скопировать строку, оставив escaping writer-у; raw обязан сделать
escaping уже на producer. Поэтому выигрыш small/rich **не разрешает** обещать отсутствие
producer regression для всех Observation payloads.

Owned raw writer model (same full schema, fresh raw records prepared outside timer):

| Follow-up, 9 samples | Value ns/record | Raw ns/record |
| --- | ---: | ---: |
| all-prefix file writer | 710.45 | 474.28 |
| 8192-cap file writer | 585.81 | 357.56 |

All-prefix raw выигрывает ~33% времени (paired saving 230.66 ns), capacity control
1.41 → 2.11M/s; bounded raw — 1.71 → 2.80M/s. Здесь возвращены record destruction
и filesystem, так что это сильнее одного encoding micro. Но это **не реализация
Raw JsonlSink** и не end-to-end измерение с producer materialization. Помимо encoding,
raw устраняет Value keys/node frees и существенно уменьшает owned payload footprint.

Препятствия к production adoption:

- Публичный `ObservationRecord.data: Value`, `Sink::write[_shared]`, MemorySink snapshots,
  пользовательские sinks и data indexing уже требуют materialized Value.
- Нужны совместимость или новая private representation с materialization для Value
  consumers; fan-out Memory+JSONL может вернуть двойную работу, а eager fallback —
  ухудшить producer. Public API break ради ~1M/s без нагрузки не оправдан.
- `Serialize` может быть custom/stateful; нельзя сериализовать payload повторно, менять
  наблюдаемые ошибки/порядок callback или считать byte equality двух workloads полной
  certification произвольных serializers/map ordering/non-finite numbers.
- Numeric payload benchmarks показывают возможность producer improvement, но text case
  прямо нарушает требование не переносить тяжёлую работу на producer threads.

**Recommendation A:** не внедрять глобально сейчас. Вернуться только с representative
payload distribution и доказательством producer budget; возможный JSONL-only opt-in —
отдельная design task, а не бесплатная замена Value.

### Variant B: typed/lazy until writer

One-pass typed numbers выше показывают большой теоретический резерв, но текущий
`Observation: Serialize + 'static` **не требует Send/Sync**; исходный `build()` и
`message()` работают синхронно на producer. Передача произвольного typed T на другой
thread требует нового Send boundary, owned `'static` data, type-erased object/serializer
callback (Serialize не является простым object-safe trait), allocation и нового sink
contract. MemorySink/custom sinks всё равно хотят Value. Serialization failure сейчас
возвращается из observe!, а при deferral станет async failure — semantic change.

**Recommendation B:** не считать приемлемой локальной оптимизацией. При текущих invariants
она требует другого дизайна и contract decisions, а не удаления одного вызова Serde.

## Deferred timestamp experiment

Production создаёт RFC3339 millis на producer. Numeric UTC ms получает тот же wall-clock
sample за 40.38 ns вместо 102.92 ns строки: optimistic saving **62.54 ns/record**.
SystemTime numeric control 27.93 ns обещает до 74.99 ns, но потребует ещё преобразования
на writer; это другой internal representation, не бесплатная замена public String.

Writer control из сохранённого numeric timestamp делает `from_timestamp_millis`,
RFC3339 и эквивалентный envelope: **270.15 vs 169.47 ns**, paired increment **99.57 ns**.
Разница с isolated formatting 66 ns включает conversion/temporary String/drop и
codegen/lifetime effects; использовать только 66 ns как writer penalty было бы оптимистично.

Accounting estimate, **не реализованный end-to-end эксперимент**:

| Path | Current | With deferral estimate |
| --- | ---: | ---: |
| single-producer API latency | 669 ns | ~607 ns (−9%) |
| saturated prebuilt writer service | 691 ns / 1.45M/s | ~790 ns / 1.27M/s |
| loaded 4-producer writer e2e | 830 ns / 1.20M/s | ~930 ns / 1.08M/s |
| admitted/no sink | 160 ns | unchanged |

Перенос сохраняет numeric instant только при acquisition **на producer**, не при writer
processing: иначе ts меняет значение под backlog. Требуются новые internal/public-record
границы; MemorySink всё равно ожидает RFC3339 String. Избавление от String allocation на
producer уменьшает часть allocation traffic, но numeric record и memory lifecycle реально
не внедрялись: не обещаем точного service-rate изменения из этой простой суммы.

**Verdict: only worth implementing if producer latency is priority.** Для текущего
single-writer bottleneck это ухудшает capacity; общего throughput выигрыша не ожидается.
У no-sink вообще нет этого bottleneck. Сейчас не внедрять.

Более локальная альтернатива в benchmark: per-thread cache formatted text по numeric
millisecond, всё равно клонирующий String на record. Same schema, bytes сверены на
millisecond/second boundaries и negative UNIX times. `Utc::now()+cache` **74.55 vs
102.92 ns**, paired saving **27.98 ns** (~4% single-producer JsonlSink cost). Работа не
переносится на writer, public record не меняется. Но это high-hit-rate control; при
редких наблюдениях на каждом из многих producers cache misses и лишний clone могут
убрать выигрыш. Низкие rates/много producers здесь не валидированы. Не выдавать его за
обязательную production оптимизацию или за producer saving deferred-formatting целиком.

## Capacity analysis

Это capacity analysis, **не утверждение о SunTrader rates**. Utilization = arrival rate
× measured wall service time; не profiler CPU percentage. Берём 830 ns для concurrent
4-producer scenario и 691 ns для prebuilt/cached-file control:

| Arrival rate / JsonlSink | Utilization at 1.20M/s | At 1.45M/s | Interpretation |
| --- | ---: | ---: | --- |
| 10k/s | 0.83% | 0.69% | tuning writer практически не нужен |
| 100k/s | 8.30% | 6.91% | большой запас |
| 500k/s | 41.51% | 34.53% | запас, но bursts/payload size важны |
| 1M/s | 83.01% | 69.06% | headroom уже не гарантирован |
| 1.25M/s | 103.76% | 86.33% | loaded case backlog растёт |
| 1.5M/s | 124.52% | 103.59% | оба режима перегружены |
| >1.5M/s sustained | >125% | >104% | один текущий writer не успевает |

При 4 producers конец producer timer имеет median ~432849 ещё не committed-to-output
records из 500k (часть в активном batch). Один writer не масштабируется с enqueue.
Даже optimistic 931 requested allocation bytes на fixed owned fixture дают порядка
400 MB для такого множества pending records, без allocator overhead, queued messages и
batch bytes. Это иллюстрация working-set риска, **не измеренный RSS**.

При 1.5M/s и loaded capacity 1.20M/s backlog растёт примерно на 295k records/s.
Unbounded queue делает краткий producer benchmark быстрым, но не создаёт persistence
capacity. Снижение writer cost на 17% полезно в этом режиме, но почти ничего не даёт
приложению с постоянными 10k/s. Длинные production payloads, antivirus/storage load и
durability contracts могут изменить всю таблицу; real application rate не измерялся.

## Recommendations

### Optimize now

**Без measured application pressure — никаких production оптимизаций сейчас.** Лучший
следующий маленький change при подтверждённой persistence нагрузке:

1. **Ограничить writer drain/byte batch**, не менять producer или JSON schema.
   - Evidence: 9-sample reversed-order control 710 → 586 ns/record; paired saving
     123 ns, положительный во всех samples. Current backlog batch может быть десятки MiB.
   - Expected benefit: ориентир **~15–20% writer service time**, до ~20% capacity на
     подобных backlogged small records; реальный gain обязан пройти actual sink run.
     При producer-limited/low-rate нагрузке может не быть выигрыша.
   - Affected path: только background writer; producer не сериализует ничего нового.
   - Complexity: low, небольшой bounded chunk в existing single writer; без новой
     архитектуры, без дополнительного public tuning API до появления необходимости.
   - Main tradeoff: больше batches/writes; слишком маленький batch (256) менее выгоден.
     Сохранить FIFO, flush acknowledgement after preceding records, periodic deadlines,
     error propagation и shutdown draining. 8192 (~1.5 MiB здесь) — экспериментальная
     отправная точка, не универсальное значение и не queue memory bound.
   - Это **рекомендация, не production implementation**. Перед merge проверить реальный
     sink с 1/4 producers и разными payload sizes, а не только prefilled model.

Условный второй, если profiling приложения показывает именно producer latency:

2. **Small per-thread RFC3339 millisecond cache**, не deferral.
   - Expected benefit: **~28 ns/record** при высоком per-producer hit rate (~4% текущего
     single-producer JsonlSink time); writer не получает дополнительной работы.
   - Complexity: low/moderate, TLS state; public record/schema остаются прежними.
   - Tradeoff: cache misses/low per-thread rates могут ухудшить результат. Нужны cold/miss,
     wall-clock-backstep и многопоточные проверки. Сейчас не внедрять без producer-pressure
     evidence: выигрыш мал, hot-path extra state требует отдельного justified change.

### Do not optimize now

- **Rejected/selector path:** 7 ns, нет нового evidence проблемы; warm selector scale
  уже решён. Это исследование не повторяет 40k-rule suite и не заявляет нового scale claim.
- **Skip serialization in no-sink:** нарушит observable Serialize errors/side effects;
  даже ~17× theoretical control speedup не даёт права менять semantics.
- **Universal raw-payload / lazy typed redesign:** small/rich выигрывают, text raw хуже
  producer в 7.2×; MemorySink/public Value API и sync errors не решаются бесплатно.
- **Deferred RFC3339:** лишь перенос CPU на уже ограничивающий writer; +~100 ns writer
  за −~63 ns producer, новые representation boundaries. Не общий throughput win.
- **Custom allocators, pooling, record recycling:** lifecycle крупный bottleneck, но его
  полное устранение (~325 ns writer control) — upper bound, не low-complexity fix.
  Не вводить pools/cross-thread reclaim или process-wide allocator без нового evidence.
- **String interning / Arc<str> public fields:** envelope strings имеют ощутимую цену
  (~86 ns alloc/free и ~88 ns bulk drop), но public String contract не менять ради micro.
- **Arc refcount / queue replacement:** hot clone ~3 ns, pure bulk enqueue ~10–25 ns,
  shared queue drain ~18 ns. Даже полное устранение 18 ns из 691 ns даёт лишь ~3% capacity.
  Не путать 8 frees record tree с одной операцией Arc refcount.
- **More buffering / flush tuning наугад:** writer уже batching + 64-KiB BufWriter;
  cached-line append ~6–7 ns, empty file flush ~3 ns. Следует ограничить huge batches,
  а не бесконечно наращивать их или менять persistence interval.
- **MemorySink sharding / EveryN contention:** в этой задаче нет production relevance
  evidence; EveryN(100) warm ~9 ns, MemorySink — отдельный retained consumer. Сложность
  и semantic changes не оправданы результатами accepted/JSONL исследования.
- **Multiple writers на один JSONL file, retention/compaction, integration/certification:**
  вне scope; не решение single-writer serialization/ownership budget.

### Revisit when

- Один sink устойчиво принимает **>~1M small records/s**, backlog регулярно не успевает
  исчезать между bursts или terminal flush длится дольше application latency budget.
  ~1.2M/s — измеренная loaded capacity, не безопасный production SLA.
- Producer observation overhead составляет **>5% application runtime**, подтверждённый
  application profile, а не synthetic throughput. Тогда сначала representative builders,
  payload shapes и per-thread rate; cache имеет смысл только при достаточном hit rate.
- Representative payloads larger/numeric-heavy, persistence CPU существенен, и JSONL-only
  routing реально доминирует: повторить raw retained-producer + owned-writer сравнение,
  включая text counterexamples, Memory fan-out и error/API compatibility.
- MemorySink становится production consumer и измерена contention; не переносить сюда
  выводы о background destruction или считать его performance target по умолчанию.
- Меняется durability/storage contract: заново измерить cached I/O vs fsync/storage service.

## Validation and change boundary

Production `.rs` source, JSONL format, scope/policy/routing и worker semantics **не менялись**.
Изменения: два standalone bench targets, dev-only serde_json `raw_value` feature для
эквивалентных controls, этот report и `performance/` evidence. Cargo.lock не менялся.
Counting allocator существует только в allocation-probe executable.

- Main optimized benchmark: все 7 samples, file/schema/count/LF assertions прошли.
- Follow-up optimized benchmark: все 9 samples, including text counterexample и reverse order.
- Existing optimized runtime hot benchmark: 7 samples.
- `cargo test --workspace --all-features`: **65 + 2 unit tests, 5 doctests**, all passed;
  unit suite включает trybuild fixtures. `performance/tests.txt`.
- `cargo clippy --workspace --all-features --benches -- -D warnings`: passed;
  `performance/clippy.txt`.
- `cargo fmt --all -- --check`: passed; `performance/fmt.txt` пуст при успехе.
- `git diff --check`: passed; diff production source и Cargo.lock пуст.

Итог: уже выполненные rejected/selector оптимизации не нужно продолжать по инерции.
Следующий оправданный reserve — **writer batch working set**, если нагрузка требует.
Большой потенциальный Value/lifetime выигрыш реален, но **не универсален и не бесплатен
для producer/public contracts**. Без backlog или application-profile pressure разумнее
остановиться на текущей реализации.
