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
        for (stored, old) in &mut self.stages {
            if *stored == stage {
                *old = macros;
                return;
            }
        }
        self.stages.push((stage, macros));
    }

    /// Looks `name` up in `stage` and the earlier stages, the latest first.
    /// Without a stage, or for `U`, it looks in every stage, starting with
    /// `U`.
    pub(crate) fn get(&self, name: &str, stage: Option<u8>) -> Option<&MilterText> {
        let position = match stage {
            Some(stage) => order_position(stage),
            None => None,
        };
        let searched: &[u8] = match position {
            Some(index) => &ORDER[..=index],
            None => {
                if let Some(value) = self.lookup(SMFIC_UNKNOWN, name) {
                    return Some(value);
                }
                &ORDER
            }
        };
        for stage in searched.iter().rev() {
            if let Some(value) = self.lookup(*stage, name) {
                return Some(value);
            }
        }
        None
    }

    /// Looks `name` up in one stage.
    fn lookup(&self, stage: u8, name: &str) -> Option<&MilterText> {
        for (stored, macros) in &self.stages {
            if *stored == stage {
                return find(macros, name);
            }
        }
        None
    }
}

/// Finds a macro by name. As in libmilter, `{x}` and `x` name the same
/// one-letter macro.
fn find<'a>(macros: &'a [(MilterText, MilterText)], name: &str) -> Option<&'a MilterText> {
    let bare = one_letter(name);
    for (stored, value) in macros {
        if same_macro(stored.as_bytes(), name, bare) {
            return Some(value);
        }
    }
    None
}

/// The letter inside `{x}`, when `name` is a one-letter macro in braces.
fn one_letter(name: &str) -> Option<&str> {
    let inner = name.strip_prefix('{')?;
    let inner = inner.strip_suffix('}')?;
    if inner.len() == 1 { Some(inner) } else { None }
}

/// Whether a stored macro name is `name`, or the same one-letter macro with
/// or without braces. `bare` is the letter when `name` is written `{x}`.
fn same_macro(stored: &[u8], name: &str, bare: Option<&str>) -> bool {
    if stored == name.as_bytes() {
        return true;
    }
    if let Some(bare) = bare {
        if stored == bare.as_bytes() {
            return true;
        }
    }
    name.len() == 1 && stored == format!("{{{name}}}").as_bytes()
}

/// The position of a stage byte in [`ORDER`].
fn order_position(stage: u8) -> Option<usize> {
    for (index, known) in ORDER.iter().enumerate() {
        if *known == stage {
            return Some(index);
        }
    }
    None
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
