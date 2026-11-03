//! The macros the MTA sent in the current session.

use crate::MilterText;
use crate::protocol::*;

/// The stage bytes in protocol order. Postfix also sends `L` for headers
/// and `B` for the body.
const ORDER: [u8; 9] = [
    SMFIC_CONNECT,
    SMFIC_HELO,
    SMFIC_MAIL,
    SMFIC_RCPT,
    SMFIC_DATA,
    SMFIC_HEADER,
    SMFIC_EOH,
    SMFIC_BODY,
    SMFIC_BODYEOB,
];

/// Macro values per stage byte.
#[derive(Debug, Default)]
pub(crate) struct Macros {
    stages: Vec<(u8, Vec<(MilterText, MilterText)>)>,
}

impl Macros {
    /// Stores the macros of one stage, replacing that stage's earlier ones.
    /// Macros of a byte that is no stage are dropped: no lookup reaches
    /// them, and keeping them would let an MTA fill memory with up to 256
    /// stages.
    pub(crate) fn set(&mut self, stage: u8, macros: Vec<(MilterText, MilterText)>) {
        if stage != SMFIC_UNKNOWN && !ORDER.contains(&stage) {
            return;
        }
        match self.stages.iter_mut().find(|(s, _)| *s == stage) {
            Some((_, old)) => *old = macros,
            None => self.stages.push((stage, macros)),
        }
    }

    /// Looks `name` up in `stage` and the earlier stages, the latest first.
    /// Without a stage, or for `U`, it looks in every stage, starting with
    /// `U`.
    pub(crate) fn get(&self, name: &str, stage: Option<u8>) -> Option<&MilterText> {
        let position = stage.and_then(|stage| ORDER.iter().position(|&s| s == stage));
        let searched: &[u8] = match position {
            Some(index) => &ORDER[..=index],
            None => &ORDER,
        };
        let unknown = position.is_none().then_some(SMFIC_UNKNOWN);
        let stages = unknown.into_iter().chain(searched.iter().rev().copied());
        stages
            .filter_map(|stage| self.stage(stage))
            .find_map(|macros| find(macros, name))
    }

    fn stage(&self, stage: u8) -> Option<&[(MilterText, MilterText)]> {
        self.stages
            .iter()
            .find(|(s, _)| *s == stage)
            .map(|(_, macros)| macros.as_slice())
    }
}

/// Finds a macro by name. As in libmilter, `{x}` and `x` name the same
/// one-letter macro.
fn find<'a>(macros: &'a [(MilterText, MilterText)], name: &str) -> Option<&'a MilterText> {
    let bare = name
        .strip_prefix('{')
        .and_then(|n| n.strip_suffix('}'))
        .filter(|n| n.len() == 1);
    macros.iter().find_map(|(stored, value)| {
        let stored = stored.as_bytes();
        let matches = stored == name.as_bytes()
            || bare.is_some_and(|bare| stored == bare.as_bytes())
            || (name.len() == 1 && stored == format!("{{{name}}}").as_bytes());
        matches.then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, value: &str) -> (MilterText, MilterText) {
        let mut data = bytes::Bytes::from(format!("{name}\0{value}\0"));
        let name = MilterText::take(&mut data).unwrap();
        (name, MilterText::take(&mut data).unwrap())
    }

    #[test]
    fn eng_047_stores_no_macros_for_a_byte_that_is_no_stage() {
        let mut macros = Macros::default();

        macros.set(b'Z', vec![pair("{x}", "1")]);
        macros.set(SMFIC_CONNECT, vec![pair("{x}", "2")]);

        assert_eq!(macros.stages.len(), 1);
        assert_eq!(macros.get("{x}", None).unwrap().as_bytes(), b"2");
    }
}
