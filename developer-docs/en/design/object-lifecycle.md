# Object publication and reclamation

The content hash is the logical `(hash, policy_id, backing)` deduplication identity. A physical object key identifies one immutable generation owned by a publication attempt. Failed attempts must not delete a shared content-hash path.

`aster_drive_storage::object_path` owns blob, upload-session, part, stream/relay/copy/promotion staging and image-derivative names and classifiers. Provider-prefix composition remains in `object_key`; Forge owns operating-system scratch roots. Callers use the central factories directly, without obsolete naming wrappers.

A warm deduplication hit locks and validates the current blob in the file/quota transaction and atomically acquires its reference. It performs no connector publication or object copy. Losing to cleanup falls back to staging a private candidate outside the transaction. Cold uploads and migrations write and verify their own random candidates first, then use database uniqueness and source-state CAS to publish metadata. Local candidates retain two-level directory fanout. Losers delete only their unused candidates; uncertain commits retain candidates.

Both streaming and multipart migrations verify the candidate's full length and digest before publishing metadata. A competing deduplication winner is verified and merged rather than overwritten or deleted.

Cleanup atomically claims `ref_count = 0 -> -1`. Repair and failed deletion never reopen that generation for adoption. Expired claims retry the same physical generation after a locked recount, using the existing ten-minute window and renewing the attempt timestamp. Metadata retirement compares the row ID, cleanup state and persisted physical path.

Historical policies can share one local object. Cleanup uses the writer's `idx_file_blobs_storage_path` lookup and resolved local paths to find other owners, including zero-reference and cleanup-claimed rows. It retires only its own metadata while another owner remains. More than 128 candidates, or ambiguous non-local identity, conservatively retain bytes with bounded allocation. Concurrent retirement can leave an orphan; `retained_shared` does not report physical bytes freed.

Provider-resumable completion atomically commits Completed/file_id and clears `object_temp_key`, transferring cleanup authority. Completed progress uses committed state. Historical Completed sessions cannot authorize deletion during team archive. Incomplete sessions first require a writer status CAS; regular file/blob purge owns permanent content reclamation.

Doctor, automatic reconcile and background blob maintenance share the same locked recount implementation. MySQL locks before establishing a consistent read snapshot; SQLite obtains its write lock through a no-value-change UPDATE. Full repair keeps healthy scans batched and locks only suspected drifts. Old audit counts are never accepted as write authority.

Quota reports are observations too. `repair_storage_usage` locks a suspected owner's quota counter in a short transaction, recounts current files and historical revisions, then updates it. Publication uses the same owner lock, so repair cannot overwrite a concurrent commit with an old report. Healthy owners remain batched: both 1-owner and 64-owner PostgreSQL/MySQL fixtures use 5 SQL statements.

Replacement writes retain the existing blob's storage destination but enforce current group enabled/size/extension/category admission before provider I/O and again in the publication transaction. Streaming receive limits apply before hashing or writing. Admission reads the policy snapshot without extra per-file database queries.

Slow I/O stays outside database write transactions. Hashing uses a 64 KiB buffer; cross-device promotion uses a fixed 1 MiB buffer. In the SQLite fixture, the warm publication phase used 29 SQL statements, no connector calls and no copied bytes for both 1 KiB and 1 MiB inputs. This is structural/query evidence, not a production latency or RSS benchmark.

Existing objects remain readable by their persisted paths without moving stored content. Upgrade all writers together so old writers cannot recreate shared hash paths. Real-provider and production rollout acceptance remain separate.
