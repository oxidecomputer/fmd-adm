# fmd-adm-sys

Raw Rust FFI bindings to illumos
[libfmd_adm](https://illumos.org/man/3LIB/libfmd_adm), the Fault
Management Daemon administrative library.

## How the bindings were generated

The bindings in `src/lib.rs` were generated using
[bindgen](https://github.com/nickel-org/rust-bindgen) against the illumos
system header `/usr/include/fm/fmd_adm.h`. The `wrapper.h` file in this
directory documents the header that was used as input.

Because `fmd_adm.h` depends on other illumos system headers (notably
`fmd_api.h`, `libnvpair.h`, `sys/types.h`, and `door.h`), bindgen must be
run with access to the full illumos header tree. The bindings were generated
on a Linux machine using headers copied from an illumos system:

```bash
# Copy /usr/include from an illumos machine to a local staging directory
rsync -az --include='*.h' --include='*/' --exclude='*' \
  user@illumos-host:/usr/include/ /tmp/illumos-headers/

# Generate bindings (using -nostdinc to avoid mixing Linux and illumos headers)
bindgen wrapper.h \
  --allowlist-function 'fmd_adm_.*' \
  --allowlist-type 'fmd_adm_.*' \
  --allowlist-type 'fmd_stat_t' \
  --allowlist-type 'fmd_stat' \
  --allowlist-type 'fmd_prop_t' \
  --allowlist-type 'fmd_prop' \
  --allowlist-var 'FMD_ADM_.*' \
  --allowlist-var 'FMD_TYPE_.*' \
  -- -nostdinc \
  -isystem "$(clang -print-resource-dir)/include" \
  -I/tmp/illumos-headers \
  > src/lib.rs
```

The generated output is checked into `src/lib.rs` directly (following the
pattern established by
[libefi-sys](https://github.com/oxidecomputer/libefi-sys)). The `build.rs`
only contains linker directives — bindgen is not invoked at build time.

## Keyword renaming

The `fmd_stat_t` union in the C header contains fields named `bool`, `i32`,
`i64`, and `str`, which are reserved keywords in Rust. Bindgen automatically
renames these by appending an underscore: `bool_`, `i32_`, `i64_`, `str_`.

## Linking

The library is at `/usr/lib/fm/amd64/libfmd_adm.so` on illumos. The
`.cargo/config.toml` at the workspace root sets the runtime library search
path via `-R/usr/lib/fm/amd64`.
