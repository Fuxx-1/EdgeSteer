# EdgeSteer Makepad Patch

This is `makepad-widgets` 1.0.0 from crates.io, retained under its original
`MIT OR Apache-2.0` license. EdgeSteer uses this local copy through Cargo's
`[patch.crates-io]` mechanism because the released desktop light theme points
its CJK and emoji font entries at files that are not included in the crate.

The desktop and mobile themes now use the same declared font crate URIs as the
font registration in `makepad-widgets`. The desktop light theme also uses a
dark text token and visible inset borders so Chinese labels and form fields
remain readable on white surfaces. No widget behavior or public API is
changed.
