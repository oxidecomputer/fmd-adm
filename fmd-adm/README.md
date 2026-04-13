# fmd-adm

Idiomatic Rust bindings for illumos
[libfmd_adm](https://illumos.org/man/3LIB/libfmd_adm), the Fault
Management Daemon administrative library.

This crate provides a safe wrapper around the raw FFI bindings in
[fmd-adm-sys](../fmd-adm-sys/).

## Usage

```rust
use fmd_adm::FmdAdm;

let adm = FmdAdm::open().expect("failed to connect to fmd");

for module in adm.modules().unwrap() {
    println!("{} v{} - {}", module.name, module.version, module.description);
}

for case in adm.cases(None).unwrap() {
    println!("{} - {}", case.uuid, case.code);
}
```

## API overview

All operations go through the `FmdAdm` handle, which connects to the local
fault management daemon on construction and disconnects on drop.

- **Modules**: `modules()`, `module_load()`, `module_unload()`,
  `module_reset()`, `module_gc()`
- **Resources**: `resources()`, `resource_count()`,
  `resource_repaired()`, `resource_replaced()`, `resource_acquit()`,
  `resource_flush()`
- **Cases**: `cases()`, `case_repair()`, `case_acquit()`
- **SERD engines**: `serd_engines()`, `serd_reset()`
- **Transports**: `transports()`
- **Statistics**: `stats()`
- **Log rotation**: `log_rotate()`

## Privileges

Opening a handle does not require elevated privileges, but most query and
mutation operations do. Run with `pfexec` or appropriate RBAC profiles to
access fmd data.
