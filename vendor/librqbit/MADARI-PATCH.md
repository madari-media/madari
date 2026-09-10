Based on crates.io librqbit 9.0.1 (upstream source and license retained).

Madari patch: storage/filesystem/opened_file.rs uses the existing buffered
FileExt write fallback on 32-bit Unix. nix pwritev accepts off_t, which is i32
on Android ARMv7, and converting offsets beyond 2 GiB fails. FileExt's u64
positioned writes use the platform large-file API. 64-bit Unix keeps pwritev.

Verified on Google TV ARMv7 with a sparse-file write/read at i32::MAX + 4096,
using both IoSlice buffers from the patched method. The fixture was removed
after verification. An equivalent regression test is included in the module.

Madari patch: storage/filesystem/fs.rs reuses an existing file instead of
failing when one is already present. The unpatched path opens every file with
create_new(true) unless allow_overwrite is set, so re-adding a torrent whose
file is already on disk aborts with EEXIST. That reaches the user as the bare io
error "entity already exists" and stops playback before either player sees a
byte, which made any source that had been played once unplayable. Only
AlreadyExists is treated as reuse: the file is then opened read/write without
truncation, so downloaded data survives and nothing is overwritten, while piece
hashes still catch anything stale or incomplete. Every other error is reported
as before, so allow_overwrite keeps its original meaning.

Verified with cargo check -p madari-media. Playback of an already-downloaded
source on device is the outstanding confirmation.
