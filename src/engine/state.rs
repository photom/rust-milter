//! libmilter's protocol states and transition rules (`next_states`,
//! `trans_ok`, and `fix_stm` in `libmilter/engine.c`).

use crate::protocol::*;

/// A protocol state, numbered as in libmilter.
///
/// libmilter's `ST_ABRT` is not modelled: it allows no transition of its
/// own, so every command after an abort goes through the recovery from
/// `ST_HELO`. The engine moves to `Helo` directly instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Init = 0,
    Opts = 1,
    Conn = 2,
    Helo = 3,
    Mail = 4,
    Rcpt = 5,
    Data = 6,
    Hdrs = 7,
    Eohs = 8,
    Body = 9,
    Endm = 10,
    Quit = 11,
    Abrt = 12,
    Unkn = 13,
    QuitNc = 14,
}

const fn mask(states: &[State]) -> u16 {
    let mut bits = 0;
    let mut i = 0;
    while i < states.len() {
        bits |= 1 << states[i] as u16;
        i += 1;
    }
    bits
}

use State::*;

/// The states each state may move to, indexed by state.
const NEXT: [u16; 15] = [
    mask(&[Opts]),
    mask(&[Conn, Unkn]),
    mask(&[Helo, Mail, Unkn]),
    mask(&[Helo, Mail, Unkn]),
    mask(&[Rcpt, Abrt, Unkn]),
    mask(&[Hdrs, Eohs, Data, Body, Endm, Rcpt, Abrt, Unkn]),
    mask(&[Eohs, Hdrs, Abrt]),
    mask(&[Eohs, Hdrs, Abrt]),
    mask(&[Body, Endm, Abrt]),
    mask(&[Endm, Body, Abrt]),
    mask(&[Quit, Mail, Unkn, QuitNc]),
    0,
    0,
    mask(&[Helo, Mail, Rcpt, Abrt, Data, Body, Unkn, Quit, QuitNc]),
    mask(&[Conn, Unkn]),
];

/// The transition rules of one connection: libmilter's table, with the
/// stages the handler switched off marked as skippable.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Transitions {
    skip: u16,
}

impl Transitions {
    /// The rules before option negotiation: no stage is skipped.
    pub(crate) const fn new() -> Self {
        Transitions { skip: 0 }
    }

    /// The rules for the agreed `SMFIP_*` flags, as `fix_stm` sets them.
    pub(crate) fn for_protocol(protocol: u32) -> Self {
        let skipped = [
            (SMFIP_NOCONNECT, Conn),
            (SMFIP_NOHELO, Helo),
            (SMFIP_NOMAIL, Mail),
            (SMFIP_NORCPT, Rcpt),
            (SMFIP_NOHDRS, Hdrs),
            (SMFIP_NOEOH, Eohs),
            (SMFIP_NOBODY, Body),
            (SMFIP_NODATA, Data),
            (SMFIP_NOUNKNOWN, Unkn),
        ];
        let mut skip = 0;
        for (flag, state) in skipped {
            if protocol & flag != 0 {
                skip |= 1 << state as u16;
            }
        }
        Transitions { skip }
    }

    /// Whether `from` may move to `to`, skipping the following states that
    /// are switched off, as `trans_ok` does. Skipping never leaves the
    /// sequential states `ST_CONN` to `ST_BODY`.
    pub(crate) fn allows(self, from: State, to: State) -> bool {
        let mut state = from as usize;
        loop {
            if NEXT[state] & (1 << to as u16) != 0 {
                return true;
            }
            let next = state + 1;
            if next > Body as usize || self.skip & (1 << next) == 0 {
                return false;
            }
            state = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_switched_off_stage_is_skipped() {
        let rules = Transitions::for_protocol(SMFIP_NOMAIL);
        assert!(rules.allows(Helo, Rcpt));
        assert!(!Transitions::new().allows(Helo, Rcpt));
    }

    #[test]
    fn quit_new_connection_is_allowed_only_after_end_of_message_and_unknown() {
        let rules = Transitions::for_protocol(SMFI_CURR_PROT);
        let mut allowed = Vec::new();
        for from in [
            Init, Opts, Conn, Helo, Mail, Rcpt, Data, Hdrs, Eohs, Body, Endm, Unkn, QuitNc,
        ] {
            if rules.allows(from, QuitNc) {
                allowed.push(from);
            }
        }
        assert_eq!(allowed, [Endm, Unkn]);
    }
}
