// SPDX-License-Identifier: AGPL-3.0-or-later
//! Debug output without secrets. Records that carry passwords, keys or codes cross the border
//! to the app, end up in error values and could be printed by a log line or a crash report.
//! Their `Debug` shows only the harmless fields; secret ones never appear.

/// Implements `Debug` for a record, showing only the listed (non-secret) fields.
macro_rules! redacted_debug {
    ($t:ident { $($shown:ident),* }) => {
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($t))$(.field(stringify!($shown), &self.$shown))*.finish_non_exhaustive()
            }
        }
    };
}
pub(crate) use redacted_debug;
