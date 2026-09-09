Based on crates.io librqbit 9.0.1 (upstream source and license retained).

Madari patch: storage/filesystem/opened_file.rs uses the existing buffered
FileExt write fallback on 32-bit Unix. nix pwritev accepts off_t, which is i32
on Android ARMv7, and converting offsets beyond 2 GiB fails. FileExt's u64
positioned writes use the platform large-file API. 64-bit Unix keeps pwritev.

Verified on Google TV ARMv7 with a sparse-file write/read at i32::MAX + 4096,
using both IoSlice buffers from the patched method. The fixture was removed
after verification. An equivalent regression test is included in the module.
