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
    /// The protocol stages a handler receives, from
    /// [`Handler::events`](crate::Handler::events). One flag per optional
    /// stage. `end_of_message`, `abort`, and `session_end` are always
    /// delivered.
    ///
    /// Each flag's value is the `SMFIP_NO*` flag the library requests when
    /// the stage is left out.
    Events {
        /// [`Handler::connect`](crate::Handler::connect); left out, the library
        /// requests `SMFIP_NOCONNECT`.
        CONNECT = SMFIP_NOCONNECT,
        /// [`Handler::helo`](crate::Handler::helo); left out, the library
        /// requests `SMFIP_NOHELO`.
        HELO = SMFIP_NOHELO,
        /// [`Handler::mail`](crate::Handler::mail); left out, the library
        /// requests `SMFIP_NOMAIL`.
        MAIL = SMFIP_NOMAIL,
        /// [`Handler::rcpt`](crate::Handler::rcpt); left out, the library
        /// requests `SMFIP_NORCPT`.
        RCPT = SMFIP_NORCPT,
        /// [`Handler::data`](crate::Handler::data); left out, the library
        /// requests `SMFIP_NODATA`.
        DATA = SMFIP_NODATA,
        /// [`Handler::header`](crate::Handler::header); left out, the library
        /// requests `SMFIP_NOHDRS`.
        HEADER = SMFIP_NOHDRS,
        /// [`Handler::end_of_headers`](crate::Handler::end_of_headers); left
        /// out, the library requests `SMFIP_NOEOH`.
        END_OF_HEADERS = SMFIP_NOEOH,
        /// [`Handler::body`](crate::Handler::body); left out, the library
        /// requests `SMFIP_NOBODY`.
        BODY = SMFIP_NOBODY,
        /// [`Handler::unknown`](crate::Handler::unknown); left out, the library
        /// requests `SMFIP_NOUNKNOWN`.
        UNKNOWN = SMFIP_NOUNKNOWN,
    }
}

flag_set! {
    /// The message modifications a handler may perform at end of message,
    /// from [`Handler::actions`](crate::Handler::actions). One flag for each
    /// modification method of [`EomContext`](crate::EomContext).
    ///
    /// Each flag's value is the matching `SMFIF_*` flag, as listed in
    /// [milter-protocol.md: Action flags].
    ///
    /// [milter-protocol.md: Action flags]: https://github.com/photom/rust-milter/blob/main/docs/reference/milter-protocol.md#action-flags-smfif_
    Actions {
        /// [`add_header`](crate::EomContext::add_header) and
        /// [`insert_header`](crate::EomContext::insert_header)
        /// (`SMFIF_ADDHDRS`)
        ADD_HEADERS = SMFIF_ADDHDRS,
        /// [`change_header`](crate::EomContext::change_header) and
        /// [`delete_header`](crate::EomContext::delete_header)
        /// (`SMFIF_CHGHDRS`)
        CHANGE_HEADERS = SMFIF_CHGHDRS,
        /// [`add_recipient`](crate::EomContext::add_recipient)
        /// (`SMFIF_ADDRCPT`)
        ADD_RECIPIENTS = SMFIF_ADDRCPT,
        /// [`add_recipient_with_args`](crate::EomContext::add_recipient_with_args)
        /// (`SMFIF_ADDRCPT_PAR`)
        ADD_RECIPIENTS_WITH_ARGS = SMFIF_ADDRCPT_PAR,
        /// [`delete_recipient`](crate::EomContext::delete_recipient)
        /// (`SMFIF_DELRCPT`)
        DELETE_RECIPIENTS = SMFIF_DELRCPT,
        /// [`change_from`](crate::EomContext::change_from) (`SMFIF_CHGFROM`)
        CHANGE_FROM = SMFIF_CHGFROM,
        /// [`replace_body`](crate::EomContext::replace_body)
        /// (`SMFIF_CHGBODY`)
        CHANGE_BODY = SMFIF_CHGBODY,
        /// [`quarantine`](crate::EomContext::quarantine)
        /// (`SMFIF_QUARANTINE`)
        QUARANTINE = SMFIF_QUARANTINE,
    }
}

flag_set! {
    /// The protocol options a handler requests during option negotiation,
    /// from [`Handler::protocol_options`](crate::Handler::protocol_options).
    /// One flag per option; [`ProtocolOptions::NONE`] requests none.
    ///
    /// Each flag's value is the matching `SMFIP_*` flag. The connection
    /// closes if the MTA does not offer all of them at protocol version 6.
    ///
    /// Without [`HEADER_LEADING_SPACE`](ProtocolOptions::HEADER_LEADING_SPACE),
    /// both MTAs remove one leading space (not a tab) from each value they
    /// send, and put one space in front of each value they store for
    /// `add_header`, `insert_header`, and `change_header`
    /// ([milter-protocol.md: Headers and body]). [Whitespace in header values]
    /// shows examples of both directions, and
    /// [Keep the leading space of header values] shows how to request the
    /// option.
    ///
    /// [milter-protocol.md: Headers and body]: https://github.com/photom/rust-milter/blob/main/docs/reference/milter-protocol.md#headers-and-body
    /// [Whitespace in header values]: https://github.com/photom/rust-milter/blob/main/docs/explanation/header-whitespace.md
    /// [Keep the leading space of header values]: https://github.com/photom/rust-milter/blob/main/docs/how-to/keep-the-leading-space-of-header-values.md
    ProtocolOptions {
        /// `SMFIP_HDR_LEADSPC`. While it is in force,
        /// [`Handler::header`](crate::Handler::header) receives each value as
        /// the message has it after the colon, leading whitespace included.
        /// `add_header`, `insert_header`, and `change_header` send each value
        /// as given, and the MTA stores it with no space added, so a value
        /// without a leading space produces `Name:value` in the message.
        HEADER_LEADING_SPACE = SMFIP_HDR_LEADSPC,
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

impl ProtocolOptions {
    /// No option.
    pub const NONE: ProtocolOptions = ProtocolOptions(0);

    /// The `SMFIP_*` flags.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The options among the given `SMFIP_*` flags; other bits are dropped.
    pub(crate) const fn from_bits_truncate(bits: u32) -> ProtocolOptions {
        ProtocolOptions(bits & ProtocolOptions::ALL.0)
    }
}
