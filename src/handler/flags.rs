//! The flag sets a handler declares during option negotiation.

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use crate::protocol::*;

macro_rules! flag_set {
    ($(#[$doc:meta])* $name:ident { $($(#[$flag_doc:meta])* $flag:ident = $bit:expr,)* }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(u32);

        impl $name {
            $($(#[$flag_doc])* pub const $flag: $name = $name($bit);)*

            /// Every flag of the set.
            pub const ALL: $name = $name(0 $(| $bit)*);

            /// Whether every flag in `other` is set in `self`.
            pub const fn contains(self, other: $name) -> bool {
                self.0 & other.0 == other.0
            }

            /// Whether no flag is set.
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// The flags in `self` that are not in `other`.
            pub const fn difference(self, other: $name) -> $name {
                $name(self.0 & !other.0)
            }
        }

        impl BitOr for $name {
            type Output = $name;

            fn bitor(self, other: $name) -> $name {
                $name(self.0 | other.0)
            }
        }

        impl BitOrAssign for $name {
            fn bitor_assign(&mut self, other: $name) {
                self.0 |= other.0;
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let names: &[(&str, $name)] = &[$((stringify!($flag), $name::$flag),)*];
                let mut set: Vec<&str> = Vec::new();
                for (name, flag) in names {
                    if self.contains(*flag) {
                        set.push(*name);
                    }
                }
                write!(f, "{}({})", stringify!($name), set.join(" | "))
            }
        }
    };
}

flag_set! {
    /// The protocol stages a handler receives. `end_of_message`, `abort`, and
    /// `session_end` are always delivered.
    ///
    /// Each flag's value is the `SMFIP_NO*` flag the library requests when
    /// the stage is left out.
    Events {
        /// `connect`
        CONNECT = SMFIP_NOCONNECT,
        /// `helo`
        HELO = SMFIP_NOHELO,
        /// `mail`
        MAIL = SMFIP_NOMAIL,
        /// `rcpt`
        RCPT = SMFIP_NORCPT,
        /// `data`
        DATA = SMFIP_NODATA,
        /// `header`
        HEADER = SMFIP_NOHDRS,
        /// `end_of_headers`
        END_OF_HEADERS = SMFIP_NOEOH,
        /// `body`
        BODY = SMFIP_NOBODY,
        /// `unknown`
        UNKNOWN = SMFIP_NOUNKNOWN,
    }
}

flag_set! {
    /// The message modifications a handler may perform at end of message.
    /// Each flag's value is the matching `SMFIF_*` flag.
    Actions {
        /// `add_header` and `insert_header`
        ADD_HEADERS = SMFIF_ADDHDRS,
        /// `change_header` and `delete_header`
        CHANGE_HEADERS = SMFIF_CHGHDRS,
        /// `add_recipient`
        ADD_RECIPIENTS = SMFIF_ADDRCPT,
        /// `add_recipient_with_args`
        ADD_RECIPIENTS_WITH_ARGS = SMFIF_ADDRCPT_PAR,
        /// `delete_recipient`
        DELETE_RECIPIENTS = SMFIF_DELRCPT,
        /// `change_from`
        CHANGE_FROM = SMFIF_CHGFROM,
        /// `replace_body`
        CHANGE_BODY = SMFIF_CHGBODY,
        /// `quarantine`
        QUARANTINE = SMFIF_QUARANTINE,
    }
}

impl Events {
    /// The `SMFIP_NO*` flags for the stages in `self`.
    pub(crate) const fn no_flags(self) -> u32 {
        self.0
    }
}

impl Actions {
    /// No action.
    pub const NONE: Actions = Actions(0);

    /// The `SMFIF_*` flags.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The actions among the given `SMFIF_*` flags; other bits are dropped.
    pub const fn from_bits_truncate(bits: u32) -> Actions {
        Actions(bits & Actions::ALL.0)
    }
}
