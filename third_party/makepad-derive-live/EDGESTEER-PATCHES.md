# EdgeSteer Makepad Patch

This is `makepad-derive-live` 1.0.0 from crates.io, retained under its
original `MIT OR Apache-2.0` license. EdgeSteer uses this local copy through
Cargo's `[patch.crates-io]` mechanism because Makepad's `live_design!` macro
otherwise embeds the absolute `CARGO_MANIFEST_DIR` path in every GUI binary.

When `EDGESTEER_LIVE_MANIFEST_PATH` is set, the macro records that stable
virtual path instead. Normal development builds leave the variable unset and
retain Makepad's original live-reload behavior.
